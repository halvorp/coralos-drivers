// SPDX-License-Identifier: GPL-2.0-only
//! Pure RTC calendar arithmetic ported from Linux `drivers/rtc/lib.c`.
//!
//! This crate contains the arithmetic from `rtc_month_days`, `rtc_year_days`,
//! `rtc_time64_to_tm`, `rtc_valid_tm`, and `rtc_tm_to_time64`; it deliberately contains no BCD,
//! century-register, MMIO, or I/O handling.
//!
//! Copyright (C) 2005-06 Tower Technologies.
//! Original authors: Alessandro Zummo and Cassio Neri.

#![no_std]
#![forbid(unsafe_code)]

pub mod calendar;
pub mod conversion;
pub mod validation;

/// Linux's `struct rtc_time` field representation (`drivers/rtc/lib.c:46-52`, :141-152).
///
/// Months are zero-based. `tm_year` is years since 1900, weekdays use Sunday = 0, and this Linux
/// source writes `tm_yday` as 1 through 366.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RtcTime {
    pub tm_sec: i32,
    pub tm_min: i32,
    pub tm_hour: i32,
    pub tm_mday: i32,
    pub tm_mon: i32,
    pub tm_year: i32,
    pub tm_wday: i32,
    pub tm_yday: i32,
    pub tm_isdst: i32,
}

/// A named refusal from calendar validation or conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeError {
    Time64BelowMinimum {
        value: i64,
        minimum: i64,
    },
    Time64AboveMaximum {
        value: i64,
        maximum: i64,
    },
    YearBelowMinimum {
        value: i32,
        minimum: i32,
    },
    YearAboveMaximum {
        value: i32,
        maximum: i32,
    },
    MonthOutOfRange {
        value: i32,
        minimum: i32,
        maximum: i32,
    },
    DayOutOfRange {
        value: i32,
        minimum: i32,
        maximum: i32,
    },
    HourOutOfRange {
        value: i32,
        minimum: i32,
        maximum: i32,
    },
    MinuteOutOfRange {
        value: i32,
        minimum: i32,
        maximum: i32,
    },
    SecondOutOfRange {
        value: i32,
        minimum: i32,
        maximum: i32,
    },
}
