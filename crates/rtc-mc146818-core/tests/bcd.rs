// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux packed-BCD helpers.
//!
//! Ported from `include/linux/bcd.h:20-22`; copyright the Linux BCD and RTC authors.

use rtc_mc146818_core::bcd::{bcd_is_valid, bcd_to_binary, binary_to_bcd};

/// include/linux/bcd.h:20 — `((x & 0x0f) + (x >> 4) * 10)`.
#[test]
fn packed_bcd_converts_to_binary() {
    assert_eq!(bcd_to_binary(0x00), 0);
    assert_eq!(bcd_to_binary(0x09), 9);
    assert_eq!(bcd_to_binary(0x42), 42);
    assert_eq!(bcd_to_binary(0x59), 59);
    assert_eq!(bcd_to_binary(0x99), 99);
}

/// include/linux/bcd.h:21 — `(((x / 10) << 4) + x % 10)`.
#[test]
fn binary_converts_to_packed_bcd() {
    assert_eq!(binary_to_bcd(0), 0x00);
    assert_eq!(binary_to_bcd(9), 0x09);
    assert_eq!(binary_to_bcd(42), 0x42);
    assert_eq!(binary_to_bcd(59), 0x59);
    assert_eq!(binary_to_bcd(99), 0x99);
}

/// include/linux/bcd.h:22 — each nibble must be below ten.
#[test]
fn validity_checks_both_decimal_digits() {
    assert!(bcd_is_valid(0x00));
    assert!(bcd_is_valid(0x59));
    assert!(bcd_is_valid(0x99));
    assert!(!bcd_is_valid(0x0a), "low nibble is not a decimal digit");
    assert!(!bcd_is_valid(0xa0), "high nibble is not a decimal digit");
}
