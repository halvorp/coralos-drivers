// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove PWM register and hardware literals.
//!
//! Ported from Linux `drivers/pwm/pwm-crc.c`.
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

/// PMIC register addresses used by the PWM controller.
pub mod reg {
    pub const PWM0_CLK_DIV: u8 = 0x4b; // pwm-crc.c:13
    pub const PWM0_DUTY_CYCLE: u8 = 0x4e; // pwm-crc.c:19
    pub const BACKLIGHT_EN: u8 = 0x51; // pwm-crc.c:20
}

/// Clock-divider register values and fields.
pub mod clk_div {
    pub const OUTPUT_ENABLE: u8 = 1 << 7; // pwm-crc.c:14, BIT(7)
    pub const DIV_CLK_0: u8 = 0x00; // pwm-crc.c:15, BASECLK
    pub const DIV_CLK_100: u8 = 0x63; // pwm-crc.c:16, BASECLK / 100
    pub const DIV_CLK_128: u8 = 0x7f; // pwm-crc.c:17, BASECLK / 128
}

pub const MAX_LEVEL: u8 = 0xff; // pwm-crc.c:22
pub const BASE_CLK_MHZ: u64 = 6; // pwm-crc.c:24
pub const MAX_PERIOD_NS: u64 = 5_461_334; // pwm-crc.c:25
pub const PWM_CHANNEL_COUNT: usize = 1; // pwm-crc.c:166
pub const PWM_CHANNEL_NAMES: [&str; PWM_CHANNEL_COUNT] = ["pwm0"]; // pwm-crc.c:13, :19, :166
pub const DRIVER_NAME: &str = "crystal_cove_pwm"; // pwm-crc.c:182
