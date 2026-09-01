// SPDX-License-Identifier: GPL-2.0-only
//! Frozen boundary vectors for both RTC conversion directions and weekday computation.
//!
//! Ported from Linux `drivers/rtc/lib.c:45-154` and `drivers/rtc/lib.c:176-185`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original authors: Alessandro Zummo and Cassio Neri.

use rtc_time_core::{
    conversion::{rtc_time64_to_tm, rtc_tm_to_time64, MAX_TIME64, MIN_TIME64},
    RtcTime, TimeError,
};

fn tm(year: i32, mon: i32, mday: i32, hour: i32, min: i32, sec: i32) -> RtcTime {
    RtcTime {
        tm_year: year - 1900,
        tm_mon: mon - 1,
        tm_mday: mday,
        tm_hour: hour,
        tm_min: min,
        tm_sec: sec,
        tm_wday: -1,
        tm_yday: -1,
        tm_isdst: -1,
    }
}

/// The Unix epoch is Thursday (4 with Sunday = 0), and Linux writes one-based `tm_yday`
/// (`drivers/rtc/lib.c:74-78`, :141-152).
#[test]
fn seconds_to_tm_pins_the_epoch() {
    assert_eq!(
        rtc_time64_to_tm(0),
        Ok(RtcTime {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 1,
            tm_mon: 0,
            tm_year: 70,
            tm_wday: 4,
            tm_yday: 1,
            tm_isdst: 0,
        })
    );
}

/// Literal seconds around year and leap-day boundaries. These are fixed vectors, not round trips:
/// a shared bug in both conversion directions must not validate itself.
#[test]
fn seconds_to_tm_crosses_year_and_leap_boundaries() {
    let expected = [
        (-1_i64, 1969, 12, 31, 23, 59, 59, 3, 365),
        (951_782_399, 2000, 2, 28, 23, 59, 59, 1, 59),
        (951_782_400, 2000, 2, 29, 0, 0, 0, 2, 60),
        (951_868_800, 2000, 3, 1, 0, 0, 0, 3, 61),
        (4_107_542_399, 2100, 2, 28, 23, 59, 59, 0, 59),
        (4_107_542_400, 2100, 3, 1, 0, 0, 0, 1, 60),
    ];

    for (seconds, year, month, day, hour, minute, second, wday, yday) in expected {
        assert_eq!(
            rtc_time64_to_tm(seconds),
            Ok(RtcTime {
                tm_sec: second,
                tm_min: minute,
                tm_hour: hour,
                tm_mday: day,
                tm_mon: month - 1,
                tm_year: year - 1900,
                tm_wday: wday,
                tm_yday: yday,
                tm_isdst: 0,
            }),
            "seconds {seconds}"
        );
    }
}

/// Linux documents that this conversion works since at least 1900 (`drivers/rtc/lib.c:45-50`).
#[test]
fn seconds_to_tm_pins_its_documented_lower_limit() {
    assert_eq!(MIN_TIME64, -2_208_988_800);
    assert_eq!(
        rtc_time64_to_tm(-2_208_988_800),
        Ok(RtcTime {
            tm_sec: 0,
            tm_min: 0,
            tm_hour: 0,
            tm_mday: 1,
            tm_mon: 0,
            tm_year: 0,
            tm_wday: 1,
            tm_yday: 1,
            tm_isdst: 0,
        })
    );
    assert_eq!(
        rtc_time64_to_tm(-2_208_988_801),
        Err(TimeError::Time64BelowMinimum {
            value: -2_208_988_801,
            minimum: -2_208_988_800,
        })
    );
}

/// Linux's arithmetic limit is `1073741823` days after 0000-03-01
/// (`drivers/rtc/lib.c:61-68`). Pin the resulting last second and the named refusal after it.
#[test]
fn seconds_to_tm_pins_the_u32_arithmetic_limit() {
    assert_eq!(MAX_TIME64, 92_709_131_558_399);
    assert_eq!(
        rtc_time64_to_tm(92_709_131_558_399),
        Ok(RtcTime {
            tm_sec: 59,
            tm_min: 59,
            tm_hour: 23,
            tm_mday: 5,
            tm_mon: 5,
            tm_year: 2_937_905,
            tm_wday: 3,
            tm_yday: 156,
            tm_isdst: 0,
        })
    );
    assert_eq!(
        rtc_time64_to_tm(92_709_131_558_400),
        Err(TimeError::Time64AboveMaximum {
            value: 92_709_131_558_400,
            maximum: 92_709_131_558_399,
        })
    );
}

/// Fixed literals for `rtc_tm_to_time64` (`drivers/rtc/lib.c:176-185`), including pre-epoch,
/// leap-day, ordinary time-of-day, and non-leap-century boundaries.
#[test]
fn tm_to_seconds_matches_literal_boundary_vectors() {
    assert_eq!(rtc_tm_to_time64(&tm(1970, 1, 1, 0, 0, 0)), Ok(0));
    assert_eq!(rtc_tm_to_time64(&tm(1969, 12, 31, 23, 59, 59)), Ok(-1));
    assert_eq!(
        rtc_tm_to_time64(&tm(1900, 1, 1, 0, 0, 0)),
        Ok(-2_208_988_800)
    );
    assert_eq!(
        rtc_tm_to_time64(&tm(2000, 2, 29, 12, 34, 56)),
        Ok(951_827_696)
    );
    assert_eq!(rtc_tm_to_time64(&tm(2000, 3, 1, 0, 0, 0)), Ok(951_868_800));
    assert_eq!(
        rtc_tm_to_time64(&tm(2100, 3, 1, 0, 0, 0)),
        Ok(4_107_542_400)
    );
}

/// Linux passes only year/month/day/time fields to `mktime64` (`drivers/rtc/lib.c:180-183`).
#[test]
fn tm_to_seconds_ignores_derived_fields() {
    let mut value = tm(2000, 2, 29, 12, 34, 56);
    value.tm_wday = 6;
    value.tm_yday = 999;
    value.tm_isdst = 1;
    assert_eq!(rtc_tm_to_time64(&value), Ok(951_827_696));
}
