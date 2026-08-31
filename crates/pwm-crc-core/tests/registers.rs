// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux literal vectors for `drivers/pwm/pwm-crc.c` registers and counts.
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

use pwm_crc_core::registers::{
    clk_div, reg, BASE_CLK_MHZ, DRIVER_NAME, MAX_LEVEL, MAX_PERIOD_NS, PWM_CHANNEL_COUNT,
    PWM_CHANNEL_NAMES,
};

/// pwm-crc.c:13, :19-20.
#[test]
fn register_addresses_match_linux() {
    assert_eq!(reg::PWM0_CLK_DIV, 0x4b);
    assert_eq!(reg::PWM0_DUTY_CYCLE, 0x4e);
    assert_eq!(reg::BACKLIGHT_EN, 0x51);
}

/// pwm-crc.c:14-17.
#[test]
fn divider_literals_match_linux() {
    assert_eq!(clk_div::OUTPUT_ENABLE, 1 << 7);
    assert_eq!(clk_div::DIV_CLK_0, 0x00);
    assert_eq!(clk_div::DIV_CLK_100, 0x63);
    assert_eq!(clk_div::DIV_CLK_128, 0x7f);
}

/// pwm-crc.c:22, :24-25.
#[test]
fn arithmetic_limits_match_linux() {
    assert_eq!(MAX_LEVEL, 0xff);
    assert_eq!(BASE_CLK_MHZ, 6);
    assert_eq!(MAX_PERIOD_NS, 5_461_334);
}

/// pwm-crc.c:13, :19 and :166 define exactly one channel, PWM0. The expected list is frozen here,
/// not generated from `PWM_CHANNEL_NAMES`.
#[test]
fn linuxs_single_pwm_channel_count_and_name_are_pinned() {
    assert_eq!(PWM_CHANNEL_COUNT, 1);
    assert_eq!(PWM_CHANNEL_NAMES, ["pwm0"]);
    assert_eq!(PWM_CHANNEL_NAMES.len(), 1);
}

/// pwm-crc.c:182.
#[test]
fn platform_driver_name_matches_linux() {
    assert_eq!(DRIVER_NAME, "crystal_cove_pwm");
}
