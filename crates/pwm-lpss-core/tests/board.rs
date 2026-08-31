// SPDX-License-Identifier: GPL-2.0-only
//! Board-data vectors from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use pwm_lpss_core::board::{validate, BoardInfo, BoardInfoError, BOARD_INFOS};

/// Linux exports exactly FOUR board-info objects (pwm-lpss.h:24-:27). Names
/// and values are written literally so deleting a production entry cannot
/// delete its own test case.
#[test]
fn all_four_linux_board_infos_are_present_and_named() {
    assert_eq!(BOARD_INFOS.len(), 4); // pwm-lpss.h:24-:27
    let names = [
        BOARD_INFOS[0].name,
        BOARD_INFOS[1].name,
        BOARD_INFOS[2].name,
        BOARD_INFOS[3].name,
    ];
    assert_eq!(names, ["BayTrail", "Braswell", "Broxton", "Tangier"]); // pwm-lpss.c:36, :44, :53, :62

    assert_eq!(
        (
            BOARD_INFOS[0].clk_rate,
            BOARD_INFOS[0].npwm,
            BOARD_INFOS[0].base_unit_bits,
            BOARD_INFOS[0].bypass,
            BOARD_INFOS[0].other_devices_aml_touches_pwm_regs
        ),
        (25_000_000, 1, 16, false, false)
    ); // pwm-lpss.c:37-:41
    assert_eq!(
        (
            BOARD_INFOS[1].clk_rate,
            BOARD_INFOS[1].npwm,
            BOARD_INFOS[1].base_unit_bits,
            BOARD_INFOS[1].bypass,
            BOARD_INFOS[1].other_devices_aml_touches_pwm_regs
        ),
        (19_200_000, 1, 16, false, true)
    ); // pwm-lpss.c:45-:50
    assert_eq!(
        (
            BOARD_INFOS[2].clk_rate,
            BOARD_INFOS[2].npwm,
            BOARD_INFOS[2].base_unit_bits,
            BOARD_INFOS[2].bypass,
            BOARD_INFOS[2].other_devices_aml_touches_pwm_regs
        ),
        (19_200_000, 4, 22, true, false)
    ); // pwm-lpss.c:54-:59
    assert_eq!(
        (
            BOARD_INFOS[3].clk_rate,
            BOARD_INFOS[3].npwm,
            BOARD_INFOS[3].base_unit_bits,
            BOARD_INFOS[3].bypass,
            BOARD_INFOS[3].other_devices_aml_touches_pwm_regs
        ),
        (19_200_000, 4, 22, false, false)
    ); // pwm-lpss.c:63-:68
}

fn sample() -> BoardInfo {
    BoardInfo {
        name: "test",
        clk_rate: 19_200_000,
        npwm: 1,
        base_unit_bits: 16,
        bypass: false,
        other_devices_aml_touches_pwm_regs: false,
    }
}

/// pwm-lpss.c:258-:271 names both probe refusals; the pure port carries the
/// rejected value and bound instead of returning a bare false.
#[test]
fn validation_names_what_refused_and_why() {
    assert_eq!(validate(&sample()), Ok(()));

    let mut too_many = sample();
    too_many.npwm = 5;
    assert_eq!(
        validate(&too_many),
        Err(BoardInfoError::TooManyPwms { npwm: 5, max: 4 }) // pwm-lpss.h:17; pwm-lpss.c:258-:259
    );

    let mut no_clock = sample();
    no_clock.clk_rate = 0;
    assert_eq!(
        validate(&no_clock),
        Err(BoardInfoError::ZeroClockRate { clk_rate: 0 }) // pwm-lpss.c:269-:271
    );

    let mut too_wide = sample();
    too_wide.base_unit_bits = 23;
    assert_eq!(
        validate(&too_wide),
        Err(BoardInfoError::BaseUnitBitsOutOfRange {
            bits: 23,
            min: 1,
            max: 22
        })
    );
}
