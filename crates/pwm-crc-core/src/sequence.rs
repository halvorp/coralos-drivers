// SPDX-License-Identifier: GPL-2.0-only
//! Pure register-write planning for Crystal Cove PWM apply sequencing.
//!
//! Ported from Linux `drivers/pwm/pwm-crc.c`, `crc_pwm_apply` (pwm-crc.c:52-120).
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.

use crate::encode::{
    encode_clock_control, encode_duty_level, validate_state, EncodeError, PwmState,
};
use crate::registers::reg;

/// One regmap write requested by Linux's apply sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterWrite {
    pub register: u8,
    pub value: u8,
}

/// Linux emits at most three writes in one apply (pwm-crc.c:67-117): the backlight-off branch and
/// live-output-clear branch are mutually exclusive, as are backlight off and backlight on.
pub const MAX_APPLY_WRITES: usize = 3;

/// Fixed-capacity, allocation-free write plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApplyPlan {
    writes: [RegisterWrite; MAX_APPLY_WRITES],
    len: usize,
}

impl ApplyPlan {
    /// The writes to perform, in Linux's required order.
    pub fn writes(&self) -> &[RegisterWrite] {
        &self.writes[..self.len]
    }

    fn empty() -> Self {
        Self {
            writes: [RegisterWrite {
                register: 0,
                value: 0,
            }; MAX_APPLY_WRITES],
            len: 0,
        }
    }

    fn push(&mut self, register: u8, value: u8) {
        // Linux's mutually exclusive enable branches cap one apply at MAX_APPLY_WRITES writes.
        debug_assert!(self.len < MAX_APPLY_WRITES);
        self.writes[self.len] = RegisterWrite { register, value };
        self.len += 1;
    }
}

/// Plan Linux's enable/disable and reconfiguration writes without performing MMIO.
///
/// The key ordering is mechanical: disable the backlight before turning output off
/// (pwm-crc.c:67-73), clear output enable before changing a live divider (pwm-crc.c:88-96), and
/// enable the backlight only after the PWM output is configured (pwm-crc.c:111-117).
pub fn plan_apply(current: PwmState, requested: PwmState) -> Result<ApplyPlan, EncodeError> {
    validate_state(requested)?;
    let period_changed = current.period_ns != requested.period_ns;
    let duty_changed = current.duty_ns != requested.duty_ns;
    let enabled_changed = current.enabled != requested.enabled;
    let mut plan = ApplyPlan::empty();

    if current.enabled && !requested.enabled {
        plan.push(reg::BACKLIGHT_EN, 0); // pwm-crc.c:67-68
    }

    if duty_changed || period_changed {
        plan.push(
            reg::PWM0_DUTY_CYCLE,
            encode_duty_level(requested.duty_ns, requested.period_ns)?,
        ); // pwm-crc.c:75-81
    }

    if current.enabled && requested.enabled && period_changed {
        plan.push(reg::PWM0_CLK_DIV, 0); // pwm-crc.c:88-91
    }

    if period_changed || enabled_changed {
        plan.push(
            reg::PWM0_CLK_DIV,
            encode_clock_control(requested.period_ns, requested.enabled)?,
        ); // pwm-crc.c:98-104
    }

    if !current.enabled && requested.enabled {
        plan.push(reg::BACKLIGHT_EN, 1); // pwm-crc.c:111-112
    }

    Ok(plan)
}
