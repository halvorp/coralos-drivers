// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the ported interrupt path.
//!
//! The ORDER is asserted as register traffic, not inferred from a return value. Acknowledging before
//! masking reopens exactly the window the mask exists to close, and that failure is invisible to any
//! test that only checks what the function returned.

use r8169_core::init::Bus;
use r8169_core::irq::{self, events, Verdict};
use r8169_core::regs;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Op {
    R16(u32, u16),
    W16(u32, u16),
}

struct Fake {
    log: Vec<Op>,
    status: u16,
}

impl Fake {
    fn new(status: u16) -> Self {
        Fake { log: Vec::new(), status }
    }
}

impl Bus for Fake {
    fn r8(&mut self, _reg: u32) -> u8 { 0 }
    fn w8(&mut self, _reg: u32, _val: u8) {}
    fn r16(&mut self, reg: u32) -> u16 {
        let v = if reg == regs::INTR_STATUS { self.status } else { 0 };
        self.log.push(Op::R16(reg, v));
        v
    }
    fn w16(&mut self, reg: u32, val: u16) {
        self.log.push(Op::W16(reg, val));
    }
    fn delay_us(&mut self, _us: u32) {}
}

/// The event enum, r8169_main.c:455-465.
#[test]
fn the_interrupt_bits_match_linux() {
    assert_eq!(events::SYS_ERR, 0x8000);
    assert_eq!(events::PCS_TIMEOUT, 0x4000);
    assert_eq!(events::SW_INT, 0x0100);
    assert_eq!(events::TX_DESC_UNAVAIL, 0x0080);
    assert_eq!(events::RX_FIFO_OVER, 0x0040);
    assert_eq!(events::LINK_CHG, 0x0020);
    assert_eq!(events::RX_OVERFLOW, 0x0010);
    assert_eq!(events::TX_ERR, 0x0008);
    assert_eq!(events::TX_OK, 0x0004);
    assert_eq!(events::RX_ERR, 0x0002);
    assert_eq!(events::RX_OK, 0x0001);
    // No two events share a bit.
    let all = [
        events::SYS_ERR, events::PCS_TIMEOUT, events::SW_INT, events::TX_DESC_UNAVAIL,
        events::RX_FIFO_OVER, events::LINK_CHG, events::RX_OVERFLOW, events::TX_ERR,
        events::TX_OK, events::RX_ERR, events::RX_OK,
    ];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_eq!(a & b, 0, "two interrupt bits overlap");
        }
    }
}

/// `(status & 0xffff) == 0xffff` (r8169_main.c:4855). A removed or powered-down PCI function reads
/// all-ones; it does not report every event at once. Servicing that as a real event word would have
/// the driver handle eleven imaginary conditions on a device that is not there.
#[test]
fn an_all_ones_status_means_the_device_is_gone_not_maximally_busy() {
    assert_eq!(irq::classify(0xffff, 0xffff), Verdict::DeviceGone);
    assert_eq!(irq::classify(0xffff, events::RX_OK), Verdict::DeviceGone);
    assert_ne!(irq::classify(0xffff, events::RX_OK), Verdict::Handle(events::RX_OK));
    // The ONLY case where the order of the two checks changes the answer: with no interrupts
    // enabled, an all-ones read is still a vanished device, not merely "not ours". Added after a
    // mutation that swapped the checks passed everything else — the suite could not tell them apart.
    assert_eq!(irq::classify(0xffff, 0), Verdict::DeviceGone, "gone, even with an empty mask");
}

/// A shared interrupt line: an event we did not ask for is not ours. (r8169_main.c:4855)
#[test]
fn an_unmasked_event_is_not_ours() {
    assert_eq!(irq::classify(events::SW_INT, events::RX_OK | events::TX_OK), Verdict::NotOurs);
    assert_eq!(irq::classify(0, events::RX_OK), Verdict::NotOurs);
}

#[test]
fn a_masked_event_is_ours_and_is_reported_masked() {
    let mask = events::RX_OK | events::TX_OK;
    // RX_OK plus an event outside the mask: only the masked bits come back.
    assert_eq!(irq::classify(events::RX_OK | events::SW_INT, mask), Verdict::Handle(events::RX_OK));
}

/// THE ORDER, asserted as traffic: read status, mask the line (IntrMask=0), THEN acknowledge
/// (IntrStatus=status). From rtl8169_interrupt (r8169_main.c:4850-4874), where the ack sits at the
/// `out:` label after rtl_irq_disable.
#[test]
fn a_real_interrupt_reads_then_masks_then_acks_in_that_order() {
    let status = events::RX_OK | events::TX_OK;
    let mut f = Fake::new(status);
    assert_eq!(irq::handle_interrupt(&mut f, 0xffff), Verdict::Handle(status));
    assert_eq!(
        f.log,
        vec![
            Op::R16(regs::INTR_STATUS, status),
            Op::W16(regs::INTR_MASK, 0),
            Op::W16(regs::INTR_STATUS, status),
        ],
        "read status, mask the line, THEN ack — acking first reopens the window the mask closes"
    );
}

/// The ack is WRITE-ONE-TO-CLEAR, so it must carry exactly the status that was read. Acking
/// something else either clears events never seen or leaves seen ones pending.
#[test]
fn the_ack_carries_exactly_the_status_that_was_read() {
    let status = events::RX_OK | events::LINK_CHG | events::SW_INT;
    let mut f = Fake::new(status);
    irq::handle_interrupt(&mut f, events::RX_OK); // narrow mask
    let acked: Vec<u16> = f.log.iter().filter_map(|o| match o {
        Op::W16(r, v) if *r == regs::INTR_STATUS => Some(*v),
        _ => None,
    }).collect();
    assert_eq!(acked, vec![status], "ack the FULL status read, not just the masked subset");
}

/// An interrupt that is not ours must touch NOTHING — no mask write, no ack. Acking another
/// device's event on a shared line loses it.
#[test]
fn an_interrupt_that_is_not_ours_writes_no_registers() {
    let mut f = Fake::new(events::SW_INT);
    assert_eq!(irq::handle_interrupt(&mut f, events::RX_OK), Verdict::NotOurs);
    assert!(
        !f.log.iter().any(|o| matches!(o, Op::W16(_, _))),
        "a foreign interrupt must not be masked or acked"
    );
}

/// Nor may a vanished device be written to.
#[test]
fn a_vanished_device_is_not_written_to() {
    let mut f = Fake::new(0xffff);
    assert_eq!(irq::handle_interrupt(&mut f, 0xffff), Verdict::DeviceGone);
    assert!(!f.log.iter().any(|o| matches!(o, Op::W16(_, _))));
}

/// enable/disable write the mask register, and only it. (r8169_main.c:1653, :1661)
#[test]
fn enable_and_disable_write_only_the_mask_register() {
    let mut f = Fake::new(0);
    irq::disable(&mut f);
    irq::enable(&mut f, events::RX_OK | events::TX_OK);
    assert_eq!(
        f.log,
        vec![
            Op::W16(regs::INTR_MASK, 0),
            Op::W16(regs::INTR_MASK, events::RX_OK | events::TX_OK),
        ]
    );
}
