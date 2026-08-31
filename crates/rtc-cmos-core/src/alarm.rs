// SPDX-License-Identifier: GPL-2.0-only
//! CMOS alarm readback, programming, range validation, and capability selection.
//!
//! Ported from Linux `drivers/rtc/rtc-cmos.c:247-340,401-559,918-920,1010-1033`; original
//! copyright Paul Gortmaker, David Brownell, and the Linux RTC authors.

use rtc_mc146818_core::{bcd, time::uses_bcd};

pub const UIP_AVOID_TIMEOUT_MS: u32 = 10; // drivers/rtc/rtc-cmos.c:305,554
pub const SECS_PER_DAY: i64 = 24 * 60 * 60; // drivers/rtc/rtc-cmos.c:918
pub const SECS_PER_MONTH: i64 = 28 * SECS_PER_DAY; // drivers/rtc/rtc-cmos.c:919
pub const SECS_PER_YEAR: i64 = 365 * SECS_PER_DAY; // drivers/rtc/rtc-cmos.c:920
pub const ACPI_REGISTER_LIMIT: u8 = 128; // drivers/rtc/rtc-cmos.c:1010-1017

/// Alarm reach selected by Linux from the enhanced alarm registers present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmRange {
    Day,
    Month,
    Year,
}

/// The three alarm ranges Linux names, in increasing capability order.
pub const ALARM_RANGES: [(&str, AlarmRange); 3] = [
    ("day", AlarmRange::Day),
    ("month", AlarmRange::Month),
    ("year", AlarmRange::Year),
]; // drivers/rtc/rtc-cmos.c:1123-1125

/// Raw alarm fields as stored in CMOS. `day` and `month` are absent without enhanced registers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawAlarm {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: Option<u8>,
    pub month: Option<u8>,
}

/// Linux RTC alarm fields. `-1` is Linux's wildcard/unavailable sentinel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlarmTime {
    pub second: i16,
    pub minute: i16,
    pub hour: i16,
    pub day: i16,
    /// Zero-based month, or `-1`.
    pub month: i16,
}

/// Alarm state decoded from the fields and register B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedAlarm {
    pub time: AlarmTime,
    pub enabled: bool,
    pub pending: bool,
}

/// CMOS values to write when programming an alarm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EncodedAlarm {
    pub second: u8,
    pub minute: u8,
    pub hour: u8,
    pub day: Option<u8>,
    pub month: Option<u8>,
}

/// Named refusal for an alarm later than the hardware can distinguish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlarmRefusal {
    AlarmBeyondDay { alarm: i64, latest: i64 },
    AlarmBeyondMonth { alarm: i64, latest: i64 },
    AlarmBeyondYear { alarm: i64, latest: i64 },
}

/// ACPI alarm register numbers must be in bank one (`< 128`); Linux turns larger values into zero.
pub const fn sanitize_acpi_register(register: u8) -> u8 {
    if register >= ACPI_REGISTER_LIMIT { 0 } else { register }
} // drivers/rtc/rtc-cmos.c:1010-1017

/// Select day/month/year reach from the enhanced alarm registers.
pub const fn alarm_range(day_register: u8, month_register: u8) -> AlarmRange {
    if month_register != 0 {
        AlarmRange::Year
    } else if day_register != 0 {
        AlarmRange::Month
    } else {
        AlarmRange::Day
    }
} // drivers/rtc/rtc-cmos.c:1028-1033

/// Linux's advertised maximum alarm offset for the selected register set.
pub const fn alarm_offset_max(range: AlarmRange) -> i64 {
    match range {
        AlarmRange::Day => SECS_PER_DAY - 1,
        AlarmRange::Month => SECS_PER_MONTH - 1,
        AlarmRange::Year => SECS_PER_YEAR - 1,
    }
} // drivers/rtc/rtc-cmos.c:1028-1033

/// Validate against the exact calendar boundary Linux computes.
pub const fn validate_alarm(
    range: AlarmRange,
    alarm_timestamp: i64,
    maximum_timestamp: i64,
) -> Result<(), AlarmRefusal> {
    if alarm_timestamp <= maximum_timestamp {
        return Ok(());
    }
    Err(match range {
        AlarmRange::Day => AlarmRefusal::AlarmBeyondDay {
            alarm: alarm_timestamp,
            latest: maximum_timestamp,
        },
        AlarmRange::Month => AlarmRefusal::AlarmBeyondMonth {
            alarm: alarm_timestamp,
            latest: maximum_timestamp,
        },
        AlarmRange::Year => AlarmRefusal::AlarmBeyondYear {
            alarm: alarm_timestamp,
            latest: maximum_timestamp,
        },
    })
} // drivers/rtc/rtc-cmos.c:408-463

/// Decode alarm registers with Linux's per-field BCD validity limits.
pub fn decode_alarm(raw: RawAlarm, control_b: u8, always_bcd: bool) -> DecodedAlarm {
    let mut time = AlarmTime {
        second: raw.second as i16,
        minute: raw.minute as i16,
        hour: raw.hour as i16,
        day: raw.day.map_or(-1, |v| (v & 0x3f) as i16),
        month: raw.month.map_or(-1, |v| v as i16),
    }; // drivers/rtc/rtc-cmos.c:260-277

    if time.day == 0 {
        time.day = -1;
    }
    if time.month == 0 {
        time.month = -1;
    } // drivers/rtc/rtc-cmos.c:264-274

    if uses_bcd(control_b, always_bcd) {
        time.second = decode_bcd_below(raw.second, 0x60);
        time.minute = decode_bcd_below(raw.minute, 0x60);
        time.hour = decode_bcd_below(raw.hour, 0x24);
        if let Some(value) = raw.day {
            let value = value & 0x3f;
            time.day = if value != 0 && value <= 0x31 {
                bcd::bcd_to_binary(value) as i16
            } else {
                -1
            };
        }
        if let Some(value) = raw.month {
            time.month = if value <= 0x12 {
                bcd::bcd_to_binary(value) as i16 - 1
            } else {
                -1
            };
        }
    } // drivers/rtc/rtc-cmos.c:308-335

    DecodedAlarm {
        time,
        enabled: control_b & rtc_mc146818_core::registers::AIE != 0,
        pending: false,
    }
} // drivers/rtc/rtc-cmos.c:337-340

const fn decode_bcd_below(value: u8, limit: u8) -> i16 {
    if value < limit { bcd::bcd_to_binary(value) as i16 } else { -1 }
}

/// Encode alarm fields. In BCD mode Linux writes `0xff` for an invalid/wildcard field.
pub fn encode_alarm(
    time: AlarmTime,
    control_b: u8,
    always_bcd: bool,
    day_register: u8,
    month_register: u8,
) -> EncodedAlarm {
    let mut encoded = EncodedAlarm {
        month: (month_register != 0).then_some(wrapping_u8(time.month + 1)),
        day: (day_register != 0).then_some(wrapping_u8(time.day)),
        hour: wrapping_u8(time.hour),
        minute: wrapping_u8(time.minute),
        second: wrapping_u8(time.second),
    }; // drivers/rtc/rtc-cmos.c:529-533

    if uses_bcd(control_b, always_bcd) {
        encoded.month = encoded.month.map(|v| {
            if v <= 12 { bcd::binary_to_bcd(v) } else { 0xff }
        });
        encoded.day = encoded.day.map(|v| {
            if (1..=31).contains(&v) { bcd::binary_to_bcd(v) } else { 0xff }
        });
        encoded.hour = encode_bcd_below(encoded.hour, 24);
        encoded.minute = encode_bcd_below(encoded.minute, 60);
        encoded.second = encode_bcd_below(encoded.second, 60);
    } // drivers/rtc/rtc-cmos.c:539-546

    encoded
}

const fn wrapping_u8(value: i16) -> u8 {
    value as u8
}

const fn encode_bcd_below(value: u8, limit: u8) -> u8 {
    if value < limit { bcd::binary_to_bcd(value) } else { 0xff }
}
