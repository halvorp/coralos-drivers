// SPDX-License-Identifier: GPL-2.0-only
//! LPSS PWM register layout, ported from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

/// PWM configuration register within each channel. `PWM` (pwm-lpss.c:27).
pub const PWM: u32 = 0x0000_0000;
/// Distance between PWM channel register spaces. `PWM_SIZE` (pwm-lpss.c:33-34).
pub const PWM_SIZE: u32 = 0x400;
/// Output enable. `PWM_ENABLE` (pwm-lpss.c:28).
pub const PWM_ENABLE: u32 = 1 << 31;
/// Commit request, cleared by hardware at the next output cycle. `PWM_SW_UPDATE`
/// (pwm-lpss.c:29, :97-:106).
pub const PWM_SW_UPDATE: u32 = 1 << 30;
/// Base-unit field starts at bit 8. `PWM_BASE_UNIT_SHIFT` (pwm-lpss.c:30).
pub const PWM_BASE_UNIT_SHIFT: u32 = 8;
/// Inverted duty divisor in bits 7:0. `PWM_ON_TIME_DIV_MASK` (pwm-lpss.c:31).
pub const PWM_ON_TIME_DIV_MASK: u32 = 0xff;
/// Maximum channel count accepted by the core. `LPSS_MAX_PWMS` (pwm-lpss.h:17).
pub const LPSS_MAX_PWMS: u8 = 4;

/// Byte offset of one channel's PWM configuration register.
///
/// Port of `regs + pwm->hwpwm * PWM_SIZE + PWM` (pwm-lpss.c:75-:86).
pub const fn channel_offset(channel: u8) -> u32 {
    channel as u32 * PWM_SIZE + PWM
}

/// Mask of the variable-width base-unit field.
///
/// Port of `(base_unit_range - 1) << PWM_BASE_UNIT_SHIFT`, where
/// `base_unit_range = BIT(base_unit_bits)` (pwm-lpss.c:139, :152).
pub const fn base_unit_mask(base_unit_bits: u8) -> u32 {
    ((1u32 << base_unit_bits) - 1) << PWM_BASE_UNIT_SHIFT
}
