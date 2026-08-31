// SPDX-License-Identifier: GPL-2.0-only
//! Time-register conversion, year pivoting, and optional ACPI century handling.
//!
//! Ported from Linux `drivers/rtc/rtc-mc146818-lib.c:100-198,211-307`, with register mode bits
//! from `include/linux/mc146818rtc.h:95-103`; copyright Torsten Duwe, Motorola, and the Linux RTC
//! authors.

use crate::{bcd, registers};

/// The six RTC values Linux reads; day-of-week is deliberately ignored
/// (`drivers/rtc/rtc-mc146818-lib.c:115-126`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RawTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    /// Hardware month, 1 through 12.
    pub month: u8,
    /// Hardware's two-digit year, unless no century register is available and 100..169 is being
    /// accepted for Linux's historical range.
    pub year: u8,
}

/// Linux `struct rtc_time` conventions for the fields this chip supplies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RtcTime {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: u8,
    /// Zero-based month.
    pub month: u8,
    /// Years since 1900.
    pub year: u16,
}

/// Encoded writes for the RTC and optional ACPI century register.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedTime {
    pub raw: RawTime,
    /// `None` means the platform did not provide a century register.
    pub century: Option<u8>,
}

/// A named refusal from Linux's year limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// `tm_year` cannot enter Linux's unsigned-byte preparation path.
    YearExceedsUnsignedByte { year: u16, maximum: u16 },
    /// Without an ACPI century register Linux accepts at most 2069 (`tm_year == 169`).
    YearExceedsNoCenturyRange { year: u16, maximum: u16 },
}

/// Whether Linux converts register values as BCD.
pub const fn uses_bcd(control: u8, always_bcd: bool) -> bool {
    control & registers::DM_BINARY == 0 || always_bcd
} // drivers/rtc/rtc-mc146818-lib.c:165,268

/// Decode one stable register snapshot using Linux's BCD mode, century, pivot, and month rules.
pub fn decode_time(
    mut raw: RawTime,
    control: u8,
    always_bcd: bool,
    mut century: Option<u8>,
) -> RtcTime {
    if uses_bcd(control, always_bcd) {
        raw.second = bcd::bcd_to_binary(raw.second);
        raw.minute = bcd::bcd_to_binary(raw.minute);
        raw.hour = bcd::bcd_to_binary(raw.hour);
        raw.day = bcd::bcd_to_binary(raw.day);
        raw.month = bcd::bcd_to_binary(raw.month);
        raw.year = bcd::bcd_to_binary(raw.year);
        century = century.map(bcd::bcd_to_binary);
    } // drivers/rtc/rtc-mc146818-lib.c:165-175

    let mut year = raw.year as u16;
    if let Some(value) = century {
        if value > 19 {
            year += (value as u16 - 19) * 100;
        }
    } // drivers/rtc/rtc-mc146818-lib.c:182-185

    if year <= 69 {
        year += 100;
    } // drivers/rtc/rtc-mc146818-lib.c:187-192

    RtcTime {
        second: raw.second,
        minute: raw.minute,
        hour: raw.hour,
        day: raw.day,
        month: raw.month.wrapping_sub(1), // drivers/rtc/rtc-mc146818-lib.c:194
        year,
    }
}

/// Encode writes using Linux's year limits, optional ACPI century split, and BCD mode.
pub fn encode_time(
    time: RtcTime,
    control: u8,
    always_bcd: bool,
    has_century_register: bool,
) -> Result<EncodedTime, EncodeError> {
    if time.year > 255 {
        return Err(EncodeError::YearExceedsUnsignedByte {
            year: time.year,
            maximum: 255,
        });
    } // drivers/rtc/rtc-mc146818-lib.c:223-231

    let mut year = time.year;
    let mut century = None;
    if has_century_register {
        century = Some(((year + 1900) / 100) as u8);
        year %= 100;
    } // drivers/rtc/rtc-mc146818-lib.c:248-253

    if year > 169 {
        return Err(EncodeError::YearExceedsNoCenturyRange { year, maximum: 169 });
    } // drivers/rtc/rtc-mc146818-lib.c:256-260
    if year >= 100 {
        year -= 100;
    } // drivers/rtc/rtc-mc146818-lib.c:262-263

    let mut raw = RawTime {
        second: time.second,
        minute: time.minute,
        hour: time.hour,
        day: time.day,
        month: time.month.wrapping_add(1), // drivers/rtc/rtc-mc146818-lib.c:223-228
        year: year as u8,
    };

    if uses_bcd(control, always_bcd) {
        raw.second = bcd::binary_to_bcd(raw.second);
        raw.minute = bcd::binary_to_bcd(raw.minute);
        raw.hour = bcd::binary_to_bcd(raw.hour);
        raw.day = bcd::binary_to_bcd(raw.day);
        raw.month = bcd::binary_to_bcd(raw.month);
        raw.year = bcd::binary_to_bcd(raw.year);
        century = century.map(bcd::binary_to_bcd);
    } // drivers/rtc/rtc-mc146818-lib.c:268-275

    Ok(EncodedTime { raw, century })
}

/// Register A value Linux writes while setting the clock.
///
/// AMD and Hygon systems clear the alternate-century bank selector; other systems reset divider
/// stage two. The caller supplies the platform decision, so this remains pure.
pub const fn set_mode_frequency_value(saved: u8, amd_register_a_behavior: bool) -> u8 {
    if amd_register_a_behavior {
        saved & !registers::AMD_BANK_SELECT
    } else {
        saved | registers::DIV_RESET2
    }
} // drivers/rtc/rtc-mc146818-lib.c:200-209,281-285
