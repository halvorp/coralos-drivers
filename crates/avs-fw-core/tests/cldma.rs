// SPDX-License-Identifier: GPL-2.0-only
//! hda_cldma_fill / hda_cldma_transfer / cldma_memcpy_work (cldma.c:60-122, :124-134) as planners.
mod common;
use avs_fw_core::cldma::*;
use avs_fw_core::effect::*;
use common::*;

const SPIB: Reg = Reg { space: Space::Dsp, off: 0xa8, width: Width::D32 };

#[test]
fn fill_without_wrap_copies_one_chunk_and_advances_spib() {
    // cldma.c:64-85. ring 100, spib 10, 30 bytes left, image position 70
    let p = fill_plan(10, 100, 30, 70).unwrap();
    assert_eq!(p.ncopies, 1);
    assert_eq!(p.copies[0], (10, 70, 30));
    assert_eq!(p.spib, 40);
    assert_eq!(p.consumed, 30);
}

#[test]
fn fill_is_capped_at_the_ring_size() {
    let p = fill_plan(0, 100, 1000, 0).unwrap();
    assert_eq!((p.ncopies, p.copies[0], p.spib, p.consumed), (1, (0, 0, 100), 100, 100)); // cldma.c:64-67
}

#[test]
fn fill_that_reaches_the_end_exactly_leaves_spib_at_the_ring_size() {
    // offset + size == buffer_size is NOT a wrap (cldma.c:70 uses `>`), SPIB is written as buffer_size
    let p = fill_plan(60, 100, 40, 5).unwrap();
    assert_eq!((p.ncopies, p.copies[0], p.spib), (1, (60, 5, 40), 100));
}

#[test]
fn fill_that_crosses_the_end_wraps_in_two_copies() {
    // cldma.c:70-79: ss = 100 - 80 = 20 to the tail, then 30 from offset 0
    let p = fill_plan(80, 100, 50, 7).unwrap();
    assert_eq!(p.ncopies, 2);
    assert_eq!(p.copies[0], (80, 7, 20));
    assert_eq!(p.copies[1], (0, 27, 30));
    assert_eq!(p.spib, 30);
    assert_eq!(p.consumed, 50);
}

#[test]
fn fill_with_spib_at_the_end_skips_the_empty_tail_copy() {
    // after a chunk that ended exactly at the ring end, SPIB == buffer_size: ss = 0, everything goes to the head
    let p = fill_plan(100, 100, 25, 0).unwrap();
    assert_eq!(p.ncopies, 1);
    assert_eq!(p.copies[0], (0, 0, 25));
    assert_eq!(p.spib, 25);
}

#[test]
fn fill_with_spib_beyond_the_ring_is_refused() {
    // Linux: ss = buffer_size - offset wraps to ~4 GiB and memcpy runs off the buffer
    assert_eq!(fill_plan(101, 100, 10, 0), None);
    assert!(fill_plan(100, 100, 10, 0).is_some());
}

/// A ring-buffer device model: SPIB value tracked, copies applied to a model ring, IOC after each start/refill.
struct Model {
    ring: Vec<u8>,
    spib: u32,
    image: Vec<u8>,
    sent: Vec<u8>,
}

fn drive_transfer(buffer: u32, image_len: u32, delay_ms: u32, hook: Option<Step>) -> (Vec<Step>, Output, Model) {
    let image: Vec<u8> = (0..image_len).map(|i| (i % 251) as u8).collect();
    let mut m = Model { ring: vec![0; buffer as usize], spib: 0, image: image.clone(), sent: Vec::new() };
    let mut t = Transfer::new(buffer, image_len, delay_ms, hook);
    let (log, out) = {
        let m = &mut m;
        run(
            |i| t.next(i),
            |s| match s.op {
                Op::Read { reg } if reg == SPIB => Input::Value(m.spib),
                Op::Write { reg, value } if reg == SPIB => {
                    m.spib = value;
                    Input::Done
                }
                Op::CopyToDma { dst_offset, src_offset, len } => {
                    for k in 0..len {
                        m.ring[(dst_offset + k) as usize] = m.image[(src_offset + k) as usize];
                    }
                    for k in 0..len {
                        m.sent.push(m.image[(src_offset + k) as usize]);
                    }
                    Input::Done
                }
                _ => ok_answer(s),
            },
            10_000,
        )
    };
    (log, out, m)
}

#[test]
fn a_small_image_is_one_fill_one_start_one_completion() {
    let (log, out, m) = drive_transfer(64, 40, 0, None);
    assert_eq!(out, Output::Finished);
    assert_eq!(
        labels(&log),
        vec!["cldma-spib-read", "cldma-copy", "cldma-spib-write", "cldma-irq-on", "cldma-run", "cldma-running", "cldma-ioc"]
    );
    assert_eq!(m.sent, m.image);
    // cldma.c:136-149: ADSPIC CLDMA on, SD_CTL INT_MASK|START on, poll START
    assert_eq!(log[3].op, Op::Update { reg: Reg { space: Space::Dsp, off: 0x08, width: Width::D32 }, mask: 2, value: 2 });
    assert_eq!(log[4].op, Op::Update { reg: Reg { space: Space::Dsp, off: 0x80, width: Width::B8 }, mask: 0x1E, value: 0x1E });
    match log[5].op {
        Op::Poll(p) => assert_eq!((p.reg.off, p.reg.width, p.mask, p.want, p.interval_us, p.timeout_us), (0x80, Width::B8, 2, 2, 3, 300)),
        o => panic!("{o:?}"),
    }
    assert_eq!(log[6].op, Op::Wait { event: Event::CldmaIoc, timeout_ms: 300 });
}

#[test]
fn an_image_larger_than_the_ring_is_refilled_after_each_completion_and_arrives_intact() {
    let (log, out, m) = drive_transfer(64, 200, 0, None); // 64 + 64 + 64 + 8
    assert_eq!(out, Output::Finished);
    assert_eq!(m.sent, m.image, "every byte arrives once, in order");
    let waits = log.iter().filter(|s| s.label == "cldma-ioc").count();
    assert_eq!(waits, 4);
    // the interrupt is re-enabled after each refill (cldma.c:118-120) but the run bit is set only once
    assert_eq!(log.iter().filter(|s| s.label == "cldma-irq-on").count(), 4);
    assert_eq!(log.iter().filter(|s| s.label == "cldma-run").count(), 1);
}

#[test]
fn start_delay_and_hook_come_between_the_first_fill_and_the_run() {
    // loader.c:201: msecs_to_jiffies(AVS_CLDMA_START_DELAY_MS); the IPC goes out while the worker waits
    let hook = Step::new("hook", Op::IpcSend(IpcCall::LoadLibrary { lib_id: 3 }), OnFail::Abort);
    let (log, out, _) = drive_transfer(64, 40, 100, Some(hook));
    assert_eq!(out, Output::Finished);
    assert_eq!(
        labels(&log),
        vec!["cldma-spib-read", "cldma-copy", "cldma-spib-write", "hook", "cldma-start-delay", "cldma-irq-on", "cldma-run", "cldma-running", "cldma-ioc"]
    );
    assert_eq!(log[4].op, Op::DelayUs(100_000));
}

#[test]
fn an_empty_image_transfers_nothing() {
    // hda_cldma_transfer: `if (!cl->remaining) return;`
    let (log, out, _) = drive_transfer(64, 0, 0, None);
    assert_eq!(out, Output::Finished);
    assert!(log.is_empty());
}

#[test]
fn a_completion_without_the_complete_bit_is_a_named_failure() {
    // cldma.c:105-110: `if (!(cl->sd_status & SD_INT_COMPLETE))` -> "cldma transfer error"
    let mut t = Transfer::new(64, 40, 0, None);
    for status in [0x00u8, 0x08, 0x10, 0x18] {
        let mut t2 = t.clone();
        let (_, out) = run(|i| t2.next(i), |s| if let Op::Wait { .. } = s.op { Input::IocStatus(status) } else { ok_answer(s) }, 100);
        assert_eq!(out, Output::Failed(Failure { step: "cldma-ioc", cause: Cause::CldmaTransferError { sd_status: status } }), "status {status:#x}");
    }
    // COMPLETE together with an error bit is still a completion (Linux tests only the one bit)
    let (_, out) = run(|i| t.next(i), |s| if let Op::Wait { .. } = s.op { Input::IocStatus(0x0C) } else { ok_answer(s) }, 100);
    assert_eq!(out, Output::Finished);
}

#[test]
fn an_ioc_timeout_is_a_named_failure() {
    // cldma.c:97-100
    let mut t = Transfer::new(64, 40, 0, None);
    let (_, out) = run(|i| t.next(i), |s| if let Op::Wait { .. } = s.op { Input::EventTimedOut } else { ok_answer(s) }, 100);
    assert_eq!(out, Output::Failed(Failure { step: "cldma-ioc", cause: Cause::EventTimeout }));
}

#[test]
fn the_engine_not_starting_is_a_named_failure() {
    // cldma.c:149 poll for SD_CTL_DMA_START
    let mut t = Transfer::new(64, 40, 0, None);
    let (_, out) = run(|i| t.next(i), |s| if let Op::Poll(_) = s.op { Input::PollTimedOut(0) } else { ok_answer(s) }, 100);
    assert_eq!(out, Output::Failed(Failure { step: "cldma-running", cause: Cause::PollTimeout { last: 0 } }));
}

#[test]
fn the_start_poll_is_bounded_by_the_planner_not_only_by_the_callers_clock() {
    let mut t = Transfer::new(64, 40, 0, None);
    let (log, out) = run(|i| t.next(i), |s| if let Op::Poll(_) = s.op { Input::Value(0) } else { ok_answer(s) }, 10_000);
    // 300us / 3us + 2 = 102 questions
    assert_eq!(out, Output::Failed(Failure { step: "cldma-running", cause: Cause::PollBound { reads: 102, last: 0 } }));
    assert_eq!(log.iter().filter(|s| s.label == "cldma-running").count(), 102);
}

#[test]
fn a_spib_beyond_the_ring_fails_the_transfer_by_name() {
    let mut t = Transfer::new(64, 40, 0, None);
    let (_, out) = run(|i| t.next(i), |s| if let Op::Read { .. } = s.op { Input::Value(65) } else { ok_answer(s) }, 100);
    assert_eq!(out, Output::Failed(Failure { step: "cldma-spib-read", cause: Cause::SpibBeyondBuffer { spib: 65, buffer_size: 64 } }));
}

#[test]
fn a_wrong_kind_of_answer_is_a_protocol_failure_not_a_panic() {
    let mut t = Transfer::new(64, 40, 0, None);
    assert!(matches!(t.next(Input::Begin), Output::Do(_)));
    // asked to Read the SPIB; answered with a completion
    assert_eq!(t.next(Input::EventFired), Output::Failed(Failure { step: "cldma-spib-read", cause: Cause::Protocol }));
    // and a transfer that was never begun
    let mut t = Transfer::new(64, 40, 0, None);
    assert!(matches!(t.next(Input::Done), Output::Failed(_)));
}

#[test]
fn fill_one_byte_to_the_end_still_copies_that_byte() {
    // ss == 1: the tail copy is one byte, the rest wraps (cldma.c:70-79)
    let p = fill_plan(99, 100, 10, 0).unwrap();
    assert_eq!(p.ncopies, 2);
    assert_eq!(p.copies[0], (99, 0, 1));
    assert_eq!(p.copies[1], (0, 1, 9));
    assert_eq!(p.spib, 9);
}

#[test]
fn fill_of_zero_or_one_byte_copies_nothing_or_one_byte() {
    let p = fill_plan(10, 100, 0, 0).unwrap();
    assert_eq!((p.ncopies, p.spib, p.consumed), (0, 10, 0));
    let p = fill_plan(10, 100, 1, 5).unwrap();
    assert_eq!((p.ncopies, p.copies[0], p.spib, p.consumed), (1, (10, 5, 1), 11, 1));
}

#[test]
fn a_one_millisecond_start_delay_is_still_a_delay() {
    let (log, _, _) = drive_transfer(64, 40, 1, None);
    assert!(log.iter().any(|s| s.op == Op::DelayUs(1000)));
    let (log, _, _) = drive_transfer(64, 40, 0, None);
    assert!(!log.iter().any(|s| s.label == "cldma-start-delay"));
}
