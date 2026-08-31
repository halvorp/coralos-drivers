// SPDX-License-Identifier: GPL-2.0-only
//! Register vectors from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use pwm_lpss_core::regs::*;

/// pwm-lpss.c:27-:34 and pwm-lpss.h:17. All expectations are frozen Linux
/// literals, not expressions derived from the constants under test.
#[test]
fn register_literals_match_linux() {
    assert_eq!(PWM, 0x0000_0000); // pwm-lpss.c:27
    assert_eq!(PWM_ENABLE, 0x8000_0000); // pwm-lpss.c:28, BIT(31)
    assert_eq!(PWM_SW_UPDATE, 0x4000_0000); // pwm-lpss.c:29, BIT(30)
    assert_eq!(PWM_BASE_UNIT_SHIFT, 8); // pwm-lpss.c:30
    assert_eq!(PWM_ON_TIME_DIV_MASK, 0x0000_00ff); // pwm-lpss.c:31, GENMASK(7, 0)
    assert_eq!(PWM_SIZE, 0x400); // pwm-lpss.c:34
    assert_eq!(LPSS_MAX_PWMS, 4); // pwm-lpss.h:17
}

/// pwm-lpss.c:79, :86 — `hwpwm * PWM_SIZE + PWM`.
#[test]
fn every_linux_channel_offset_is_pinned() {
    let got = [
        channel_offset(0),
        channel_offset(1),
        channel_offset(2),
        channel_offset(3),
    ];
    assert_eq!(got.len(), 4); // pwm-lpss.h:17
    assert_eq!(got, [0x000, 0x400, 0x800, 0xc00]); // pwm-lpss.c:27, :34, :79
}

/// pwm-lpss.c:40/:48 and :57/:66 provide the two field widths; :152 shifts
/// `(base_unit_range - 1)` by 8.
#[test]
fn base_unit_masks_match_both_linux_widths() {
    assert_eq!(base_unit_mask(16), 0x00ff_ff00); // pwm-lpss.c:40, :48, :152
    assert_eq!(base_unit_mask(22), 0x3fff_ff00); // pwm-lpss.c:57, :66, :152
    assert_eq!(base_unit_mask(16) & 0x0000_00ff, 0);
    assert_eq!(base_unit_mask(22) & 0xc000_0000, 0);
}
