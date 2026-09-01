// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for month lengths, leap years, and days within a year.
//!
//! Ported from Linux `drivers/rtc/lib.c:16-43` and `drivers/rtc/lib.c:119-121`.
//!
//! Copyright (C) 2005-06 Tower Technologies. Original author: Alessandro Zummo.

use rtc_time_core::{
    calendar::{is_leap_year, rtc_month_days, rtc_year_days, RTC_DAYS_IN_MONTH, RTC_YDAYS},
    TimeError,
};

/// `rtc_days_in_month[]` has exactly twelve entries (`drivers/rtc/lib.c:16-18`). The names and
/// literals are written here independently; deriving this expectation from the production table
/// would let an accidental deletion disappear from the test too.
#[test]
fn all_twelve_common_year_month_lengths_match_linux() {
    let expected = [
        ("January", 31_u8),
        ("February", 28),
        ("March", 31),
        ("April", 30),
        ("May", 31),
        ("June", 30),
        ("July", 31),
        ("August", 31),
        ("September", 30),
        ("October", 31),
        ("November", 30),
        ("December", 31),
    ];
    assert_eq!(RTC_DAYS_IN_MONTH.len(), 12);
    assert_eq!(expected.len(), 12);
    for (month, (name, days)) in expected.into_iter().enumerate() {
        assert_eq!(RTC_DAYS_IN_MONTH[month], days, "{name}");
    }
}

/// `rtc_ydays` is two rows of thirteen literals (`drivers/rtc/lib.c:20-25`). Pin the count, row
/// names, column names, and every literal rather than calculating cumulative values from month
/// lengths.
#[test]
fn both_thirteen_entry_year_day_rows_match_linux() {
    let expected = [
        (
            "Normal years",
            [
                0_u16, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334, 365,
            ],
        ),
        (
            "Leap years",
            [
                0_u16, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335, 366,
            ],
        ),
    ];
    let columns = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
        "end of year",
    ];

    assert_eq!(RTC_YDAYS.len(), 2);
    assert_eq!(expected.len(), 2);
    assert_eq!(columns.len(), 13);
    for (row, (row_name, values)) in expected.into_iter().enumerate() {
        assert_eq!(RTC_YDAYS[row].len(), 13, "{row_name}");
        for (column, column_name) in columns.into_iter().enumerate() {
            assert_eq!(
                RTC_YDAYS[row][column], values[column],
                "{row_name}, {column_name}"
            );
        }
    }
}

/// Gregorian century handling corresponding to `drivers/rtc/lib.c:119-121`.
#[test]
fn leap_years_include_four_hundred_years_but_not_other_centuries() {
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(1996));
    assert!(is_leap_year(2000));
    assert!(!is_leap_year(2001));
    assert!(!is_leap_year(2100));
    assert!(is_leap_year(2400));
}

/// `rtc_month_days` adds one only to February in a leap year (`drivers/rtc/lib.c:30-33`).
#[test]
fn month_days_changes_only_leap_february() {
    assert_eq!(rtc_month_days(1, 2000), Ok(29));
    assert_eq!(rtc_month_days(1, 1900), Ok(28));
    assert_eq!(rtc_month_days(0, 2000), Ok(31));
    assert_eq!(rtc_month_days(11, 2000), Ok(31));
    assert_eq!(
        rtc_month_days(12, 2000),
        Err(TimeError::MonthOutOfRange {
            value: 12,
            minimum: 0,
            maximum: 11
        })
    );
}

/// `rtc_year_days` is zero-based despite Linux's later `tm_yday + 1` assignment
/// (`drivers/rtc/lib.c:36-43`, :145).
#[test]
fn year_days_pins_leap_day_and_year_boundaries() {
    assert_eq!(rtc_year_days(1, 0, 2000), Ok(0));
    assert_eq!(rtc_year_days(28, 1, 2000), Ok(58));
    assert_eq!(rtc_year_days(29, 1, 2000), Ok(59));
    assert_eq!(rtc_year_days(1, 2, 2000), Ok(60));
    assert_eq!(rtc_year_days(31, 11, 2000), Ok(365));
    assert_eq!(rtc_year_days(31, 11, 2001), Ok(364));
    assert_eq!(
        rtc_year_days(29, 1, 1900),
        Err(TimeError::DayOutOfRange {
            value: 29,
            minimum: 1,
            maximum: 28
        })
    );
}
