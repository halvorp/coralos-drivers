// SPDX-License-Identifier: GPL-2.0-only
//! Interrupt handling, ported from Linux's r8169.
//!
//! Two idioms here are the kind a rewrite quietly drops, and both are ported deliberately: the
//! acknowledge happens LAST, and an all-ones status means the device is GONE rather than maximally
//! busy.

use crate::init::Bus;
use crate::regs;

/// Interrupt bits, from the event enum at r8169_main.c:455-465.
pub mod events {
    pub const SYS_ERR: u16 = 0x8000;
    pub const PCS_TIMEOUT: u16 = 0x4000;
    pub const SW_INT: u16 = 0x0100;
    pub const TX_DESC_UNAVAIL: u16 = 0x0080;
    pub const RX_FIFO_OVER: u16 = 0x0040;
    pub const LINK_CHG: u16 = 0x0020;
    pub const RX_OVERFLOW: u16 = 0x0010;
    pub const TX_ERR: u16 = 0x0008;
    pub const TX_OK: u16 = 0x0004;
    pub const RX_ERR: u16 = 0x0002;
    pub const RX_OK: u16 = 0x0001;
}

/// What an interrupt turned out to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Every bit set. On a shared PCI line this is a device that has VANISHED — a removed or
    /// powered-down function reads as all-ones, it does not report 0xffff of simultaneous events.
    /// Treating it as a real event word would have the driver service eleven imaginary conditions
    /// on a device that is not there.
    ///
    /// Linux does not distinguish this from "not ours": both are one condition returning IRQ_NONE
    /// (`if ((status & 0xffff) == 0xffff || !(status & tp->irq_mask))`, r8169_main.c:4855). This port
    /// reports them separately because "the device is gone" is worth acting on and "another device
    /// raised the line" is not — but that means the ORDER of the two checks only changes the answer
    /// when `irq_mask` is 0, and in Linux it cannot change the answer at all. An earlier version of
    /// this comment called the order load-bearing; a mutation that swapped the checks passed every
    /// test, which is how the overstatement was found.
    DeviceGone,
    /// Nothing we asked for. On a shared line another device raised it. (r8169_main.c:4855)
    NotOurs,
    /// Ours, with the masked event bits.
    Handle(u16),
}

/// Classify a status word against the driver's interrupt mask.
/// Ported from the opening of `rtl8169_interrupt` (r8169_main.c:4853-4856).
pub fn classify(status: u16, irq_mask: u16) -> Verdict {
    if status == 0xffff {
        return Verdict::DeviceGone;
    }
    if status & irq_mask == 0 {
        return Verdict::NotOurs;
    }
    Verdict::Handle(status & irq_mask)
}

/// Mask all interrupts: `RTL_W16(tp, IntrMask, 0)` (r8169_main.c:1653).
pub fn disable<B: Bus>(bus: &mut B) {
    bus.w16(regs::INTR_MASK, 0);
}

/// Restore the driver's mask: `RTL_W16(tp, IntrMask, tp->irq_mask)` (r8169_main.c:1661).
pub fn enable<B: Bus>(bus: &mut B, irq_mask: u16) {
    bus.w16(regs::INTR_MASK, irq_mask);
}

/// Acknowledge events: `RTL_W16(tp, IntrStatus, bits)` (r8169_main.c:1645). WRITE-ONE-TO-CLEAR —
/// the bits written are the bits cleared, so acking with anything other than the status just read
/// either clears events that were never seen or leaves seen ones pending.
pub fn ack<B: Bus>(bus: &mut B, bits: u16) {
    bus.w16(regs::INTR_STATUS, bits);
}

/// Read the pending event word: 16-bit, from IntrStatus.
pub fn read_events<B: Bus>(bus: &mut B) -> u16 {
    bus.r16(regs::INTR_STATUS)
}

/// One interrupt, in Linux's order.
///
/// From `rtl8169_interrupt` (r8169_main.c:4850-4874): read status, classify, and on a real
/// interrupt MASK the line before scheduling the poll, then ACK LAST.
///
/// THE ACK IS LAST AND THAT ORDER IS LOAD-BEARING. Acking before the line is masked reopens the
/// window the mask exists to close: an event arriving between the ack and the mask is cleared
/// without being handled and never raises again, which presents as a NIC that stops receiving under
/// load rather than as an error. Linux acks at the `out:` label reached by BOTH the normal path and
/// the SYSErr early exit — every path that takes the interrupt also acks it.
///
/// Returns the verdict so a caller can drive its own poll; this crate does not own a scheduler.
pub fn handle_interrupt<B: Bus>(bus: &mut B, irq_mask: u16) -> Verdict {
    let status = read_events(bus);
    let verdict = classify(status, irq_mask);
    match verdict {
        Verdict::DeviceGone | Verdict::NotOurs => verdict,
        Verdict::Handle(_) => {
            disable(bus);
            ack(bus, status);
            verdict
        }
    }
}
