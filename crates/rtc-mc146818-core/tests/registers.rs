// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for MC146818 register definitions.
//!
//! Ported from `include/linux/mc146818rtc.h`, copyright Torsten Duwe, Motorola, and Linux authors.

use rtc_mc146818_core::registers::*;

/// include/linux/mc146818rtc.h:50-69 defines exactly fourteen summary registers. The expected
/// names and values are handwritten Linux literals, not derived from `REGISTERS`.
#[test]
fn all_fourteen_register_names_and_indices_match_linux() {
    let expected = [
        ("RTC_SECONDS", 0u8),
        ("RTC_SECONDS_ALARM", 1),
        ("RTC_MINUTES", 2),
        ("RTC_MINUTES_ALARM", 3),
        ("RTC_HOURS", 4),
        ("RTC_HOURS_ALARM", 5),
        ("RTC_DAY_OF_WEEK", 6),
        ("RTC_DAY_OF_MONTH", 7),
        ("RTC_MONTH", 8),
        ("RTC_YEAR", 9),
        ("RTC_REG_A", 10),
        ("RTC_REG_B", 11),
        ("RTC_REG_C", 12),
        ("RTC_REG_D", 13),
    ];
    assert_eq!(REGISTERS.len(), 14);
    assert_eq!(
        REGISTERS.map(|register| (register.name, register.index)),
        expected
    );
}

/// include/linux/mc146818rtc.h:74,95,106,114 defines exactly four detail aliases.
#[test]
fn all_four_register_alias_names_and_values_match_linux() {
    let expected = [
        ("RTC_FREQ_SELECT", 10u8),
        ("RTC_CONTROL", 11),
        ("RTC_INTR_FLAGS", 12),
        ("RTC_VALID", 13),
    ];
    assert_eq!(REGISTER_ALIASES.len(), 4);
    assert_eq!(
        REGISTER_ALIASES.map(|item| (item.name, item.value)),
        expected
    );
}

/// include/linux/mc146818rtc.h:56-115 defines exactly twenty-three alarm/register detail values.
/// Pinning every name catches deletion from the production list; literals catch wrong masks.
#[test]
fn all_twenty_three_field_names_and_values_match_linux() {
    let expected = [
        ("RTC_ALARM_DONT_CARE", 0xc0u8), // include/linux/mc146818rtc.h:56-57
        ("RTC_UIP", 0x80),               // include/linux/mc146818rtc.h:80
        ("RTC_DIV_CTL", 0x70),           // include/linux/mc146818rtc.h:81
        ("RTC_REF_CLCK_4MHZ", 0x00),     // include/linux/mc146818rtc.h:83
        ("RTC_REF_CLCK_1MHZ", 0x10),     // include/linux/mc146818rtc.h:84
        ("RTC_REF_CLCK_32KHZ", 0x20),    // include/linux/mc146818rtc.h:85
        ("RTC_DIV_RESET1", 0x60),        // include/linux/mc146818rtc.h:87
        ("RTC_DIV_RESET2", 0x70),        // include/linux/mc146818rtc.h:88
        ("RTC_AMD_BANK_SELECT", 0x10),   // include/linux/mc146818rtc.h:90
        ("RTC_RATE_SELECT", 0x0f),       // include/linux/mc146818rtc.h:92
        ("RTC_SET", 0x80),               // include/linux/mc146818rtc.h:96
        ("RTC_PIE", 0x40),               // include/linux/mc146818rtc.h:97
        ("RTC_AIE", 0x20),               // include/linux/mc146818rtc.h:98
        ("RTC_UIE", 0x10),               // include/linux/mc146818rtc.h:99
        ("RTC_SQWE", 0x08),              // include/linux/mc146818rtc.h:100
        ("RTC_DM_BINARY", 0x04),         // include/linux/mc146818rtc.h:101
        ("RTC_24H", 0x02),               // include/linux/mc146818rtc.h:102
        ("RTC_DST_EN", 0x01),            // include/linux/mc146818rtc.h:103
        ("RTC_IRQF", 0x80),              // include/linux/mc146818rtc.h:108
        ("RTC_PF", 0x40),                // include/linux/mc146818rtc.h:109
        ("RTC_AF", 0x20),                // include/linux/mc146818rtc.h:110
        ("RTC_UF", 0x10),                // include/linux/mc146818rtc.h:111
        ("RTC_VRT", 0x80),               // include/linux/mc146818rtc.h:115
    ];
    assert_eq!(REGISTER_FIELDS.len(), 23);
    assert_eq!(
        REGISTER_FIELDS.map(|item| (item.name, item.value)),
        expected
    );
}
