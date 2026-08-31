// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for `drivers/pwm/pwm-crc.c` apply and enable sequencing.
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

use pwm_crc_core::encode::{EncodeError, Polarity, PwmState};
use pwm_crc_core::sequence::{plan_apply, RegisterWrite, MAX_APPLY_WRITES};

fn state(period_ns: u64, duty_ns: u64, enabled: bool) -> PwmState {
    PwmState {
        period_ns,
        duty_ns,
        polarity: Polarity::Normal,
        enabled,
    }
}

fn write(register: u8, value: u8) -> RegisterWrite {
    RegisterWrite { register, value }
}

/// pwm-crc.c:67-68, :98-104. Disable first writes BACKLIGHT_EN(0x51)=0, then writes the disabled
/// clock-control value to PWM0_CLK_DIV(0x4b).
#[test]
fn disabling_blanks_backlight_before_disabling_pwm_output() {
    let plan = plan_apply(
        state(4_266_667, 2_133_333, true),
        state(4_266_667, 2_133_333, false),
    )
    .unwrap();
    assert_eq!(plan.writes(), [write(0x51, 0x00), write(0x4b, 0x63)]);
}

/// pwm-crc.c:98-112. Enabling configures PWM0_CLK_DIV first and BACKLIGHT_EN last, so the
/// backlight cannot expose an unconfigured PWM output.
#[test]
fn enabling_configures_pwm_before_enabling_backlight() {
    let plan = plan_apply(
        state(4_266_667, 2_133_333, false),
        state(4_266_667, 2_133_333, true),
    )
    .unwrap();
    assert_eq!(plan.writes(), [write(0x4b, 0xe3), write(0x51, 0x01)]);
}

/// pwm-crc.c:75-104. A live period change writes duty, clears PWM_OUTPUT_ENABLE, then installs the
/// new divider with output enabled. Omitting or moving the clear violates Linux's explicit :90
/// sequencing requirement.
#[test]
fn live_period_change_clears_output_before_installing_new_divider() {
    let plan = plan_apply(
        state(4_266_667, 2_133_333, true),
        state(5_461_334, 2_730_667, true),
    )
    .unwrap();
    assert_eq!(
        plan.writes(),
        [write(0x4e, 0x7f), write(0x4b, 0x00), write(0x4b, 0xff),]
    );
}

/// pwm-crc.c:75-86. Duty-only changes write only PWM0_DUTY_CYCLE; they do not disturb the divider
/// or either enable.
#[test]
fn duty_only_change_writes_only_duty_register() {
    let plan = plan_apply(state(1_000, 250, true), state(1_000, 500, true)).unwrap();
    assert_eq!(plan.writes(), [write(0x4e, 0x7f)]);
}

/// pwm-crc.c:67-117. No changed state field means no regmap write.
#[test]
fn unchanged_state_has_an_empty_plan() {
    let current = state(1_000, 500, true);
    assert!(plan_apply(current, current).unwrap().writes().is_empty());
}

/// pwm-crc.c:67-117 can emit at most three writes in one apply: backlight-off and live-clear are
/// mutually exclusive, as are backlight-off and backlight-on. A live period change reaches three.
#[test]
fn maximum_write_count_is_pinned() {
    assert_eq!(MAX_APPLY_WRITES, 3);
    let plan = plan_apply(
        state(4_266_667, 2_133_333, true),
        state(5_461_334, 2_730_667, false),
    )
    .unwrap();
    assert_eq!(
        plan.writes(),
        [write(0x51, 0), write(0x4e, 0x7f), write(0x4b, 0x7f)]
    );
    assert_eq!(plan.writes().len(), 3);
}

/// Validation happens before planning, matching pwm-crc.c:59-65. The polarity vector is important:
/// downstream numeric encoders cannot reject it, so this fails if up-front validation is skipped.
#[test]
fn plan_refuses_invalid_request_before_any_write() {
    assert_eq!(
        plan_apply(state(1_000, 500, false), state(5_461_335, 0, false)),
        Err(EncodeError::PeriodExceedsMaximum {
            period_ns: 5_461_335,
            maximum_ns: 5_461_334,
        })
    );
    let inversed = PwmState {
        period_ns: 1_000,
        duty_ns: 500,
        polarity: Polarity::Inversed,
        enabled: true,
    };
    assert_eq!(
        plan_apply(state(1_000, 500, false), inversed),
        Err(EncodeError::UnsupportedPolarity {
            polarity: Polarity::Inversed
        })
    );
}

/// `ApplyPlan::writes` is itself public API and this vector pins its order and exact slice length.
#[test]
fn writes_accessor_exposes_only_populated_entries() {
    let plan = plan_apply(state(1_000, 0, false), state(1_000, 1_000, false)).unwrap();
    assert_eq!(plan.writes().len(), 1);
    assert_eq!(plan.writes()[0], write(0x4e, 0xff));
}
