// SPDX-License-Identifier: GPL-2.0-only
//! LPSS PWM update/wait sequencing, ported from Linux
//! `drivers/pwm/pwm-lpss.c` and `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use crate::regs::{PWM_ENABLE, PWM_SW_UPDATE};

/// `readl_poll_timeout(..., 40, ms)` poll interval (pwm-lpss.c:108).
pub const UPDATE_POLL_INTERVAL_US: u32 = 40;
/// `500 * USEC_PER_MSEC` total update budget (pwm-lpss.c:93, :108).
pub const UPDATE_TIMEOUT_US: u32 = 500_000;
/// Linux's timeout diagnostic (pwm-lpss.c:110).
pub const UPDATE_TIMEOUT_MESSAGE: &str = "PWM_SW_UPDATE was not cleared";
/// Linux's pre-update refusal diagnostic (pwm-lpss.c:118).
pub const UPDATE_BUSY_MESSAGE: &str = "PWM_SW_UPDATE is still set, skipping update";

/// Why an update sequence refused to advance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateRefusal {
    SwUpdateStillSet {
        ctrl: u32,
        mask: u32,
        message: &'static str,
    },
    SwUpdateWasNotCleared {
        ctrl: u32,
        timeout_us: u32,
        message: &'static str,
    },
}

/// The caller-visible actions in Linux `pwm_lpss_prepare_enable`
/// (pwm-lpss.c:166-:183).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    WriteConfigured(u32),
    WriteCommitted(u32),
    WriteEnabled(u32),
    PollForUpdateClear { interval_us: u32, timeout_us: u32 },
    Complete,
}

/// Pure state machine for the write and wait ordering. It never accesses MMIO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdateSequence {
    configured: u32,
    committed: u32,
    bypass: bool,
    stage: u8,
}

impl UpdateSequence {
    /// Start only when SW_UPDATE is clear, as required by
    /// `pwm_lpss_is_updating` (pwm-lpss.c:115-:123, :172-:176).
    pub fn start(
        sampled_ctrl: u32,
        configured: u32,
        committed: u32,
        bypass: bool,
    ) -> Result<Self, UpdateRefusal> {
        if sampled_ctrl & PWM_SW_UPDATE != 0 {
            return Err(UpdateRefusal::SwUpdateStillSet {
                ctrl: sampled_ctrl,
                mask: PWM_SW_UPDATE,
                message: UPDATE_BUSY_MESSAGE,
            });
        }
        Ok(Self {
            configured,
            committed,
            bypass,
            stage: 0,
        })
    }

    /// Return the next action. `sampled_ctrl` is consulted only after the poll
    /// action, modelling hardware clearing SW_UPDATE at the next cycle
    /// (pwm-lpss.c:97-:108).
    pub fn advance(&mut self, sampled_ctrl: u32) -> Result<UpdateAction, UpdateRefusal> {
        let action = match (self.bypass, self.stage) {
            (_, 0) => UpdateAction::WriteConfigured(self.configured),
            (_, 1) => UpdateAction::WriteCommitted(self.committed),
            // `pwm_lpss_cond_enable` performs a fresh read-modify-write
            // (pwm-lpss.c:160-:164), so use the caller's current sample rather
            // than assuming the preceding committed word is still present.
            (false, 2) => UpdateAction::WriteEnabled(sampled_ctrl | PWM_ENABLE),
            (false, 3) | (true, 2) => UpdateAction::PollForUpdateClear {
                interval_us: UPDATE_POLL_INTERVAL_US,
                timeout_us: UPDATE_TIMEOUT_US,
            },
            (false, 4) | (true, 3) if sampled_ctrl & PWM_SW_UPDATE != 0 => {
                return Err(UpdateRefusal::SwUpdateWasNotCleared {
                    ctrl: sampled_ctrl,
                    timeout_us: UPDATE_TIMEOUT_US,
                    message: UPDATE_TIMEOUT_MESSAGE,
                });
            }
            (false, 4) => UpdateAction::Complete,
            (true, 3) => UpdateAction::WriteEnabled(sampled_ctrl | PWM_ENABLE),
            (true, 4) => UpdateAction::Complete,
            _ => UpdateAction::Complete,
        };
        self.stage = self.stage.saturating_add(1);
        Ok(action)
    }
}

/// Whether the update bit has cleared, the success predicate in
/// `pwm_lpss_wait_for_update` (pwm-lpss.c:108).
pub const fn update_cleared(ctrl: u32) -> bool {
    ctrl & PWM_SW_UPDATE == 0
}
