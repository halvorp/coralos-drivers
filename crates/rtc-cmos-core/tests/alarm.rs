// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for CMOS alarm conversion and range selection.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c`; original copyright Paul Gortmaker, David Brownell,
//! and the Linux RTC authors.

use rtc_cmos_core::alarm::*;
use rtc_mc146818_core::registers::{AIE, DM_BINARY};

/// rtc-cmos.c:305,554,918-920,1010-1017.
#[test]
fn alarm_literals_match_linux() {
    assert_eq!(UIP_AVOID_TIMEOUT_MS, 10);
    assert_eq!(SECS_PER_DAY, 86_400);
    assert_eq!(SECS_PER_MONTH, 2_419_200);
    assert_eq!(SECS_PER_YEAR, 31_536_000);
    assert_eq!(ACPI_REGISTER_LIMIT, 128);
}

/// rtc-cmos.c:1123-1125 names exactly three alarm reaches. This list is literal, not generated
/// from the production table.
#[test]
fn all_three_alarm_range_names_are_pinned() {
    assert_eq!(ALARM_RANGES.len(), 3);
    assert_eq!(
        ALARM_RANGES,
        [
            ("day", AlarmRange::Day),
            ("month", AlarmRange::Month),
            ("year", AlarmRange::Year),
        ]
    );
}

/// rtc-cmos.c:1010-1017 rejects FADT register numbers at or above 128 by clearing them.
#[test]
fn acpi_registers_are_limited_to_bank_one() {
    assert_eq!(sanitize_acpi_register(0), 0);
    assert_eq!(sanitize_acpi_register(127), 127);
    assert_eq!(sanitize_acpi_register(128), 0);
    assert_eq!(sanitize_acpi_register(255), 0);
}

/// rtc-cmos.c:1028-1033: month implies year reach, day implies month reach, neither means day.
#[test]
fn enhanced_registers_select_alarm_reach_and_literal_offsets() {
    assert_eq!(alarm_range(0, 0), AlarmRange::Day);
    assert_eq!(alarm_range(9, 0), AlarmRange::Month);
    assert_eq!(alarm_range(9, 8), AlarmRange::Year);
    assert_eq!(alarm_offset_max(AlarmRange::Day), 86_399);
    assert_eq!(alarm_offset_max(AlarmRange::Month), 2_419_199);
    assert_eq!(alarm_offset_max(AlarmRange::Year), 31_535_999);
}

/// rtc-cmos.c:408-463 uses a strict `>` comparison and names day/month/year as the refusal cause.
#[test]
fn validation_accepts_the_boundary_and_names_each_refusal() {
    assert_eq!(validate_alarm(AlarmRange::Day, 199, 200), Ok(()));
    assert_eq!(validate_alarm(AlarmRange::Day, 200, 200), Ok(()));
    assert_eq!(
        validate_alarm(AlarmRange::Day, 201, 200),
        Err(AlarmRefusal::AlarmBeyondDay { alarm: 201, latest: 200 })
    );
    assert_eq!(
        validate_alarm(AlarmRange::Month, 301, 300),
        Err(AlarmRefusal::AlarmBeyondMonth { alarm: 301, latest: 300 })
    );
    assert_eq!(
        validate_alarm(AlarmRange::Year, 401, 400),
        Err(AlarmRefusal::AlarmBeyondYear { alarm: 401, latest: 400 })
    );
}

/// rtc-cmos.c:260-338. ACPI says day upper bits are ignored; BCD limits are 0x60,0x60,0x24,
/// 0x31,0x12; month becomes zero-based; AIE supplies enabled and pending is always zero.
#[test]
fn bcd_alarm_readback_matches_linux() {
    let decoded = decode_alarm(
        RawAlarm {
            second: 0x59,
            minute: 0x42,
            hour: 0x23,
            day: Some(0xc9),
            month: Some(0x12),
        },
        AIE,
        false,
    );
    assert_eq!(
        decoded,
        DecodedAlarm {
            time: AlarmTime { second: 59, minute: 42, hour: 23, day: 9, month: 11 },
            enabled: true,
            pending: false,
        }
    );

    let invalid = decode_alarm(
        RawAlarm { second: 0x60, minute: 0xff, hour: 0x24, day: Some(0x32), month: Some(0x13) },
        0,
        false,
    );
    assert_eq!(invalid.time, AlarmTime { second: -1, minute: -1, hour: -1, day: -1, month: -1 });
    let zero_day = decode_alarm(
        RawAlarm { second: 0, minute: 0, hour: 0, day: Some(0), month: Some(0) },
        0,
        false,
    );
    assert_eq!(zero_day.time.day, -1, "rtc-cmos.c:267-268 keeps zero as unavailable");
}

/// rtc-cmos.c:308-335 skips BCD conversion in binary mode and uses zero as the enhanced-field
/// unavailable sentinel.
#[test]
fn binary_alarm_readback_is_not_converted() {
    let decoded = decode_alarm(
        RawAlarm { second: 59, minute: 42, hour: 23, day: Some(0), month: None },
        DM_BINARY,
        false,
    );
    assert_eq!(decoded.time, AlarmTime { second: 59, minute: 42, hour: 23, day: -1, month: -1 });
}

/// rtc-cmos.c:529-546 writes BCD and maps each out-of-range field to literal 0xff.
#[test]
fn alarm_programming_uses_mc146818_bcd_and_linux_wildcards() {
    assert_eq!(
        encode_alarm(
            AlarmTime { second: 59, minute: 42, hour: 23, day: 31, month: 11 },
            0,
            false,
            7,
            8,
        ),
        EncodedAlarm { second: 0x59, minute: 0x42, hour: 0x23, day: Some(0x31), month: Some(0x12) }
    );
    assert_eq!(
        encode_alarm(
            AlarmTime { second: 60, minute: -1, hour: 24, day: 0, month: 12 },
            0,
            false,
            7,
            8,
        ),
        EncodedAlarm { second: 0xff, minute: 0xff, hour: 0xff, day: Some(0xff), month: Some(0xff) }
    );
    assert_eq!(
        encode_alarm(
            AlarmTime { second: -1, minute: -1, hour: -1, day: -1, month: -1 },
            0,
            false,
            7,
            8,
        ),
        EncodedAlarm { second: 0xff, minute: 0xff, hour: 0xff, day: Some(0xff), month: Some(0x00) },
        "rtc-cmos.c:529-545 adds one to tm_mon, then narrows fields before range checks"
    );
}

/// rtc-cmos.c:529-546 leaves binary values alone and writes enhanced fields only when registers
/// exist.
#[test]
fn binary_programming_omits_absent_enhanced_registers() {
    assert_eq!(
        encode_alarm(
            AlarmTime { second: 5, minute: 4, hour: 3, day: 2, month: 1 },
            DM_BINARY,
            false,
            0,
            0,
        ),
        EncodedAlarm { second: 5, minute: 4, hour: 3, day: None, month: None }
    );
}
