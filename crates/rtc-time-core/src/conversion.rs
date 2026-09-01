// SPDX-License-Identifier: GPL-2.0-only
//! `rtc_time` and seconds-since-epoch conversion in both directions.
//!
//! Ported from Linux `drivers/rtc/lib.c:45-154` and `drivers/rtc/lib.c:176-185`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original authors: Alessandro Zummo and Cassio Neri.

use crate::{calendar::rtc_month_days, RtcTime, TimeError};

/// Seconds from 0000-03-01 to the Unix epoch (`drivers/rtc/lib.c:61-72`).
const MARCH_ZERO_TO_EPOCH_SECONDS: i64 = 719_468 * 86_400;

/// Earliest time covered by Linux's stated guarantee: 1900-01-01 (`drivers/rtc/lib.c:45-50`).
pub const MIN_TIME64: i64 = -2_208_988_800;

/// Last second of the latest date for which `4 * udays + 3` fits in `u32`
/// (`drivers/rtc/lib.c:66-68`).
pub const MAX_TIME64: i64 = 92_709_131_558_399;

/// Convert Unix-epoch seconds to an RTC time (`drivers/rtc/lib.c:45-154`).
pub fn rtc_time64_to_tm(time: i64) -> Result<RtcTime, TimeError> {
    if time < MIN_TIME64 {
        return Err(TimeError::Time64BelowMinimum {
            value: time,
            minimum: MIN_TIME64,
        });
    }
    if time > MAX_TIME64 {
        return Err(TimeError::Time64AboveMaximum {
            value: time,
            maximum: MAX_TIME64,
        });
    }

    let shifted = time + MARCH_ZERO_TO_EPOCH_SECONDS;
    let udays = (shifted / 86_400) as u32;
    let mut secs = (shifted % 86_400) as i32;

    let wday = ((udays + 3) % 7) as i32; // drivers/rtc/lib.c:74-78

    let mut u32tmp = 4 * udays + 3; // drivers/rtc/lib.c:110
    let century = u32tmp / 146_097; // drivers/rtc/lib.c:111
    let day_of_century = u32tmp % 146_097 / 4; // drivers/rtc/lib.c:112

    u32tmp = 4 * day_of_century + 3; // drivers/rtc/lib.c:114
    let u64tmp = 2_939_745_u64 * u32tmp as u64; // drivers/rtc/lib.c:115
    let year_of_century = (u64tmp >> 32) as u32; // drivers/rtc/lib.c:116
    let mut day_of_year = (u64tmp as u32) / 2_939_745 / 4; // drivers/rtc/lib.c:117

    let mut year = 100 * century + year_of_century; // drivers/rtc/lib.c:119
    let leap = if year_of_century != 0 {
        year_of_century % 4 == 0
    } else {
        century % 4 == 0
    }; // drivers/rtc/lib.c:120-121

    u32tmp = 2_141 * day_of_year + 132_377; // drivers/rtc/lib.c:123
    let mut month = u32tmp >> 16; // drivers/rtc/lib.c:124
    let mut day = (u32tmp as u16) as u32 / 2_141; // drivers/rtc/lib.c:125

    let jan_or_feb = day_of_year >= 306; // drivers/rtc/lib.c:127-131
    year += u32::from(jan_or_feb);
    month = if jan_or_feb { month - 12 } else { month };
    day += 1;
    day_of_year = if jan_or_feb {
        day_of_year - 306
    } else {
        day_of_year + 31 + 28 + u32::from(leap)
    }; // drivers/rtc/lib.c:133-139

    let hour = secs / 3_600;
    secs -= hour * 3_600;
    let minute = secs / 60;
    let second = secs - minute * 60; // drivers/rtc/lib.c:147-150

    Ok(RtcTime {
        tm_sec: second,
        tm_min: minute,
        tm_hour: hour,
        tm_mday: day as i32,
        tm_mon: month as i32,
        tm_year: year as i32 - 1900,
        tm_wday: wday,
        tm_yday: day_of_year as i32 + 1,
        tm_isdst: 0,
    }) // drivers/rtc/lib.c:141-152
}

/// Convert an RTC time to Unix-epoch seconds (`drivers/rtc/lib.c:176-185`).
///
/// As in Linux, weekday, year-day, and DST fields do not participate. Unlike C array indexing,
/// malformed calendar fields produce a named refusal.
pub fn rtc_tm_to_time64(tm: &RtcTime) -> Result<i64, TimeError> {
    const MIN_CONVERTIBLE_TM_YEAR: i32 = -1900;
    const MAX_CONVERTIBLE_TM_YEAR: i32 = i32::MAX - 1900;

    if tm.tm_year < MIN_CONVERTIBLE_TM_YEAR {
        return Err(TimeError::YearBelowMinimum {
            value: tm.tm_year,
            minimum: MIN_CONVERTIBLE_TM_YEAR,
        });
    }
    if tm.tm_year > MAX_CONVERTIBLE_TM_YEAR {
        return Err(TimeError::YearAboveMaximum {
            value: tm.tm_year,
            maximum: MAX_CONVERTIBLE_TM_YEAR,
        });
    }
    if !(0..=11).contains(&tm.tm_mon) {
        return Err(TimeError::MonthOutOfRange {
            value: tm.tm_mon,
            minimum: 0,
            maximum: 11,
        });
    }

    let year = tm.tm_year as i64 + 1900;
    let maximum_day = rtc_month_days(tm.tm_mon as u32, year as u32)
        .expect("month was checked immediately above") as i32;
    if tm.tm_mday < 1 || tm.tm_mday > maximum_day {
        return Err(TimeError::DayOutOfRange {
            value: tm.tm_mday,
            minimum: 1,
            maximum: maximum_day,
        });
    }
    if !(0..=23).contains(&tm.tm_hour) {
        return Err(TimeError::HourOutOfRange {
            value: tm.tm_hour,
            minimum: 0,
            maximum: 23,
        });
    }
    if !(0..=59).contains(&tm.tm_min) {
        return Err(TimeError::MinuteOutOfRange {
            value: tm.tm_min,
            minimum: 0,
            maximum: 59,
        });
    }
    if !(0..=59).contains(&tm.tm_sec) {
        return Err(TimeError::SecondOutOfRange {
            value: tm.tm_sec,
            minimum: 0,
            maximum: 59,
        });
    }

    // The March-based inverse of the calendar used above. Linux supplies these exact fields to
    // mktime64 at drivers/rtc/lib.c:180-183.
    let month = tm.tm_mon as i64 + 1;
    let computational_year = if month <= 2 { year - 1 } else { year };
    let computational_month = if month <= 2 { month + 9 } else { month - 3 };
    let days = 365 * computational_year + computational_year / 4 - computational_year / 100
        + computational_year / 400
        + (153 * computational_month + 2) / 5
        + tm.tm_mday as i64
        - 1;

    Ok((days - 719_468) * 86_400
        + tm.tm_hour as i64 * 3_600
        + tm.tm_min as i64 * 60
        + tm.tm_sec as i64)
}
