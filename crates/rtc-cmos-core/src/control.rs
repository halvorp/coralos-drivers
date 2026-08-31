// SPDX-License-Identifier: GPL-2.0-only
//! Register-B interrupt controls and register-C acknowledgement semantics.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c:99-109,343-399,691-740,1043-1069`; original
//! copyright Paul Gortmaker, David Brownell, and the Linux RTC authors.

use rtc_mc146818_core::registers::{AF, AIE, HOUR_24, IRQF, PF, PIE, UF, UIE};

pub const IRQ_MASK: u8 = PF | AF | UF; // drivers/rtc/rtc-cmos.c:103
pub const DEFAULT_PERIODIC_FREQUENCY_HZ: u32 = 1024; // drivers/rtc/rtc-cmos.c:1045-1055
pub const DEFAULT_FREQUENCY_SELECT: u8 = 0x20 | 0x06; // drivers/rtc/rtc-cmos.c:1055

/// The register-B interrupt enable controls Linux manipulates.
pub const INTERRUPT_ENABLE_BITS: [(&str, u8); 3] = [
    ("RTC_PIE", PIE),
    ("RTC_AIE", AIE),
    ("RTC_UIE", UIE),
]; // drivers/rtc/rtc-cmos.c:103,1058-1060

/// Set selected register-B interrupt enable bits.
pub const fn enable_irqs(control_b: u8, mask: u8) -> u8 {
    control_b | mask
} // drivers/rtc/rtc-cmos.c:367-371

/// Clear selected register-B interrupt enable bits.
pub const fn disable_irqs(control_b: u8, mask: u8) -> u8 {
    control_b & !mask
} // drivers/rtc/rtc-cmos.c:387-389

/// Filter register C against enabled register-B sources while retaining the summary flag.
pub const fn filter_irq_status(intr_c: u8, control_b: u8) -> u8 {
    intr_c & ((control_b & IRQ_MASK) | IRQF)
} // drivers/rtc/rtc-cmos.c:355,717-720

/// Linux reports an RTC interrupt only when IRQF and at least one enabled source are both present.
pub const fn interrupt_sources(intr_c: u8) -> u8 {
    if intr_c & IRQF == 0 { 0 } else { intr_c & IRQ_MASK }
} // drivers/rtc/rtc-cmos.c:105-109

/// Register-B values after handling register C. Alarm interrupts are one-shot in Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptResult {
    pub status: u8,
    pub control_b: u8,
    pub suspend_control: u8,
    pub handled: bool,
}

/// Apply interrupt filtering and Linux's one-shot AIE clearing rule.
pub const fn handle_interrupt(
    intr_c: u8,
    control_b: u8,
    suspend_control: u8,
) -> InterruptResult {
    let effective_control = if suspend_control == 0 { control_b } else { suspend_control };
    let status = filter_irq_status(intr_c, effective_control);
    let alarm = status & AIE != 0;
    let next_control = if alarm { control_b & !AIE } else { control_b };
    let next_suspend = if alarm { suspend_control & !AIE } else { suspend_control };
    InterruptResult {
        status,
        control_b: next_control,
        suspend_control: next_suspend,
        handled: interrupt_sources(status) != 0,
    }
} // drivers/rtc/rtc-cmos.c:714-740

/// Whether Linux accepts the RTC's mode when a valid IRQ makes alarm operations available.
pub const fn supports_required_24_hour_mode(valid_irq: bool, control_b: u8) -> bool {
    !valid_irq || control_b & HOUR_24 != 0
} // drivers/rtc/rtc-cmos.c:1066-1069
