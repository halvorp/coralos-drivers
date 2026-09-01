// SPDX-License-Identifier: GPL-2.0-only
//! Explicit CMD6 busy polling over SEND_STATUS samples.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c`: `mmc_busy_cb`
//! (mmc_ops.c:468-:510), `__mmc_poll_for_busy` (mmc_ops.c:512-:547), and the
//! CMD6 call site (mmc_ops.c:648). R1 state/ready literals come from
//! `include/linux/mmc/mmc.h:154-:177` through `mmc-core-cmd`.
//!
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.

use mmc_core_cmd::status::{current_state, CardState, StatusError};

/// Initial software-poll delay when no period is supplied.
pub const INITIAL_POLL_DELAY_US: u32 = 32; // mmc_ops.c:519
/// Maximum software-poll delay.
pub const MAX_POLL_DELAY_US: u32 = 32_768; // mmc_ops.c:519
/// Native R1 SWITCH_ERROR.
pub const R1_SWITCH_ERROR: u32 = 1 << 7; // include/linux/mmc/mmc.h:156

/// The externally visible busy-poll state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyPoll {
    /// The card has not met BOTH readiness conditions, so CMD13 must be polled
    /// again after this delay.
    PollAgain { next_delay_us: u32 },
    /// The card reports READY_FOR_DATA and TRAN; the next command is now legal.
    Ready,
}

/// Named refusal from a busy poll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyRefusal {
    /// Expiration is checked before sampling, but Linux only times out when the
    /// sample still says busy (mmc_ops.c:524-:537).
    CardStuckBusy {
        elapsed_ms: u32,
        timeout_ms: u32,
        status: u32,
    },
    /// CMD6 status reports that the switch itself failed.
    SwitchError { status: u32 },
    /// The R1 current-state field contains an undefined value.
    ReservedCardState { value: u8, maximum_defined: u8 },
}

/// One clock-free state machine modelling Linux's CMD13 busy loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyPoller {
    timeout_ms: u32,
    delay_us: u32,
}

impl BusyPoller {
    /// Start with Linux's 32us default backoff (mmc_ops.c:519).
    pub fn new(timeout_ms: u32) -> Self {
        Self {
            timeout_ms,
            delay_us: INITIAL_POLL_DELAY_US,
        }
    }

    /// Start with an explicit period, matching `period_us ? period_us : 32`.
    pub fn with_period(timeout_ms: u32, period_us: u32) -> Self {
        Self {
            timeout_ms,
            delay_us: if period_us == 0 {
                INITIAL_POLL_DELAY_US
            } else {
                period_us
            },
        }
    }

    /// Consume one SEND_STATUS response at `elapsed_ms`.
    ///
    /// Readiness requires BOTH R1_READY_FOR_DATA and R1_STATE_TRAN. A PRG card
    /// remains busy even if it spuriously raises READY_FOR_DATA, and a TRAN card
    /// remains busy if READY_FOR_DATA is clear (mmc.h:170-:177; mmc_ops.c:506).
    /// The timeout boundary follows Linux's `time_after`, not `>=`: elapsed
    /// exactly equal to the budget still gets a sample; a still-busy sample
    /// strictly after the budget is refused (mmc_ops.c:524, :532-:537).
    pub fn sample(&mut self, elapsed_ms: u32, status: u32) -> Result<BusyPoll, BusyRefusal> {
        if status & R1_SWITCH_ERROR != 0 {
            return Err(BusyRefusal::SwitchError { status });
        }

        let state = current_state(status).map_err(map_state_error)?;
        let ready =
            status & mmc_core_cmd::status::READY_FOR_DATA != 0 && state == CardState::Transfer;

        if ready {
            return Ok(BusyPoll::Ready);
        }

        if elapsed_ms > self.timeout_ms {
            return Err(BusyRefusal::CardStuckBusy {
                elapsed_ms,
                timeout_ms: self.timeout_ms,
                status,
            });
        }

        let current_delay = self.delay_us;
        if self.delay_us < MAX_POLL_DELAY_US {
            self.delay_us = self.delay_us.saturating_mul(2);
        }
        Ok(BusyPoll::PollAgain {
            next_delay_us: current_delay,
        })
    }
}

fn map_state_error(error: StatusError) -> BusyRefusal {
    match error {
        StatusError::ReservedCardState {
            value,
            maximum_defined,
        } => BusyRefusal::ReservedCardState {
            value,
            maximum_defined,
        },
    }
}
