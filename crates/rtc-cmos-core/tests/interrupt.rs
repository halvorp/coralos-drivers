// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for caller-executed interrupt control ordering.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c`; original copyright Paul Gortmaker, David Brownell,
//! and the Linux RTC authors.

use rtc_cmos_core::interrupt::*;
use rtc_mc146818_core::registers::{AIE, INTR_FLAGS, PIE};

/// rtc-cmos.c:360-381 flushes C before enabling, writes B, updates HPET and ACPI, then reads C
/// again. The order is the contract.
#[test]
fn enable_flushes_old_status_before_arming_the_new_source() {
    let actions = irq_enable_actions(PIE, AIE, true, true, true);
    assert_eq!(actions.len(), 5);
    assert!(!actions.is_empty());
    assert_eq!(actions.get(0), Some(ControlAction::ReadInterruptFlags { register: 12 }));
    assert_eq!(actions.get(1), Some(ControlAction::WriteControl { value: 0x60 }));
    assert_eq!(actions.get(2), Some(ControlAction::HpetSet { mask: 0x20 }));
    assert_eq!(actions.get(3), Some(ControlAction::AcpiWakeOn));
    assert_eq!(actions.get(4), Some(ControlAction::ReadInterruptFlags { register: 12 }));
    assert_eq!(actions.get(5), None);
    assert_eq!(INTR_FLAGS, 12);
}

/// rtc-cmos.c:383-399 disables B first, then HPET/ACPI, and finally reads C to acknowledge/check.
#[test]
fn disable_acknowledges_status_after_disarming() {
    let actions = irq_disable_actions(PIE | AIE, AIE, true, true, true);
    assert_eq!(actions.len(), 4);
    assert_eq!(actions.get(0), Some(ControlAction::WriteControl { value: 0x40 }));
    assert_eq!(actions.get(1), Some(ControlAction::HpetMask { mask: 0x20 }));
    assert_eq!(actions.get(2), Some(ControlAction::AcpiWakeOff));
    assert_eq!(actions.get(3), Some(ControlAction::ReadInterruptFlags { register: 12 }));
}

/// rtc-cmos.c:372-378,390-396 only emits platform actions when their respective facilities exist;
/// non-AIE changes never toggle ACPI wake.
#[test]
fn optional_platform_actions_are_not_invented() {
    let plain = irq_enable_actions(0, PIE, false, true, true);
    assert_eq!(plain.len(), 3);
    assert_eq!(plain.get(0), Some(ControlAction::ReadInterruptFlags { register: 12 }));
    assert_eq!(plain.get(1), Some(ControlAction::WriteControl { value: 0x40 }));
    assert_eq!(plain.get(2), Some(ControlAction::ReadInterruptFlags { register: 12 }));

    let no_hook = irq_disable_actions(AIE, AIE, false, true, false);
    assert_eq!(no_hook.len(), 2);
    assert_eq!(no_hook.get(0), Some(ControlAction::WriteControl { value: 0 }));
    assert_eq!(no_hook.get(1), Some(ControlAction::ReadInterruptFlags { register: 12 }));
}
