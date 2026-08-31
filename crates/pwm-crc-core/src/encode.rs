// SPDX-License-Identifier: GPL-2.0-only
//! Clock-divider/duty encoding and state decoding for Crystal Cove PWM.
//!
//! Ported from Linux `drivers/pwm/pwm-crc.c`: `crc_pwm_calc_clk_div` (pwm-crc.c:40-50),
//! duty encoding (pwm-crc.c:75-81), and `crc_pwm_get_state` (pwm-crc.c:122-152).
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

use crate::registers::{clk_div, BASE_CLK_MHZ, MAX_LEVEL, MAX_PERIOD_NS};

const NSEC_PER_USEC: u64 = 1_000; // pwm-crc.c:44, :145 (`NSEC_PER_USEC`)

/// Normal is the sole polarity accepted by Linux (pwm-crc.c:64-65, :148).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    Normal,
    Inversed,
}

/// A hardware-independent PWM state, with period and duty in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PwmState {
    pub period_ns: u64,
    pub duty_ns: u64,
    pub polarity: Polarity,
    pub enabled: bool,
}

/// A refusal produced before any write is planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    PeriodExceedsMaximum { period_ns: u64, maximum_ns: u64 },
    PeriodIsZero,
    DutyExceedsPeriod { duty_ns: u64, period_ns: u64 },
    UnsupportedPolarity { polarity: Polarity },
}

/// Validate the conditions assumed by Linux's apply arithmetic.
pub fn validate_state(state: PwmState) -> Result<(), EncodeError> {
    if state.period_ns > MAX_PERIOD_NS {
        return Err(EncodeError::PeriodExceedsMaximum {
            period_ns: state.period_ns,
            maximum_ns: MAX_PERIOD_NS,
        });
    }
    if state.polarity != Polarity::Normal {
        return Err(EncodeError::UnsupportedPolarity {
            polarity: state.polarity,
        });
    }
    if state.period_ns == 0 {
        return Err(EncodeError::PeriodIsZero);
    }
    if state.duty_ns > state.period_ns {
        return Err(EncodeError::DutyExceedsPeriod {
            duty_ns: state.duty_ns,
            period_ns: state.period_ns,
        });
    }
    Ok(())
}

/// Encode period as `(6 * period_ns / (256 * 1000)) - 1`, except zero remains zero
/// (pwm-crc.c:40-49). The maximum-period validation makes the result fit bits 6:0.
pub fn encode_clock_divider(period_ns: u64) -> Result<u8, EncodeError> {
    if period_ns > MAX_PERIOD_NS {
        return Err(EncodeError::PeriodExceedsMaximum {
            period_ns,
            maximum_ns: MAX_PERIOD_NS,
        });
    }
    if period_ns == 0 {
        return Err(EncodeError::PeriodIsZero);
    }

    let quotient = BASE_CLK_MHZ * period_ns / (256 * NSEC_PER_USEC);
    Ok(if quotient > 0 {
        (quotient - 1) as u8
    } else {
        0
    })
}

/// Encode duty as `duty_ns * 0xff / period_ns`, truncating as Linux does
/// (pwm-crc.c:75-81).
pub fn encode_duty_level(duty_ns: u64, period_ns: u64) -> Result<u8, EncodeError> {
    if period_ns == 0 {
        return Err(EncodeError::PeriodIsZero);
    }
    if duty_ns > period_ns {
        return Err(EncodeError::DutyExceedsPeriod { duty_ns, period_ns });
    }
    Ok((u128::from(duty_ns) * u128::from(MAX_LEVEL) / u128::from(period_ns)) as u8)
}

/// Add the output-enable bit to an encoded clock divider (pwm-crc.c:100-104).
pub fn encode_clock_control(period_ns: u64, enabled: bool) -> Result<u8, EncodeError> {
    let divider = encode_clock_divider(period_ns)?;
    Ok(divider | if enabled { clk_div::OUTPUT_ENABLE } else { 0 })
}

/// Decode the two PMIC registers using Linux's round-up arithmetic (pwm-crc.c:142-149).
pub fn decode_state(clock_control: u8, duty_level: u8) -> PwmState {
    let divider = u64::from(clock_control & !clk_div::OUTPUT_ENABLE) + 1;
    let period_ns = div_round_up(divider * NSEC_PER_USEC * 256, BASE_CLK_MHZ);
    let duty_ns = div_round_up(u64::from(duty_level) * period_ns, u64::from(MAX_LEVEL));

    PwmState {
        period_ns,
        duty_ns,
        polarity: Polarity::Normal,
        enabled: clock_control & clk_div::OUTPUT_ENABLE != 0,
    }
}

fn div_round_up(numerator: u64, denominator: u64) -> u64 {
    numerator / denominator + u64::from(!numerator.is_multiple_of(denominator))
}
