// SPDX-License-Identifier: GPL-2.0-only
//! Month lengths, leap-year rules, and year-day arithmetic.
//!
//! Ported from Linux `drivers/rtc/lib.c:16-43` and the explicit Gregorian leap calculation at
//! `drivers/rtc/lib.c:119-121`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original author: Alessandro Zummo.

use crate::TimeError;

/// Common-year month lengths (`drivers/rtc/lib.c:16-18`).
pub const RTC_DAYS_IN_MONTH: [u8; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Days before each month plus the end-of-year sentinel (`drivers/rtc/lib.c:20-25`).
pub const RTC_YDAYS: [[u16; 13]; 2] = [
    [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365],
    [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366],
];

/// Proleptic-Gregorian leap-year rule (`drivers/rtc/lib.c:119-121`).
pub const fn is_leap_year(year: u32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

/// Number of days in a zero-based month (`drivers/rtc/lib.c:27-34`).
pub fn rtc_month_days(month: u32, year: u32) -> Result<u32, TimeError> {
    if month >= RTC_DAYS_IN_MONTH.len() as u32 {
        return Err(TimeError::MonthOutOfRange {
            value: month as i32,
            minimum: 0,
            maximum: 11,
        });
    }

    Ok(RTC_DAYS_IN_MONTH[month as usize] as u32 + u32::from(is_leap_year(year) && month == 1))
}

/// Zero-based number of days since January 1 (`drivers/rtc/lib.c:36-43`).
pub fn rtc_year_days(day: u32, month: u32, year: u32) -> Result<u32, TimeError> {
    let maximum = rtc_month_days(month, year)?;
    if day < 1 || day > maximum {
        return Err(TimeError::DayOutOfRange {
            value: day as i32,
            minimum: 1,
            maximum: maximum as i32,
        });
    }

    Ok(RTC_YDAYS[usize::from(is_leap_year(year))][month as usize] as u32 + day - 1)
}
