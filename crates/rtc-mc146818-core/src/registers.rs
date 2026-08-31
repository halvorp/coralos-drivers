// SPDX-License-Identifier: GPL-2.0-only
//! MC146818 register indices and bit masks.
//!
//! Ported from Linux `include/linux/mc146818rtc.h` (written by Torsten Duwe, derived from the
//! Motorola data sheet) and `drivers/rtc/rtc-mc146818-lib.c`; copyright Torsten Duwe, Motorola,
//! and the Linux RTC authors.

/// One named Linux register index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Register {
    pub name: &'static str,
    pub index: u8,
}

/// One named Linux register alias, bit, or field mask.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedValue {
    pub name: &'static str,
    pub value: u8,
}

pub const SECONDS: u8 = 0; // include/linux/mc146818rtc.h:50
pub const SECONDS_ALARM: u8 = 1; // include/linux/mc146818rtc.h:51
pub const MINUTES: u8 = 2; // include/linux/mc146818rtc.h:52
pub const MINUTES_ALARM: u8 = 3; // include/linux/mc146818rtc.h:53
pub const HOURS: u8 = 4; // include/linux/mc146818rtc.h:54
pub const HOURS_ALARM: u8 = 5; // include/linux/mc146818rtc.h:55
pub const DAY_OF_WEEK: u8 = 6; // include/linux/mc146818rtc.h:59
pub const DAY_OF_MONTH: u8 = 7; // include/linux/mc146818rtc.h:60
pub const MONTH: u8 = 8; // include/linux/mc146818rtc.h:61
pub const YEAR: u8 = 9; // include/linux/mc146818rtc.h:62
pub const REG_A: u8 = 10; // include/linux/mc146818rtc.h:66
pub const REG_B: u8 = 11; // include/linux/mc146818rtc.h:67
pub const REG_C: u8 = 12; // include/linux/mc146818rtc.h:68
pub const REG_D: u8 = 13; // include/linux/mc146818rtc.h:69

/// The complete fourteen-register MC146818 register summary in Linux header order.
pub const REGISTERS: [Register; 14] = [
    Register {
        name: "RTC_SECONDS",
        index: SECONDS,
    },
    Register {
        name: "RTC_SECONDS_ALARM",
        index: SECONDS_ALARM,
    },
    Register {
        name: "RTC_MINUTES",
        index: MINUTES,
    },
    Register {
        name: "RTC_MINUTES_ALARM",
        index: MINUTES_ALARM,
    },
    Register {
        name: "RTC_HOURS",
        index: HOURS,
    },
    Register {
        name: "RTC_HOURS_ALARM",
        index: HOURS_ALARM,
    },
    Register {
        name: "RTC_DAY_OF_WEEK",
        index: DAY_OF_WEEK,
    },
    Register {
        name: "RTC_DAY_OF_MONTH",
        index: DAY_OF_MONTH,
    },
    Register {
        name: "RTC_MONTH",
        index: MONTH,
    },
    Register {
        name: "RTC_YEAR",
        index: YEAR,
    },
    Register {
        name: "RTC_REG_A",
        index: REG_A,
    },
    Register {
        name: "RTC_REG_B",
        index: REG_B,
    },
    Register {
        name: "RTC_REG_C",
        index: REG_C,
    },
    Register {
        name: "RTC_REG_D",
        index: REG_D,
    },
]; // include/linux/mc146818rtc.h:50-69

pub const FREQ_SELECT: u8 = REG_A; // include/linux/mc146818rtc.h:74
pub const CONTROL: u8 = REG_B; // include/linux/mc146818rtc.h:95
pub const INTR_FLAGS: u8 = REG_C; // include/linux/mc146818rtc.h:106
pub const VALID: u8 = REG_D; // include/linux/mc146818rtc.h:114

pub const ALARM_DONT_CARE: u8 = 0xc0; // include/linux/mc146818rtc.h:56-57
pub const UIP: u8 = 0x80; // include/linux/mc146818rtc.h:76-80
pub const DIV_CTL: u8 = 0x70; // include/linux/mc146818rtc.h:81
pub const REF_CLCK_4MHZ: u8 = 0x00; // include/linux/mc146818rtc.h:83
pub const REF_CLCK_1MHZ: u8 = 0x10; // include/linux/mc146818rtc.h:84
pub const REF_CLCK_32KHZ: u8 = 0x20; // include/linux/mc146818rtc.h:85
pub const DIV_RESET1: u8 = 0x60; // include/linux/mc146818rtc.h:87
pub const DIV_RESET2: u8 = 0x70; // include/linux/mc146818rtc.h:88
pub const AMD_BANK_SELECT: u8 = 0x10; // include/linux/mc146818rtc.h:89-90
pub const RATE_SELECT: u8 = 0x0f; // include/linux/mc146818rtc.h:91-92

pub const SET: u8 = 0x80; // include/linux/mc146818rtc.h:96
pub const PIE: u8 = 0x40; // include/linux/mc146818rtc.h:97
pub const AIE: u8 = 0x20; // include/linux/mc146818rtc.h:98
pub const UIE: u8 = 0x10; // include/linux/mc146818rtc.h:99
pub const SQWE: u8 = 0x08; // include/linux/mc146818rtc.h:100
pub const DM_BINARY: u8 = 0x04; // include/linux/mc146818rtc.h:101
pub const HOUR_24: u8 = 0x02; // include/linux/mc146818rtc.h:102
pub const DST_EN: u8 = 0x01; // include/linux/mc146818rtc.h:103

pub const IRQF: u8 = 0x80; // include/linux/mc146818rtc.h:108
pub const PF: u8 = 0x40; // include/linux/mc146818rtc.h:109
pub const AF: u8 = 0x20; // include/linux/mc146818rtc.h:110
pub const UF: u8 = 0x10; // include/linux/mc146818rtc.h:111
pub const VRT: u8 = 0x80; // include/linux/mc146818rtc.h:115

/// All four Moto-name register aliases in Linux's register details.
pub const REGISTER_ALIASES: [NamedValue; 4] = [
    NamedValue {
        name: "RTC_FREQ_SELECT",
        value: FREQ_SELECT,
    },
    NamedValue {
        name: "RTC_CONTROL",
        value: CONTROL,
    },
    NamedValue {
        name: "RTC_INTR_FLAGS",
        value: INTR_FLAGS,
    },
    NamedValue {
        name: "RTC_VALID",
        value: VALID,
    },
]; // include/linux/mc146818rtc.h:74,95,106,114

/// All twenty-three alarm/register-detail values in Linux's header.
pub const REGISTER_FIELDS: [NamedValue; 23] = [
    NamedValue {
        name: "RTC_ALARM_DONT_CARE",
        value: ALARM_DONT_CARE,
    },
    NamedValue {
        name: "RTC_UIP",
        value: UIP,
    },
    NamedValue {
        name: "RTC_DIV_CTL",
        value: DIV_CTL,
    },
    NamedValue {
        name: "RTC_REF_CLCK_4MHZ",
        value: REF_CLCK_4MHZ,
    },
    NamedValue {
        name: "RTC_REF_CLCK_1MHZ",
        value: REF_CLCK_1MHZ,
    },
    NamedValue {
        name: "RTC_REF_CLCK_32KHZ",
        value: REF_CLCK_32KHZ,
    },
    NamedValue {
        name: "RTC_DIV_RESET1",
        value: DIV_RESET1,
    },
    NamedValue {
        name: "RTC_DIV_RESET2",
        value: DIV_RESET2,
    },
    NamedValue {
        name: "RTC_AMD_BANK_SELECT",
        value: AMD_BANK_SELECT,
    },
    NamedValue {
        name: "RTC_RATE_SELECT",
        value: RATE_SELECT,
    },
    NamedValue {
        name: "RTC_SET",
        value: SET,
    },
    NamedValue {
        name: "RTC_PIE",
        value: PIE,
    },
    NamedValue {
        name: "RTC_AIE",
        value: AIE,
    },
    NamedValue {
        name: "RTC_UIE",
        value: UIE,
    },
    NamedValue {
        name: "RTC_SQWE",
        value: SQWE,
    },
    NamedValue {
        name: "RTC_DM_BINARY",
        value: DM_BINARY,
    },
    NamedValue {
        name: "RTC_24H",
        value: HOUR_24,
    },
    NamedValue {
        name: "RTC_DST_EN",
        value: DST_EN,
    },
    NamedValue {
        name: "RTC_IRQF",
        value: IRQF,
    },
    NamedValue {
        name: "RTC_PF",
        value: PF,
    },
    NamedValue {
        name: "RTC_AF",
        value: AF,
    },
    NamedValue {
        name: "RTC_UF",
        value: UF,
    },
    NamedValue {
        name: "RTC_VRT",
        value: VRT,
    },
]; // include/linux/mc146818rtc.h:56-115
