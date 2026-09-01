// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux RTC valid-range checks and named refusals.
//!
//! Ported from Linux `drivers/rtc/lib.c:156-174`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original author: Alessandro Zummo.

use rtc_time_core::{
    validation::{rtc_valid_tm, MAX_VALID_TM_YEAR, MIN_VALID_TM_YEAR},
    RtcTime, TimeError,
};

fn value(year: i32, mon: i32, mday: i32, hour: i32, min: i32, sec: i32) -> RtcTime {
    RtcTime {
        tm_year: year,
        tm_mon: mon,
        tm_mday: mday,
        tm_hour: hour,
        tm_min: min,
        tm_sec: sec,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
    }
}

/// Linux accepts `tm_year` from 70 through `INT_MAX - 1900`, inclusive
/// (`drivers/rtc/lib.c:161-162`).
#[test]
fn documented_year_limits_are_inclusive() {
    assert_eq!(MIN_VALID_TM_YEAR, 70);
    assert_eq!(MAX_VALID_TM_YEAR, 2_147_481_747);
    assert_eq!(rtc_valid_tm(&value(70, 0, 1, 0, 0, 0)), Ok(()));
    assert_eq!(
        rtc_valid_tm(&value(2_147_481_747, 11, 31, 23, 59, 59)),
        Ok(())
    );
    assert_eq!(
        rtc_valid_tm(&value(69, 0, 1, 0, 0, 0)),
        Err(TimeError::YearBelowMinimum {
            value: 69,
            minimum: 70
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(2_147_481_748, 0, 1, 0, 0, 0)),
        Err(TimeError::YearAboveMaximum {
            value: 2_147_481_748,
            maximum: 2_147_481_747
        })
    );
}

/// Linux's unsigned comparisons reject negative month/hour/minute/second values as well as values
/// at the upper bound (`drivers/rtc/lib.c:163`, :167-169). Every refusal names its value and bounds.
#[test]
fn every_clock_and_month_boundary_has_a_named_refusal() {
    assert_eq!(
        rtc_valid_tm(&value(70, -1, 1, 0, 0, 0)),
        Err(TimeError::MonthOutOfRange {
            value: -1,
            minimum: 0,
            maximum: 11
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(70, 12, 1, 0, 0, 0)),
        Err(TimeError::MonthOutOfRange {
            value: 12,
            minimum: 0,
            maximum: 11
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(70, 0, 1, -1, 0, 0)),
        Err(TimeError::HourOutOfRange {
            value: -1,
            minimum: 0,
            maximum: 23
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(70, 0, 1, 24, 0, 0)),
        Err(TimeError::HourOutOfRange {
            value: 24,
            minimum: 0,
            maximum: 23
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(70, 0, 1, 0, 60, 0)),
        Err(TimeError::MinuteOutOfRange {
            value: 60,
            minimum: 0,
            maximum: 59
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(70, 0, 1, 0, 0, 60)),
        Err(TimeError::SecondOutOfRange {
            value: 60,
            minimum: 0,
            maximum: 59
        })
    );
}

/// Month-specific day checks at leap boundaries (`drivers/rtc/lib.c:164-166`).
#[test]
fn day_range_uses_the_actual_month_and_leap_rule() {
    assert_eq!(rtc_valid_tm(&value(100, 1, 29, 0, 0, 0)), Ok(())); // 2000-02-29
    assert_eq!(
        rtc_valid_tm(&value(200, 1, 29, 0, 0, 0)), // 2100 is not leap
        Err(TimeError::DayOutOfRange {
            value: 29,
            minimum: 1,
            maximum: 28
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(124, 3, 31, 0, 0, 0)), // April
        Err(TimeError::DayOutOfRange {
            value: 31,
            minimum: 1,
            maximum: 30
        })
    );
    assert_eq!(
        rtc_valid_tm(&value(124, 0, 0, 0, 0, 0)),
        Err(TimeError::DayOutOfRange {
            value: 0,
            minimum: 1,
            maximum: 31
        })
    );
}
