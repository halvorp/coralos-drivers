// SPDX-License-Identifier: GPL-2.0-only
//! Encode/decode vectors from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use pwm_lpss_core::board::{BoardInfo, BSW};
use pwm_lpss_core::encode::{
    decode_state, disable_word, prepare_writes, DecodedState, EncodeError, NSEC_PER_SEC,
    ON_TIME_SCALE,
};

/// Linux literals used by the arithmetic (pwm-lpss.c:130, :146-:148, :228-:233).
#[test]
fn arithmetic_scales_match_linux() {
    assert_eq!(NSEC_PER_SEC, 1_000_000_000); // pwm-lpss.c:130, :228, :230
    assert_eq!(ON_TIME_SCALE, 255); // pwm-lpss.c:146-:148, :222-:233
}

/// Braswell literals are clk=19.2 MHz and a 16-bit base unit
/// (pwm-lpss.c:45-:50). For 100 us, frequency=10000, and Linux rounds
/// 655360000/19200000 to 34. A 25 us duty truncates 255/4 to 63, then stores
/// 255-63=192. These expected register words are written out literally.
#[test]
fn prepare_encodes_period_and_inverted_duty_then_requests_update() {
    let writes = prepare_writes(0x8000_0000, &BSW, 25_000, 100_000).unwrap();
    assert_eq!(writes.configured, 0x8000_22c0); // pwm-lpss.c:133-:156
    assert_eq!(writes.committed, 0xc000_22c0); // pwm-lpss.c:157
}

/// pwm-lpss.c:143-:148 — base unit clamps to one at the slow end; duty 0
/// stores an inverted divisor of 255.
#[test]
fn prepare_preserves_linux_clamp_and_zero_duty_encoding() {
    let writes = prepare_writes(0, &BSW, 0, 10_000_000).unwrap();
    assert_eq!(writes.configured, 0x0000_01ff); // clamp base=1; on_time_div=255
    assert_eq!(writes.committed, 0x4000_01ff); // PWM_SW_UPDATE, pwm-lpss.c:157
}

/// pwm-lpss.c:219-:237. From base=34 and inverted divisor=192, Linux computes
/// freq=9960, period=100401 ns, and duty=63*100401/255=24804 ns.
#[test]
fn get_state_decodes_with_linux_integer_truncation() {
    assert_eq!(
        decode_state(0x8000_22c0, &BSW),
        Ok(DecodedState {
            period_ns: 100_401,
            duty_ns: 24_804,
            enabled: true
        })
    );
    // A zero base unit takes Linux's explicit one-second fallback (:227-:230).
    assert_eq!(
        decode_state(0x0000_00ff, &BSW),
        Ok(DecodedState {
            period_ns: 1_000_000_000,
            duty_ns: 0,
            enabled: false
        })
    );
}

/// pwm-lpss.c:201-:203 clears only PWM_ENABLE.
#[test]
fn disabling_clears_enable_and_preserves_every_other_bit() {
    assert_eq!(disable_word(0xffff_ffff), 0x7fff_ffff);
    assert_eq!(disable_word(0x4000_22c0), 0x4000_22c0);
}

fn custom(clk_rate: u32, bits: u8) -> BoardInfo {
    BoardInfo {
        name: "test",
        clk_rate,
        npwm: 1,
        base_unit_bits: bits,
        bypass: false,
        other_devices_aml_touches_pwm_regs: false,
    }
}

/// Refusals carry the rejected value and its bound; none silently clamps an
/// invalid API relation or risks Linux's integer division by zero.
#[test]
fn invalid_requests_are_named() {
    assert_eq!(
        prepare_writes(0, &BSW, 1, 0),
        Err(EncodeError::ZeroPeriod { period_ns: 0 })
    );
    assert_eq!(
        prepare_writes(0, &BSW, 101, 100),
        Err(EncodeError::DutyExceedsPeriod {
            duty_ns: 101,
            period_ns: 100
        })
    );
    assert_eq!(
        prepare_writes(0, &custom(0, 16), 0, 100),
        Err(EncodeError::ZeroClockRate { clk_rate: 0 })
    );
    assert_eq!(
        decode_state(0, &custom(19_200_000, 23)),
        Err(EncodeError::BaseUnitBitsOutOfRange {
            bits: 23,
            min: 1,
            max: 22
        })
    );
}
