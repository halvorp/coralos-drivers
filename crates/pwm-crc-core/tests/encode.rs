// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for `drivers/pwm/pwm-crc.c` clock-divider/duty encode and decode arithmetic.
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

use pwm_crc_core::encode::{
    decode_state, encode_clock_control, encode_clock_divider, encode_duty_level, validate_state,
    EncodeError, Polarity, PwmState,
};

fn state(period_ns: u64, duty_ns: u64, polarity: Polarity, enabled: bool) -> PwmState {
    PwmState {
        period_ns,
        duty_ns,
        polarity,
        enabled,
    }
}

/// pwm-crc.c:44-47: `6 * period_ns / (256 * 1000)`, then decrement a positive result. Lines
/// :15-17 provide the named divider literals 0, 99, and 127.
#[test]
fn clock_divider_encoding_matches_linux_boundaries() {
    assert_eq!(encode_clock_divider(1), Ok(0));
    assert_eq!(encode_clock_divider(42_666), Ok(0));
    assert_eq!(encode_clock_divider(42_667), Ok(0));
    assert_eq!(encode_clock_divider(85_000), Ok(0));
    assert_eq!(encode_clock_divider(4_266_667), Ok(0x63));
    assert_eq!(encode_clock_divider(5_461_334), Ok(0x7f));
}

/// pwm-crc.c:100-104: enabled contributes BIT(7), while the divider remains bits 6:0.
#[test]
fn clock_control_encoding_adds_exactly_the_enable_bit() {
    assert_eq!(encode_clock_control(4_266_667, false), Ok(0x63));
    assert_eq!(encode_clock_control(4_266_667, true), Ok(0xe3));
}

/// pwm-crc.c:77-81: `duty * 0xff / period`, using integer truncation.
#[test]
fn duty_encoding_matches_linux_literals() {
    assert_eq!(encode_duty_level(0, 1_000), Ok(0x00));
    assert_eq!(encode_duty_level(500, 1_000), Ok(0x7f));
    assert_eq!(encode_duty_level(1_000, 1_000), Ok(0xff));
}

/// pwm-crc.c:142-149: divider is register + 1; both divisions round up; polarity is normal; BIT(7)
/// is enabled. For register 0x7f Linux's arithmetic produces its :25 maximum 5,461,334 ns.
#[test]
fn state_decoding_matches_linux_round_up_arithmetic() {
    assert_eq!(
        decode_state(0xff, 0x7f),
        state(5_461_334, 2_719_959, Polarity::Normal, true)
    );
    assert_eq!(
        decode_state(0x00, 0x00),
        state(42_667, 0, Polarity::Normal, false)
    );
    assert_eq!(
        decode_state(0x80, 0xff),
        state(42_667, 42_667, Polarity::Normal, true)
    );
}

/// pwm-crc.c:59-65 rejects a period above 5,461,334 ns and non-normal polarity. Zero period and
/// duty above period are named refusals for the assumptions at :77-79 rather than a divide panic or
/// an overflowing level.
#[test]
fn validation_names_what_refused_and_why() {
    assert_eq!(
        validate_state(state(1_000, 500, Polarity::Normal, true)),
        Ok(())
    );
    assert_eq!(
        validate_state(state(5_461_335, 0, Polarity::Normal, false)),
        Err(EncodeError::PeriodExceedsMaximum {
            period_ns: 5_461_335,
            maximum_ns: 5_461_334,
        })
    );
    assert_eq!(
        validate_state(state(0, 0, Polarity::Normal, false)),
        Err(EncodeError::PeriodIsZero)
    );
    assert_eq!(
        validate_state(state(1_000, 1_001, Polarity::Normal, false)),
        Err(EncodeError::DutyExceedsPeriod {
            duty_ns: 1_001,
            period_ns: 1_000
        })
    );
    assert_eq!(
        validate_state(state(1_000, 500, Polarity::Inversed, false)),
        Err(EncodeError::UnsupportedPolarity {
            polarity: Polarity::Inversed
        })
    );
}

/// Public encoder refusal vectors pin their own error paths, rather than relying only on
/// `validate_state` coverage.
#[test]
fn individual_encoders_return_named_refusals() {
    assert_eq!(encode_clock_divider(0), Err(EncodeError::PeriodIsZero));
    assert_eq!(
        encode_clock_divider(5_461_335),
        Err(EncodeError::PeriodExceedsMaximum {
            period_ns: 5_461_335,
            maximum_ns: 5_461_334,
        })
    );
    assert_eq!(encode_duty_level(1, 0), Err(EncodeError::PeriodIsZero));
    assert_eq!(
        encode_duty_level(2, 1),
        Err(EncodeError::DutyExceedsPeriod {
            duty_ns: 2,
            period_ns: 1
        })
    );
    assert_eq!(
        encode_clock_control(0, true),
        Err(EncodeError::PeriodIsZero)
    );
}
