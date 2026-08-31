// SPDX-License-Identifier: GPL-2.0-only
//! Suspend, wake, ACPI selection, and power-off alarm decisions.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c:817-836,841-867,1181-1224,1226-1265,1281-1367`;
//! original copyright Paul Gortmaker, David Brownell, and the Linux RTC authors.

use rtc_mc146818_core::registers::{AIE, PIE, UIE};
use crate::control::IRQ_MASK;

/// CPU vendors named by Linux's ACPI alarm quirk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuVendor {
    Intel,
    Amd,
    Hygon,
    Other,
}

/// All four vendor cases named by the quirk switch.
pub const CPU_VENDORS: [(&str, CpuVendor); 4] = [
    ("X86_VENDOR_INTEL", CpuVendor::Intel),
    ("X86_VENDOR_AMD", CpuVendor::Amd),
    ("X86_VENDOR_HYGON", CpuVendor::Hygon),
    ("default", CpuVendor::Other),
]; // drivers/rtc/rtc-cmos.c:819-830

/// Linux enables ACPI SCI alarm handling only on sufficiently new Intel/AMD/Hygon BIOSes with HPET.
pub const fn use_acpi_alarm_quirk(vendor: CpuVendor, bios_year: u16, hpet_enabled: bool) -> bool {
    let new_enough = match vendor {
        CpuVendor::Intel => bios_year >= 2015,
        CpuVendor::Amd | CpuVendor::Hygon => bios_year >= 2021,
        CpuVendor::Other => false,
    };
    new_enough && hpet_enabled
} // drivers/rtc/rtc-cmos.c:817-836

/// Suspend register-B result and whether the non-ACPI wake route must be enabled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendDecision {
    pub saved_control: u8,
    pub suspend_control: u8,
    pub masked: u8,
    pub enable_non_acpi_wake: bool,
}

/// Keep only AIE during wake-capable suspend; otherwise mask every RTC interrupt source.
pub const fn suspend_decision(
    control_b: u8,
    device_may_wakeup: bool,
    use_acpi_alarm: bool,
) -> SuspendDecision {
    let mask = if device_may_wakeup { IRQ_MASK & !AIE } else { IRQ_MASK };
    let suspend_control = control_b & !mask;
    SuspendDecision {
        saved_control: control_b,
        suspend_control,
        masked: mask,
        enable_non_acpi_wake: suspend_control & AIE != 0 && !use_acpi_alarm,
    }
} // drivers/rtc/rtc-cmos.c:1226-1255

/// Named power-off alarm disposition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoweroffDecision {
    RefusedNoProgrammedAlarm { alarm_expires: i64 },
    RefusedAlarmAlreadyEnabled { control_b: u8 },
    CancelOneSecondAlarm { replacement_timestamp: i64 },
    RefusedFutureAlarmStillArmed { alarm_expires: i64, latest_safe: i64 },
    NoChange,
}

/// Reproduce Linux's shutdown workaround for firmware that reboots on a now+1 alarm.
pub const fn poweroff_decision(
    alarm_expires: i64,
    now: i64,
    control_b: u8,
) -> PoweroffDecision {
    if alarm_expires == 0 {
        return PoweroffDecision::RefusedNoProgrammedAlarm { alarm_expires };
    }
    if control_b & AIE != 0 {
        return PoweroffDecision::RefusedAlarmAlreadyEnabled { control_b };
    }
    if alarm_expires == now + 1 {
        PoweroffDecision::CancelOneSecondAlarm { replacement_timestamp: now - 1 }
    } else if alarm_expires > now + 1 {
        PoweroffDecision::RefusedFutureAlarmStillArmed {
            alarm_expires,
            latest_safe: now + 1,
        }
    } else {
        PoweroffDecision::NoChange
    }
} // drivers/rtc/rtc-cmos.c:1181-1224

/// Sources restored from the saved register B after resume.
pub const fn resume_irq_mask(saved_control: u8) -> u8 {
    saved_control & (PIE | AIE | UIE)
} // drivers/rtc/rtc-cmos.c:1332-1348

/// Whether resume must restore the saved alarm after firmware changed it.
pub const fn alarm_restore_needed(
    current_expires: i64,
    current_enabled: bool,
    saved_expires: i64,
    saved_enabled: bool,
) -> bool {
    current_expires != saved_expires || current_enabled != saved_enabled
} // drivers/rtc/rtc-cmos.c:1306-1313

/// Whether the resumed ACPI alarm has expired and should be acknowledged immediately.
pub const fn acpi_alarm_expired(
    suspend_control: u8,
    now: i64,
    alarm_expires: i64,
    use_acpi_alarm: bool,
) -> bool {
    suspend_control & AIE != 0 && now >= alarm_expires && use_acpi_alarm
} // drivers/rtc/rtc-cmos.c:1290-1304
