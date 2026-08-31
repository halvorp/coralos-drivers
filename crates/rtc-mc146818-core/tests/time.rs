// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for time conversion, century handling, and set preparation.
//!
//! Ported from `drivers/rtc/rtc-mc146818-lib.c`; copyright Torsten Duwe, Motorola, and Linux RTC.

use rtc_mc146818_core::registers::DM_BINARY;
use rtc_mc146818_core::time::{
    decode_time, encode_time, set_mode_frequency_value, uses_bcd, EncodeError, EncodedTime,
    RawTime, RtcTime,
};

const RAW_BCD: RawTime = RawTime {
    second: 0x56,
    minute: 0x34,
    hour: 0x12,
    day: 0x29,
    month: 0x02,
    year: 0x24,
};

/// rtc-mc146818-lib.c:165 and :268 — clear DM_BINARY means BCD, while RTC_ALWAYS_BCD overrides it.
#[test]
fn mode_selection_matches_linux_condition() {
    assert!(uses_bcd(0x00, false));
    assert!(!uses_bcd(DM_BINARY, false));
    assert!(uses_bcd(DM_BINARY, true));
}

/// rtc-mc146818-lib.c:165-175,187-194 — BCD conversion, <=69 pivot, and zero-based month.
#[test]
fn bcd_snapshot_decodes_with_linux_pivot_and_month_convention() {
    assert_eq!(
        decode_time(RAW_BCD, 0x00, false, None),
        RtcTime {
            second: 56,
            minute: 34,
            hour: 12,
            day: 29,
            month: 1,
            year: 124
        }
    );
    // 69 pivots to 2069; 70 remains 1970.
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x69,
                month: 0x01,
                ..RAW_BCD
            },
            0,
            false,
            None
        )
        .year,
        169
    );
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x70,
                month: 0x01,
                ..RAW_BCD
            },
            0,
            false,
            None
        )
        .year,
        70
    );
}

/// rtc-mc146818-lib.c:165-176 — binary mode skips conversion; RTC_ALWAYS_BCD forces it anyway.
#[test]
fn binary_mode_and_always_bcd_are_distinct() {
    let raw = RawTime {
        second: 0x42,
        minute: 1,
        hour: 2,
        day: 3,
        month: 4,
        year: 70,
    };
    assert_eq!(decode_time(raw, DM_BINARY, false, None).second, 0x42);
    assert_eq!(decode_time(raw, DM_BINARY, true, None).second, 42);
}

/// rtc-mc146818-lib.c:173-185 — the century uses the same encoding and contributes above 19.
#[test]
fn century_register_extends_the_year() {
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x24,
                month: 1,
                ..RawTime::default()
            },
            0,
            false,
            Some(0x20)
        )
        .year,
        124
    );
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x24,
                month: 1,
                ..RawTime::default()
            },
            0,
            false,
            Some(0x21)
        )
        .year,
        224
    );
    // Linux only applies a century strictly greater than 19. Year 70 distinguishes century 20's
    // explicit +100 from the later <=69 pivot, which would otherwise hide this boundary.
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x70,
                month: 1,
                ..RawTime::default()
            },
            0,
            false,
            Some(0x20)
        )
        .year,
        170
    );
    assert_eq!(
        decode_time(
            RawTime {
                year: 0x24,
                month: 1,
                ..RawTime::default()
            },
            0,
            false,
            Some(0x19)
        )
        .year,
        124
    );
}

/// rtc-mc146818-lib.c:223-275 — set path adds one to month, folds 100..169, and BCD-encodes.
#[test]
fn no_century_bcd_set_vector_matches_linux() {
    let encoded = encode_time(
        RtcTime {
            second: 56,
            minute: 34,
            hour: 12,
            day: 29,
            month: 1,
            year: 124,
        },
        0,
        false,
        false,
    );
    assert_eq!(
        encoded,
        Ok(EncodedTime {
            raw: RAW_BCD,
            century: None
        })
    );
}

/// rtc-mc146818-lib.c:248-253,268-275 — ACPI century is `(tm_year + 1900) / 100` then BCD.
#[test]
fn century_set_vector_splits_2124_into_21_and_24() {
    let encoded = encode_time(
        RtcTime {
            second: 1,
            minute: 2,
            hour: 3,
            day: 4,
            month: 5,
            year: 224,
        },
        0,
        false,
        true,
    );
    assert_eq!(
        encoded,
        Ok(EncodedTime {
            raw: RawTime {
                second: 0x01,
                minute: 0x02,
                hour: 0x03,
                day: 0x04,
                month: 0x06,
                year: 0x24
            },
            century: Some(0x21),
        })
    );
}

/// rtc-mc146818-lib.c:230-231 and :259-260 — refusals name the supplied year and Linux bound.
#[test]
fn year_refusals_name_value_and_bound() {
    let base = RtcTime {
        year: 256,
        ..RtcTime::default()
    };
    assert_eq!(
        encode_time(base, DM_BINARY, false, false),
        Err(EncodeError::YearExceedsUnsignedByte {
            year: 256,
            maximum: 255
        })
    );
    let no_century = RtcTime {
        year: 170,
        ..RtcTime::default()
    };
    assert_eq!(
        encode_time(no_century, DM_BINARY, false, false),
        Err(EncodeError::YearExceedsNoCenturyRange {
            year: 170,
            maximum: 169
        })
    );
    assert!(encode_time(
        RtcTime {
            year: 169,
            ..RtcTime::default()
        },
        DM_BINARY,
        false,
        false
    )
    .is_ok());
}

/// rtc-mc146818-lib.c:281-285 — AMD/Hygon clears 0x10; other vendors OR divider reset 0x70.
#[test]
fn register_a_setting_has_linux_vendor_split() {
    assert_eq!(set_mode_frequency_value(0x3f, true), 0x2f);
    assert_eq!(set_mode_frequency_value(0x05, false), 0x75);
}
