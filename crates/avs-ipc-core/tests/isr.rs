// SPDX-License-Identifier: GPL-2.0-only
//! avs_skl_dsp_interrupt / avs_skl_ipc_interrupt (skl.c:18-76) against a fake register file.
use avs_ipc_core::isr::*;

/// Run one interrupt. `regs` answers reads by offset; returns the ops and the final handled flag.
fn run(adspis: u32, ack: u32, rsp: u32, hipct: u32, hipcte: u32, sd_sts: u8) -> (Vec<IsrOp>, bool) {
    let mut isr = SklIsr::new();
    let mut ops = Vec::new();
    let mut out = isr.next(IsrInput::Begin);
    loop {
        match out {
            IsrOutput::Finished { handled } => return (ops, handled),
            IsrOutput::Protocol => panic!("protocol failure after {ops:?}"),
            IsrOutput::Do(op) => {
                ops.push(op);
                assert!(ops.len() <= MAX_OPS, "more than MAX_OPS: {ops:?}");
                let first_rsp_read = ops.iter().filter(|o| matches!(o, IsrOp::Read32 { off: 0x40 })).count();
                let input = match op {
                    IsrOp::Read32 { off: 0x0C } => IsrInput::Value(adspis),
                    IsrOp::Read32 { off: 0x4C } => IsrInput::Value(ack),
                    IsrOp::Read32 { off: 0x40 } => IsrInput::Value(if first_rsp_read == 1 { rsp } else { hipct }),
                    IsrOp::Read32 { off: 0x44 } => IsrInput::Value(hipcte),
                    IsrOp::Read8 { off: 0x83 } => IsrInput::Value(sd_sts as u32),
                    IsrOp::Read32 { .. } | IsrOp::Read8 { .. } => panic!("unexpected read {op:?}"),
                    IsrOp::Update32 { .. } | IsrOp::Signal(_) => IsrInput::Done,
                };
                out = isr.next(input);
            }
        }
    }
}

const IS: IsrOp = IsrOp::Read32 { off: 0x0C };
const MASK_CTL: IsrOp = IsrOp::Update32 { off: 0x50, mask: 3, value: 0 };
const UNMASK_CTL: IsrOp = IsrOp::Update32 { off: 0x50, mask: 3, value: 3 };

#[test]
fn a_dead_device_is_irq_none_after_one_read() {
    // skl.c:62-63
    let (ops, handled) = run(u32::MAX, 0, 0, 0, 0, 0);
    assert_eq!(ops, vec![IS]);
    assert!(!handled);
}

#[test]
fn nothing_pending_is_irq_none() {
    let (ops, handled) = run(0, 0, 0, 0, 0, 0);
    assert_eq!(ops, vec![IS]);
    assert!(!handled);
    // an unrelated bit
    let (ops, handled) = run(0x4, 0, 0, 0, 0, 0);
    assert_eq!(ops, vec![IS]);
    assert!(!handled);
}

#[test]
fn ipc_interrupt_with_nothing_to_service_still_masks_and_unmasks() {
    let (ops, handled) = run(1, 0, 0, 0, 0, 0);
    assert_eq!(ops, vec![IS, MASK_CTL, IsrOp::Read32 { off: 0x4C }, IsrOp::Read32 { off: 0x40 }, UNMASK_CTL]);
    assert!(handled);
}

#[test]
fn the_dsp_acking_a_request_completes_it_and_acks_back() {
    // skl.c:30-36: ack bit is HIPCIE bit 30
    let (ops, handled) = run(1, 0x4000_0000, 0, 0, 0, 0);
    assert_eq!(
        ops,
        vec![
            IS,
            MASK_CTL,
            IsrOp::Read32 { off: 0x4C },
            IsrOp::Read32 { off: 0x40 },
            IsrOp::Signal(Signal::TxDone),
            IsrOp::Update32 { off: 0x4C, mask: 0x4000_0000, value: 0x4000_0000 },
            UNMASK_CTL,
        ]
    );
    assert!(handled);
}

#[test]
fn a_response_is_read_from_hipct_and_hipcte_signalled_and_acknowledged() {
    // skl.c:39-50: rsp bit is HIPCT bit 31; the message is HIPCTE:HIPCT
    let (ops, handled) = run(1, 0, 0x8000_0000, 0xAABB_CCDD, 0x1122_3344, 0);
    assert_eq!(
        ops,
        vec![
            IS,
            MASK_CTL,
            IsrOp::Read32 { off: 0x4C },
            IsrOp::Read32 { off: 0x40 },
            IsrOp::Read32 { off: 0x40 },
            IsrOp::Read32 { off: 0x44 },
            IsrOp::Signal(Signal::Response(0x1122_3344_AABB_CCDD)),
            IsrOp::Update32 { off: 0x40, mask: 0x8000_0000, value: 0x8000_0000 },
            UNMASK_CTL,
        ]
    );
    assert!(handled);
}

#[test]
fn ack_and_response_together_signal_the_ack_first() {
    let (ops, _) = run(1, 0x4000_0000, 0x8000_0000, 0x0000_0001, 0x0000_0002, 0);
    let sig: Vec<_> = ops.iter().filter_map(|o| if let IsrOp::Signal(s) = o { Some(*s) } else { None }).collect();
    assert_eq!(sig, vec![Signal::TxDone, Signal::Response(0x0000_0002_0000_0001)]);
    assert_eq!(ops.len(), MAX_OPS - 3, "IPC-only worst case: 11 ops");
}

#[test]
fn the_code_loader_interrupt_masks_itself_reads_the_status_and_signals() {
    // cldma.c:251-260: disable the CLDMA interrupt, read SD_STS, complete
    let (ops, handled) = run(2, 0, 0, 0, 0, 0x04);
    assert_eq!(
        ops,
        vec![IS, IsrOp::Update32 { off: 0x08, mask: 2, value: 0 }, IsrOp::Read8 { off: 0x83 }, IsrOp::Signal(Signal::CldmaIoc { sd_status: 0x04 })]
    );
    assert!(handled);
    let (ops, _) = run(2, 0, 0, 0, 0, 0x14);
    assert_eq!(ops[3], IsrOp::Signal(Signal::CldmaIoc { sd_status: 0x14 }));
}

#[test]
fn both_sources_are_serviced_in_one_interrupt_the_loader_first() {
    // skl.c:65-73
    let (ops, handled) = run(3, 0x4000_0000, 0x8000_0000, 0x11, 0x22, 0x04);
    assert_eq!(ops.len(), MAX_OPS);
    assert!(handled);
    assert!(matches!(ops[3], IsrOp::Signal(Signal::CldmaIoc { .. })));
    assert_eq!(ops[4], MASK_CTL);
    assert_eq!(*ops.last().unwrap(), UNMASK_CTL);
}

#[test]
fn whatever_the_hardware_says_the_interrupt_finishes_within_max_ops() {
    assert_eq!(MAX_OPS, 14);
    for adspis in [0u32, 1, 2, 3, 4, 0x8000_0003, u32::MAX] {
        for ack in [0u32, 0x4000_0000, 0xFFFF_FFFF] {
            for rsp in [0u32, 0x8000_0000, 0xFFFF_FFFF, 0x7FFF_FFFF] {
                let (ops, _) = run(adspis, ack, rsp, 0xFFFF_FFFF, 0xFFFF_FFFF, 0xFF);
                assert!(ops.len() <= MAX_OPS);
            }
        }
    }
}

#[test]
fn a_wrong_answer_is_a_protocol_failure_not_a_panic() {
    let mut isr = SklIsr::new();
    assert_eq!(isr.next(IsrInput::Done), IsrOutput::Protocol); // not begun
    let mut isr = SklIsr::new();
    assert!(matches!(isr.next(IsrInput::Begin), IsrOutput::Do(_)));
    assert_eq!(isr.next(IsrInput::Done), IsrOutput::Protocol); // asked for a read, told "done"
}
