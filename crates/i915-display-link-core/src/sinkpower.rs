// SPDX-License-Identifier: GPL-2.0-only
//! `intel_dp_set_power()`: put the sink in D0 (with Linux's three 1 ms retries) or D3 over DPCD `DP_SET_POWER`.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dp.c:3778-3821 (v7.0-rc6). Copyright (c) 2008 Intel Corporation.
//! NOT PORTED: LSPCON resume / PCON mode wait, the source-OUI write and the `downstream_hpd_needs_d0` D3 veto (an eDP panel
//! has no downstream port).
use crate::dpcd::{DpcdFail, DpcdOk, DpcdRequest};
use crate::dpdefs::*;

/// "When turning on, we need to retry for 1ms to give the sink time to wake up": `for (i = 0; i < 3; i++)` — intel_dp.c:3805.
pub const SET_POWER_D0_TRIES: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerStep {
    /// Perform this DPCD write; answer with `Some(result)`.
    Write(DpcdRequest),
    /// `msleep(1)`; answer `None`.
    SleepMs(u32),
    /// Finished. `Ok` = the write that succeeded; `Err` = every try failed (Linux only logs "Set power to D0 failed").
    Done(Result<(), SetPowerError>),
}

/// The sink never accepted the power state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetPowerError {
    /// `dpcd[DP_DPCD_REV] < 0x11` — the function is a no-op (intel_dp.c:3785).
    NotApplicable,
    /// All tries failed; carries the last failure.
    Failed { mode: u8, last: DpcdFail },
    /// The executor answered a step with the wrong kind of reply (no write outstanding).
    OutOfSequence,
}

impl SetPowerError {
    pub const fn name(&self) -> &'static str {
        match self {
            SetPowerError::NotApplicable => "SET-POWER-NOT-APPLICABLE",
            SetPowerError::Failed { .. } => "SET-POWER-FAILED",
            SetPowerError::OutOfSequence => "SET-POWER-OUT-OF-SEQUENCE",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SetPower {
    mode: u8,
    max_tries: u32,
    tries: u32,
    sleeping: bool,
    started: bool,
}

impl SetPower {
    /// `mode` = `DP_SET_POWER_D0` or `DP_SET_POWER_D3`. `dpcd_rev` is `dpcd[DP_DPCD_REV]`.
    pub fn new(mode: u8, dpcd_rev: u8) -> Result<SetPower, SetPowerError> {
        if dpcd_rev < DP_DPCD_REV_11 {
            return Err(SetPowerError::NotApplicable);
        }
        // D3 is a single write; D0 retries.
        let max_tries = if mode == DP_SET_POWER_D0 { SET_POWER_D0_TRIES } else { 1 };
        Ok(SetPower { mode, max_tries, tries: 0, sleeping: false, started: false })
    }

    fn write(&self) -> PowerStep {
        PowerStep::Write(DpcdRequest::write(DP_SET_POWER, &[self.mode]))
    }

    /// First call passes `None`; afterwards the DPCD result of the last [`PowerStep::Write`], or `None` after a sleep.
    pub fn step(&mut self, reply: Option<Result<DpcdOk, DpcdFail>>) -> PowerStep {
        if !self.started {
            self.started = true;
            return self.write();
        }
        if self.sleeping {
            self.sleeping = false;
            return self.write();
        }
        match reply {
            Some(Ok(ok)) if ok.len == 1 => PowerStep::Done(Ok(())),
            Some(r) => {
                self.tries += 1;
                // `ret == 1` is the only success (:3807); a short ACK is a failed try like any other.
                let last = match r {
                    Err(e) => e,
                    Ok(ok) => DpcdFail { first: crate::dpcd::DpcdErr::Short { got: ok.len, want: 1 }, attempts: 1 },
                };
                if self.tries >= self.max_tries {
                    return PowerStep::Done(Err(SetPowerError::Failed { mode: self.mode, last }));
                }
                self.sleeping = true;
                PowerStep::SleepMs(1)
            }
            None => PowerStep::Done(Err(SetPowerError::OutOfSequence)),
        }
    }
}
