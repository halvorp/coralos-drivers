// SPDX-License-Identifier: GPL-2.0-only
//! LPSS PWM duty/period encode and state decode, ported from Linux
//! `drivers/pwm/pwm-lpss.c` and `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use crate::board::BoardInfo;
use crate::regs::{
    base_unit_mask, PWM_BASE_UNIT_SHIFT, PWM_ENABLE, PWM_ON_TIME_DIV_MASK, PWM_SW_UPDATE,
};

/// Nanoseconds in one second, `NSEC_PER_SEC` as used by pwm-lpss.c:130, :228,
/// :230.
pub const NSEC_PER_SEC: u64 = 1_000_000_000;
/// Linux scales duty by 255 (pwm-lpss.c:146-:148, :222-:233).
pub const ON_TIME_SCALE: u64 = 255;

/// A configuration request that Linux's arithmetic cannot encode safely.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    ZeroClockRate { clk_rate: u32 },
    ZeroPeriod { period_ns: u64 },
    DutyExceedsPeriod { duty_ns: u64, period_ns: u64 },
    BaseUnitBitsOutOfRange { bits: u8, min: u8, max: u8 },
}

/// The two writes emitted by `pwm_lpss_prepare`: first the configuration with
/// SW_UPDATE clear, then the same word with SW_UPDATE set (pwm-lpss.c:150-:157).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedWrites {
    pub configured: u32,
    pub committed: u32,
}

/// State reconstructed by Linux `pwm_lpss_get_state` (pwm-lpss.c:209-:241).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedState {
    pub period_ns: u64,
    pub duty_ns: u64,
    pub enabled: bool,
}

/// Encode period and duty into Linux's two prepare writes.
///
/// This preserves unrelated control bits from `current_ctrl`, clears and
/// replaces only ON_TIME_DIV and BASE_UNIT, then sets SW_UPDATE only in the
/// second write (pwm-lpss.c:133-:157).
pub fn prepare_writes(
    current_ctrl: u32,
    info: &BoardInfo,
    duty_ns: u64,
    period_ns: u64,
) -> Result<PreparedWrites, EncodeError> {
    if info.clk_rate == 0 {
        return Err(EncodeError::ZeroClockRate {
            clk_rate: info.clk_rate,
        });
    }
    if period_ns == 0 {
        return Err(EncodeError::ZeroPeriod { period_ns });
    }
    if duty_ns > period_ns {
        return Err(EncodeError::DutyExceedsPeriod { duty_ns, period_ns });
    }
    if info.base_unit_bits == 0 || info.base_unit_bits > 22 {
        return Err(EncodeError::BaseUnitBitsOutOfRange {
            bits: info.base_unit_bits,
            min: 1,
            max: 22,
        });
    }

    // `freq = NSEC_PER_SEC; do_div(freq, period_ns)` (pwm-lpss.c:130, :133).
    let frequency = NSEC_PER_SEC / period_ns;
    // `base_unit_range = BIT(bits); freq *= range; DIV_ROUND_CLOSEST_ULL`
    // (pwm-lpss.c:139-:144). Inputs are bounded above to keep this in u64.
    let range = 1u64 << info.base_unit_bits;
    let numerator = frequency * range;
    let mut base_unit = (numerator + info.clk_rate as u64 / 2) / info.clk_rate as u64;
    // Linux deliberately clamps rather than refusing an unrepresentable period
    // (pwm-lpss.c:143-:144).
    base_unit = base_unit.clamp(1, range - 1);

    // The division truncates before subtraction (pwm-lpss.c:146-:148).
    let on_time_div = ON_TIME_SCALE - (ON_TIME_SCALE * duty_ns / period_ns);

    let mut ctrl = current_ctrl;
    ctrl &= !PWM_SW_UPDATE;
    ctrl &= !PWM_ON_TIME_DIV_MASK;
    ctrl &= !base_unit_mask(info.base_unit_bits);
    ctrl |= (base_unit as u32) << PWM_BASE_UNIT_SHIFT;
    ctrl |= on_time_div as u32;

    Ok(PreparedWrites {
        configured: ctrl,
        committed: ctrl | PWM_SW_UPDATE,
    })
}

/// Decode a sampled configuration register using Linux's integer arithmetic
/// (pwm-lpss.c:219-:237).
pub fn decode_state(ctrl: u32, info: &BoardInfo) -> Result<DecodedState, EncodeError> {
    if info.clk_rate == 0 {
        return Err(EncodeError::ZeroClockRate {
            clk_rate: info.clk_rate,
        });
    }
    if info.base_unit_bits == 0 || info.base_unit_bits > 22 {
        return Err(EncodeError::BaseUnitBitsOutOfRange {
            bits: info.base_unit_bits,
            min: 1,
            max: 22,
        });
    }

    let range = 1u64 << info.base_unit_bits;
    let on_time = ON_TIME_SCALE - (ctrl & PWM_ON_TIME_DIV_MASK) as u64;
    let base_unit = ((ctrl >> PWM_BASE_UNIT_SHIFT) as u64) & (range - 1);
    let frequency = base_unit * info.clk_rate as u64 / range;
    let period_ns = if frequency == 0 {
        NSEC_PER_SEC
    } else {
        NSEC_PER_SEC / frequency
    };
    let duty_ns = on_time * period_ns / ON_TIME_SCALE;

    Ok(DecodedState {
        period_ns,
        duty_ns,
        enabled: ctrl & PWM_ENABLE != 0,
    })
}

/// Return the register word Linux writes to disable an enabled PWM
/// (pwm-lpss.c:201-:203).
pub const fn disable_word(current_ctrl: u32) -> u32 {
    current_ctrl & !PWM_ENABLE
}
