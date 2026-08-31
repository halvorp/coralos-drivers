// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for MC146818 alarm semantics.
//!
//! Ported from `include/linux/mc146818rtc.h:51-57`; copyright Torsten Duwe, Motorola, and Linux.

use rtc_mc146818_core::alarm::{
    alarm_field_matches, decode_alarm_field, encode_alarm_field, AlarmField,
};

/// include/linux/mc146818rtc.h:56-57 — an alarm is always true only if BOTH MSBs are set.
#[test]
fn only_both_high_bits_mean_dont_care() {
    assert_eq!(decode_alarm_field(0xc0), AlarmField::Any);
    assert_eq!(decode_alarm_field(0xc1), AlarmField::Any);
    assert_eq!(decode_alarm_field(0xff), AlarmField::Any);
    assert_eq!(decode_alarm_field(0x80), AlarmField::Match(0x80));
    assert_eq!(decode_alarm_field(0x40), AlarmField::Match(0x40));
    assert_eq!(decode_alarm_field(0x25), AlarmField::Match(0x25));
}

/// include/linux/mc146818rtc.h:57 gives Linux's literal wildcard encoding, 0xc0.
#[test]
fn encoding_preserves_exact_values_and_uses_linux_wildcard() {
    assert_eq!(encode_alarm_field(AlarmField::Any), 0xc0);
    assert_eq!(encode_alarm_field(AlarmField::Match(0x25)), 0x25);
}

/// include/linux/mc146818rtc.h:56 — wildcard is always true; another field compares normally.
#[test]
fn wildcard_matches_every_sample_and_exact_value_does_not() {
    assert!(alarm_field_matches(AlarmField::Any, 0x00));
    assert!(alarm_field_matches(AlarmField::Any, 0x59));
    assert!(alarm_field_matches(AlarmField::Match(0x25), 0x25));
    assert!(!alarm_field_matches(AlarmField::Match(0x25), 0x26));
}
