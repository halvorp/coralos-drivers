// SPDX-License-Identifier: GPL-2.0-only
//! The memory-latency mailbox read: two single-attempt `GEN9_PCODE_READ_MEM_LATENCY` exchanges on `display-clock-core`'s session.
use i915_common_core::Reg;
use i915_display_clock_core::{AttemptOutcome, ClockError};
use i915_display_modeset_core::latency::*;
use i915_kbl_display_core::skl_wm::{adjust_latencies, latencies_from_pcode, DramFacts};

fn replied(data: u32) -> AttemptOutcome {
    AttemptOutcome::Replied { mailbox: 0, data }
}

#[test]
fn two_exchanges_then_the_words() {
    let mut r = MemLatencyRead::new();
    let a = r.attempt().unwrap();
    // intel_pcode.c __snb_pcode_rw: DATA = data0, DATA1 = 0, MAILBOX = READY | mbox
    assert_eq!(a.writes[0], (Reg::new(0x138128), 0));
    assert_eq!(a.writes[1], (Reg::new(0x13812c), 0));
    assert_eq!(a.writes[2], (Reg::new(0x138124), 0x8000_0006));
    assert_eq!(a.ready_poll.reg.offset(), 0x138124);
    assert_eq!((a.ready_poll.mask, a.ready_poll.want), (0x8000_0000, 0));
    assert_eq!(r.feed(replied(0x120a_0402), 100), ReadStep::Next);
    let b = r.attempt().unwrap();
    assert_eq!(b.writes[0], (Reg::new(0x138128), 1), "data0 = 1 asks for levels 4-7");
    assert_eq!(r.feed(replied(0x3c32_281e), 100), ReadStep::Done([0x120a_0402, 0x3c32_281e]));
    assert!(r.attempt().is_none());
    // and the words decode/adjust to the table the watermark tests use
    let mut l = latencies_from_pcode(0x120a_0402, 0x3c32_281e);
    adjust_latencies(&mut l, &DramFacts { symmetric_memory: false, has_16gb_dimms: false });
    assert_eq!(l, [2, 4, 10, 18, 30, 40, 50, 60]);
}

#[test]
fn the_ready_wait_is_bounded_at_500us_plus_20ms() {
    let a = MemLatencyRead::new().attempt().unwrap();
    assert_eq!(a.ready_poll.max_polls, READ_READY_TIMEOUT_US / READ_READY_INTERVAL_US + 1);
    assert_eq!(READ_READY_TIMEOUT_US, 500 + 20_000);
    let mut reads = 0;
    let r = a.ready_poll.run(
        || {
            reads += 1;
            assert!(reads <= 3000, "unbounded");
            0x8000_0000
        },
        |_| {},
    );
    assert!(r.is_err());
    assert_eq!(reads, a.ready_poll.max_polls);
}

#[test]
fn a_failed_exchange_fails_the_read_once_and_never_resends() {
    // busy mailbox: -EAGAIN in Linux; one attempt only (a read is not skl_pcode_request)
    let mut r = MemLatencyRead::new();
    assert!(matches!(r.feed(AttemptOutcome::Busy { mailbox: 0x8000_0006 }, 1), ReadStep::Failed(ClockError::PcodeBusy { .. })));
    assert!(r.attempt().is_none(), "after a failure no further exchange is offered");
    assert!(matches!(r.feed(replied(0), 1), ReadStep::Failed(_)), "and nothing is accepted afterwards");
    // READY never cleared
    let mut r = MemLatencyRead::new();
    assert!(matches!(r.feed(AttemptOutcome::MailboxTimeout { got: 0x8000_0006 }, 20_500), ReadStep::Failed(ClockError::HandshakeTimeout { .. })));
    // a non-zero STATUS (GEN7_PCODE_ILLEGAL_DATA = 3) fails the read even though READY cleared
    let mut r = MemLatencyRead::new();
    let e = match r.feed(AttemptOutcome::Replied { mailbox: 3, data: 0x1234 }, 5) {
        ReadStep::Failed(e) => e,
        other => panic!("{other:?}"),
    };
    assert_eq!(e.name(), "PCODE-REJECTED");
    // failure in the SECOND exchange also fails the whole read, with no partial result
    let mut r = MemLatencyRead::new();
    assert_eq!(r.feed(replied(1), 5), ReadStep::Next);
    assert!(matches!(r.feed(AttemptOutcome::Busy { mailbox: 0x8000_0006 }, 5), ReadStep::Failed(_)));
    assert!(r.attempt().is_none());
}

#[test]
fn a_reply_with_any_payload_acknowledges() {
    // reply_mask 0: the reply IS the data; even an all-zero latency word is a valid answer.
    let mut r = MemLatencyRead::new();
    assert_eq!(r.feed(replied(0), 1), ReadStep::Next);
    assert_eq!(r.feed(replied(0xffff_ffff), 1), ReadStep::Done([0, 0xffff_ffff]));
}
