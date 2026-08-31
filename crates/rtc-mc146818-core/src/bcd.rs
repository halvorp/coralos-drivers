// SPDX-License-Identifier: GPL-2.0-only
//! Binary-coded-decimal conversion used by the MC146818 time path.
//!
//! Ported from Linux `include/linux/bcd.h:20-22` and
//! `drivers/rtc/rtc-mc146818-lib.c:165-175,268-275`; copyright the Linux BCD and RTC authors,
//! Torsten Duwe, and Motorola.

/// Convert one packed-BCD byte exactly as Linux `const_bcd2bin` does.
pub const fn bcd_to_binary(value: u8) -> u8 {
    (value & 0x0f) + (value >> 4) * 10 // include/linux/bcd.h:20
}

/// Convert a binary value to packed BCD exactly as Linux `const_bin2bcd` does.
///
/// The caller is responsible for selecting a value representable by its target register.
pub const fn binary_to_bcd(value: u8) -> u8 {
    ((value / 10) << 4) + value % 10 // include/linux/bcd.h:21
}

/// Whether both nibbles form decimal digits, matching Linux `const_bcd_is_valid`.
pub const fn bcd_is_valid(value: u8) -> bool {
    (value & 0x0f) < 10 && (value >> 4) < 10 // include/linux/bcd.h:22
}
