// SPDX-License-Identifier: GPL-2.0-only
//! LPSS PWM board variants, ported from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

/// Static hardware data corresponding to Linux `struct pwm_lpss_boardinfo` as
/// consumed by pwm-lpss.c:37-:68.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardInfo {
    pub name: &'static str,
    pub clk_rate: u32,
    pub npwm: u8,
    pub base_unit_bits: u8,
    pub bypass: bool,
    pub other_devices_aml_touches_pwm_regs: bool,
}

/// BayTrail board data (pwm-lpss.c:36-:41).
pub const BYT: BoardInfo = BoardInfo {
    name: "BayTrail",
    clk_rate: 25_000_000,
    npwm: 1,
    base_unit_bits: 16,
    bypass: false,
    other_devices_aml_touches_pwm_regs: false,
};
/// Braswell board data (pwm-lpss.c:44-:50).
pub const BSW: BoardInfo = BoardInfo {
    name: "Braswell",
    clk_rate: 19_200_000,
    npwm: 1,
    base_unit_bits: 16,
    bypass: false,
    other_devices_aml_touches_pwm_regs: true,
};
/// Broxton board data (pwm-lpss.c:53-:59).
pub const BXT: BoardInfo = BoardInfo {
    name: "Broxton",
    clk_rate: 19_200_000,
    npwm: 4,
    base_unit_bits: 22,
    bypass: true,
    other_devices_aml_touches_pwm_regs: false,
};
/// Tangier board data (pwm-lpss.c:62-:68).
pub const TNG: BoardInfo = BoardInfo {
    name: "Tangier",
    clk_rate: 19_200_000,
    npwm: 4,
    base_unit_bits: 22,
    bypass: false,
    other_devices_aml_touches_pwm_regs: false,
};

/// The four exported Linux board-info objects (pwm-lpss.h:24-:27;
/// pwm-lpss.c:37-:68).
pub const BOARD_INFOS: [BoardInfo; 4] = [BYT, BSW, BXT, TNG];

/// Why board data cannot be used by this pure core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardInfoError {
    /// Linux refuses `npwm > LPSS_MAX_PWMS` (pwm-lpss.c:258-:259).
    TooManyPwms { npwm: u8, max: u8 },
    /// Linux refuses a zero clock rate (pwm-lpss.c:269-:271).
    ZeroClockRate { clk_rate: u32 },
    /// The encoded base-unit mask must fit below `PWM_SW_UPDATE`.
    BaseUnitBitsOutOfRange { bits: u8, min: u8, max: u8 },
}

/// Validate the board constraints Linux checks before operating the device.
pub const fn validate(info: &BoardInfo) -> Result<(), BoardInfoError> {
    if info.npwm > crate::regs::LPSS_MAX_PWMS {
        return Err(BoardInfoError::TooManyPwms {
            npwm: info.npwm,
            max: crate::regs::LPSS_MAX_PWMS,
        });
    }
    if info.clk_rate == 0 {
        return Err(BoardInfoError::ZeroClockRate {
            clk_rate: info.clk_rate,
        });
    }
    // Linux's shipped values are 16 and 22 (pwm-lpss.c:40, :48, :57, :66).
    // The upper bound follows from shift 8 and SW_UPDATE at bit 30.
    if info.base_unit_bits == 0 || info.base_unit_bits > 22 {
        return Err(BoardInfoError::BaseUnitBitsOutOfRange {
            bits: info.base_unit_bits,
            min: 1,
            max: 22,
        });
    }
    Ok(())
}
