// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for wake, suspend, resume, and power-off decisions.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c`; original copyright Paul Gortmaker, David Brownell,
//! and the Linux RTC authors.

use rtc_cmos_core::wake::*;
use rtc_mc146818_core::registers::{AIE, HOUR_24, PIE, UIE};

/// rtc-cmos.c:819-830 has four named switch outcomes. Expected names are literal and independent
/// of the production table.
#[test]
fn all_four_cpu_vendor_cases_are_pinned() {
    assert_eq!(CPU_VENDORS.len(), 4);
    assert_eq!(
        CPU_VENDORS,
        [
            ("X86_VENDOR_INTEL", CpuVendor::Intel),
            ("X86_VENDOR_AMD", CpuVendor::Amd),
            ("X86_VENDOR_HYGON", CpuVendor::Hygon),
            ("default", CpuVendor::Other),
        ]
    );
}

/// rtc-cmos.c:817-836 uses literal BIOS-year thresholds 2015 and 2021, and HPET is mandatory.
#[test]
fn acpi_alarm_quirk_uses_linux_vendor_year_thresholds() {
    assert!(!use_acpi_alarm_quirk(CpuVendor::Intel, 2014, true));
    assert!(use_acpi_alarm_quirk(CpuVendor::Intel, 2015, true));
    assert!(!use_acpi_alarm_quirk(CpuVendor::Amd, 2020, true));
    assert!(use_acpi_alarm_quirk(CpuVendor::Amd, 2021, true));
    assert!(use_acpi_alarm_quirk(CpuVendor::Hygon, 2021, true));
    assert!(!use_acpi_alarm_quirk(CpuVendor::Other, 9999, true));
    assert!(!use_acpi_alarm_quirk(CpuVendor::Intel, 2026, false));
}

/// rtc-cmos.c:1226-1255 preserves AIE only when the device may wake; mode bits are preserved.
#[test]
fn suspend_keeps_only_a_wakeup_alarm() {
    assert_eq!(
        suspend_decision(HOUR_24 | PIE | AIE | UIE, true, false),
        SuspendDecision {
            saved_control: 0x72,
            suspend_control: 0x22,
            masked: 0x50,
            enable_non_acpi_wake: true,
        }
    );
    assert_eq!(
        suspend_decision(HOUR_24 | PIE | AIE | UIE, false, false),
        SuspendDecision {
            saved_control: 0x72,
            suspend_control: 0x02,
            masked: 0x70,
            enable_non_acpi_wake: false,
        }
    );
    assert!(!suspend_decision(AIE, true, true).enable_non_acpi_wake);
}

/// rtc-cmos.c:1189-1221 names four exceptional shutdown cases and cancels now+1 with now-1.
#[test]
fn poweroff_alarm_workaround_is_explicit_and_named() {
    assert_eq!(
        poweroff_decision(0, 100, 0),
        PoweroffDecision::RefusedNoProgrammedAlarm { alarm_expires: 0 }
    );
    assert_eq!(
        poweroff_decision(101, 100, AIE),
        PoweroffDecision::RefusedAlarmAlreadyEnabled { control_b: 0x20 }
    );
    assert_eq!(
        poweroff_decision(101, 100, 0),
        PoweroffDecision::CancelOneSecondAlarm { replacement_timestamp: 99 }
    );
    assert_eq!(
        poweroff_decision(102, 100, 0),
        PoweroffDecision::RefusedFutureAlarmStillArmed { alarm_expires: 102, latest_safe: 101 }
    );
    assert_eq!(poweroff_decision(100, 100, 0), PoweroffDecision::NoChange);
}

/// rtc-cmos.c:1332-1348 restores exactly the three interrupt-enable bits, not register-B modes.
#[test]
fn resume_extracts_only_irq_enable_bits() {
    assert_eq!(resume_irq_mask(HOUR_24 | PIE | AIE | UIE), 0x70);
    assert_eq!(resume_irq_mask(HOUR_24), 0);
}

/// rtc-cmos.c:1306-1313 restores if either expiration or enabled state changed.
#[test]
fn firmware_alarm_change_is_detected_by_time_or_enabled_state() {
    assert!(!alarm_restore_needed(100, true, 100, true));
    assert!(alarm_restore_needed(101, true, 100, true));
    assert!(alarm_restore_needed(100, false, 100, true));
}

/// rtc-cmos.c:1290-1304 acknowledges only an armed, expired ACPI alarm.
#[test]
fn expired_acpi_alarm_requires_all_three_conditions() {
    assert!(acpi_alarm_expired(AIE, 100, 100, true));
    assert!(!acpi_alarm_expired(0, 100, 100, true));
    assert!(!acpi_alarm_expired(AIE, 99, 100, true));
    assert!(!acpi_alarm_expired(AIE, 100, 100, false));
}
