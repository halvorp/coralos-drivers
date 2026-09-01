// SPDX-License-Identifier: GPL-2.0-only
//! Linux RTC valid-range checks.
//!
//! Ported from Linux `drivers/rtc/lib.c:156-174`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original author: Alessandro Zummo.

use crate::{calendar::rtc_month_days, RtcTime, TimeError};

/// Smallest valid `tm_year`, representing 1970 (`drivers/rtc/lib.c:161`).
pub const MIN_VALID_TM_YEAR: i32 = 70;
/// Largest valid `tm_year`, preserving `tm_year + 1900 <= INT_MAX` (`drivers/rtc/lib.c:162`).
pub const MAX_VALID_TM_YEAR: i32 = i32::MAX - 1900;

/// Validate the fields Linux checks in `rtc_valid_tm` (`drivers/rtc/lib.c:156-174`).
pub fn rtc_valid_tm(tm: &RtcTime) -> Result<(), TimeError> {
    if tm.tm_year < MIN_VALID_TM_YEAR {
        return Err(TimeError::YearBelowMinimum {
            value: tm.tm_year,
            minimum: MIN_VALID_TM_YEAR,
        });
    }
    if tm.tm_year > MAX_VALID_TM_YEAR {
        return Err(TimeError::YearAboveMaximum {
            value: tm.tm_year,
            maximum: MAX_VALID_TM_YEAR,
        });
    }
    if !(0..=11).contains(&tm.tm_mon) {
        return Err(TimeError::MonthOutOfRange {
            value: tm.tm_mon,
            minimum: 0,
            maximum: 11,
        });
    }

    let maximum_day = rtc_month_days(tm.tm_mon as u32, (tm.tm_year + 1900) as u32)
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

    Ok(())
}
