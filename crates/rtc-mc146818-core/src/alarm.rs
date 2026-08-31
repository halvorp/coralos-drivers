// SPDX-License-Identifier: GPL-2.0-only
//! MC146818 alarm-field wildcard semantics.
//!
//! Ported from Linux `include/linux/mc146818rtc.h:51-57`, written by Torsten Duwe and derived from
//! the Motorola data sheet; copyright Torsten Duwe, Motorola, and the Linux RTC authors.

use crate::registers::ALARM_DONT_CARE;

/// Meaning of one seconds, minutes, or hours alarm register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmField {
    /// This field must equal the encoded register value.
    Match(u8),
    /// The two most-significant bits are both set, so this field always matches.
    Any,
}

/// Decode the alarm wildcard rule: an alarm field is always true only when BOTH MSBs are set.
pub const fn decode_alarm_field(value: u8) -> AlarmField {
    if value & ALARM_DONT_CARE == ALARM_DONT_CARE {
        AlarmField::Any
    } else {
        AlarmField::Match(value)
    }
} // include/linux/mc146818rtc.h:56-57

/// Encode one alarm field without altering exact-match values.
pub const fn encode_alarm_field(field: AlarmField) -> u8 {
    match field {
        AlarmField::Match(value) => value,
        AlarmField::Any => ALARM_DONT_CARE,
    }
} // include/linux/mc146818rtc.h:56-57

/// Whether a sampled register value satisfies one alarm field.
pub const fn alarm_field_matches(field: AlarmField, sampled: u8) -> bool {
    match field {
        AlarmField::Any => true,
        AlarmField::Match(value) => value == sampled,
    }
} // include/linux/mc146818rtc.h:56-57
