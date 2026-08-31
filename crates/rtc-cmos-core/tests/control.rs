// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for register-B/C control semantics.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c`; original copyright Paul Gortmaker, David Brownell,
//! and the Linux RTC authors.

use rtc_cmos_core::control::*;
use rtc_mc146818_core::registers::{AF, AIE, HOUR_24, IRQF, PF, PIE, UF, UIE};

/// rtc-cmos.c:103,1045-1055.
#[test]
fn control_literals_match_linux() {
    assert_eq!(IRQ_MASK, 0x70);
    assert_eq!(DEFAULT_PERIODIC_FREQUENCY_HZ, 1024);
    assert_eq!(DEFAULT_FREQUENCY_SELECT, 0x26);
}

/// rtc-cmos.c:103,1058-1060 has exactly three interrupt-enable controls. Names and values are
/// literal, not collected from the production table.
#[test]
fn all_three_interrupt_enable_names_are_pinned() {
    assert_eq!(INTERRUPT_ENABLE_BITS.len(), 3);
    assert_eq!(INTERRUPT_ENABLE_BITS, [("RTC_PIE", 0x40), ("RTC_AIE", 0x20), ("RTC_UIE", 0x10)]);
}

/// rtc-cmos.c:367-371,387-389 preserves unrelated register-B mode bits.
#[test]
fn enabling_and_disabling_only_changes_the_requested_bits() {
    assert_eq!(enable_irqs(HOUR_24, AIE | UIE), 0x32);
    assert_eq!(disable_irqs(HOUR_24 | PIE | AIE | UIE, PIE | UIE), 0x22);
}

/// rtc-cmos.c:99-109,355 masks source flags by corresponding enable bits while retaining IRQF.
#[test]
fn status_requires_irqf_and_an_enabled_source() {
    let raw = IRQF | PF | AF | UF;
    assert_eq!(filter_irq_status(raw, AIE), IRQF | AF);
    assert_eq!(interrupt_sources(IRQF | AF), AF);
    assert_eq!(interrupt_sources(AF), 0);
    assert_eq!(interrupt_sources(IRQF), 0);
}

/// rtc-cmos.c:714-740: suspend_ctrl overrides possibly BIOS-modified B; AIE is one-shot and is
/// cleared from both saved and live controls.
#[test]
fn alarm_interrupt_is_one_shot_and_suspend_control_wins() {
    assert_eq!(
        handle_interrupt(IRQF | AF | PF, PIE, AIE),
        InterruptResult {
            status: IRQF | AF,
            control_b: PIE,
            suspend_control: 0,
            handled: true,
        }
    );
    assert_eq!(
        handle_interrupt(IRQF | PF, PIE | HOUR_24, 0),
        InterruptResult {
            status: IRQF | PF,
            control_b: PIE | HOUR_24,
            suspend_control: 0,
            handled: true,
        }
    );
    assert!(!handle_interrupt(IRQF | UF, PIE, 0).handled);
}

/// rtc-cmos.c:1066-1069 only rejects 12-hour mode when there is a valid IRQ.
#[test]
fn alarm_capable_hardware_must_be_in_24_hour_mode() {
    assert!(supports_required_24_hour_mode(true, HOUR_24));
    assert!(!supports_required_24_hour_mode(true, 0));
    assert!(supports_required_24_hour_mode(false, 0));
}
