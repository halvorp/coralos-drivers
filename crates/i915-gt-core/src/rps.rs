// SPDX-License-Identifier: GPL-2.0-only
//! Render P-State (RPS) waitboost and idle frequency planner for Gen9 GT.
//!
//! Ported from Linux v7.0-rc6 `drivers/gpu/drm/i915/gt/intel_rps.c`:
//! - `gen6_rps_set_thresholds` / `rps_set_power` (`gt/intel_rps.c:680-744`) for
//!   threshold-interrupt RP0 selection in the full hardware-interrupt governor,
//! - `gen6_rps_set` (`gt/intel_rps.c:802-824`) for programming `GEN6_RPNSWREQ`
//!   software frequency requests via `RpsLayout::swreq`,
//! - `intel_rps_park` (`gt/intel_rps.c:896`) for dropping back to `idle_freq`
//!   (`rpn`) when the GT parks on idle,
//! - `intel_rps_boost` (`gt/intel_rps.c:1020`) for boosting the GT request to
//!   `boost_freq` (`rp0`) while work is active,
//! - `rps_reset` (`gt/intel_rps.c:1221-1236`) and `intel_rps_set_manual`
//!   (`gt/intel_rps.c:2650-2657`) for placing `GEN6_RP_CONTROL` into software
//!   control mode (`GEN9_RPSWCTL_ENABLE`).
//!
//! This module implements the interrupt-less boost-on-submit / park-on-idle
//! subset of RPS: the caller reports busyness after each device poll and the
//! planner drives the software frequency request between `rpn` (idle) and `rp0`
//! (boost). All time and counter arithmetic saturates.

use crate::effect::{Effect, WritePlan};
use i915_common_core::Reg;
use i915_devinfo_core::rps::{RpsCaps, RpsLayout};
use i915_gt_regs_core::{
    GEN6_RPNSWREQ, GEN6_RP_CONTROL, GEN9_RPSWCTL_ENABLE,
};

/// Idle holdoff in milliseconds before decaying from RP0 back to RPn.
pub const BOOST_IDLE_MS: u64 = 100;

/// Interval in milliseconds between busy frequency telemetry samples.
pub const FREQ_LOG_MS: u64 = 2000;

/// Closed set of MMIO registers written by the RPS waitboost planner.
pub const RPS_WRITE_SET: [Reg; 2] = [GEN6_RP_CONTROL, GEN6_RPNSWREQ];

/// Reason for a frequency request transition emitted by [`Waitboost::tick`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    /// Active GT workload boosted the software request to RP0 (`intel_rps_boost`, `gt/intel_rps.c:1020`).
    Boost,
    /// Sustained GT idleness decayed the software request back to RPn (`intel_rps_park`, `gt/intel_rps.c:896`).
    Idle,
}

/// Pure state machine for interrupt-less Gen9 RPS waitboost and idle decay.
///
/// Tracks the current software-requested frequency ratio (`rpn` or `rp0`) and
/// schedules `GEN6_RP_CONTROL` / `GEN6_RPNSWREQ` writes without performing MMIO.
/// All time arithmetic saturates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Waitboost {
    layout: RpsLayout,
    caps: RpsCaps,
    requested: Option<u32>,
    last_busy_ms: u64,
    next_log_ms: Option<u64>,
    /// Saturated count of transitions up to RP0.
    pub boosts: u32,
    /// Saturated count of transitions down to RPn.
    pub decays: u32,
}

impl Waitboost {
    /// Construct a planner from `layout` and a raw `GEN6_RP_STATE_CAP` readout.
    ///
    /// Rejects `0xffff_ffff` (unpowered/unmapped BAR readout), `rp0 == 0`,
    /// `rpn == 0`, and inverted capabilities where `rpn > rp0`.
    pub fn new(layout: RpsLayout, rp_state_cap: u32) -> Option<Self> {
        if rp_state_cap == 0xffff_ffff {
            return None;
        }
        let caps = layout.caps(rp_state_cap);
        if caps.rp0 == 0 || caps.rpn == 0 || caps.rpn > caps.rp0 {
            return None;
        }
        Some(Self {
            layout,
            caps,
            requested: None,
            last_busy_ms: 0,
            next_log_ms: None,
            boosts: 0,
            decays: 0,
        })
    }

    /// Decoded RP0/RP1/RPn capability ratios.
    pub fn caps(&self) -> RpsCaps {
        self.caps
    }

    /// Generation-specific RPS register encoding layout.
    pub fn layout(&self) -> RpsLayout {
        self.layout
    }

    /// Currently requested frequency in request units (50/3 MHz on Gen9), or `None` before [`Self::enable_plan`].
    pub fn requested(&self) -> Option<u32> {
        self.requested
    }

    /// Plan the initial RPS enablement writes at timestamp `now`.
    ///
    /// Enables software RPS control in `GEN6_RP_CONTROL` (`rps_reset` at
    /// `gt/intel_rps.c:1221-1236` and `intel_rps_set_manual` at
    /// `gt/intel_rps.c:2650-2657`) and parks the initial software request at
    /// `idle_freq` (`caps.rpn`) via `GEN6_RPNSWREQ` (`intel_rps_park` at
    /// `gt/intel_rps.c:896` and `gen6_rps_set` at `gt/intel_rps.c:802-824`).
    pub fn enable_plan(&mut self, now: u64) -> [Effect; 2] {
        self.requested = Some(self.caps.rpn);
        self.last_busy_ms = now;
        [
            Effect::Write(WritePlan {
                reg: GEN6_RP_CONTROL,
                value: GEN9_RPSWCTL_ENABLE,
            }),
            Effect::Write(WritePlan {
                reg: GEN6_RPNSWREQ,
                value: self.layout.swreq(self.caps.rpn),
            }),
        ]
    }

    /// Advance the boost/idle state machine at monotonic millisecond timestamp `now`.
    ///
    /// In the interrupt-less subset of `gt/intel_rps.c`:
    /// - Before [`Self::enable_plan`], returns `None` without mutating state.
    /// - When `busy` is true, records `last_busy_ms = now` and, if not already at
    ///   `rp0`, boosts to `rp0` (`intel_rps_boost` at `gt/intel_rps.c:1020`,
    ///   threshold-interrupt RP0 selection in `gen6_rps_set_thresholds` /
    ///   `rps_set_power` at `gt/intel_rps.c:680-744`, and `gen6_rps_set` at
    ///   `gt/intel_rps.c:802-824`).
    /// - When `busy` is false, decays from `rp0` to `rpn` (`intel_rps_park` at
    ///   `gt/intel_rps.c:896`) once `now >= last_busy_ms.saturating_add(BOOST_IDLE_MS)`
    ///   provided `rp0 != rpn`.
    pub fn tick(&mut self, now: u64, busy: bool) -> Option<(Effect, Why)> {
        let cur = self.requested?;
        if busy {
            self.last_busy_ms = now;
            if cur != self.caps.rp0 {
                self.requested = Some(self.caps.rp0);
                self.boosts = self.boosts.saturating_add(1);
                return Some((
                    Effect::Write(WritePlan {
                        reg: GEN6_RPNSWREQ,
                        value: self.layout.swreq(self.caps.rp0),
                    }),
                    Why::Boost,
                ));
            }
            None
        } else {
            if cur == self.caps.rp0
                && self.caps.rp0 != self.caps.rpn
                && now >= self.last_busy_ms.saturating_add(BOOST_IDLE_MS)
            {
                self.requested = Some(self.caps.rpn);
                self.decays = self.decays.saturating_add(1);
                return Some((
                    Effect::Write(WritePlan {
                        reg: GEN6_RPNSWREQ,
                        value: self.layout.swreq(self.caps.rpn),
                    }),
                    Why::Idle,
                ));
            }
            None
        }
    }

    /// Next monotonic millisecond timestamp at which an idle decay check is due.
    ///
    /// Returns `Some(last_busy_ms.saturating_add(BOOST_IDLE_MS))` only while
    /// boosted to `rp0` with distinct `rp0` and `rpn` limits.
    pub fn next_deadline(&self) -> Option<u64> {
        if self.requested == Some(self.caps.rp0) && self.caps.rp0 != self.caps.rpn {
            Some(self.last_busy_ms.saturating_add(BOOST_IDLE_MS))
        } else {
            None
        }
    }

    /// Whether a periodic busy frequency sample should be logged at `now`.
    ///
    /// Returns `false` before enablement or while idle; otherwise returns `true`
    /// on the first busy sample or once `now` reaches the scheduled log timestamp,
    /// advancing the schedule to `now.saturating_add(FREQ_LOG_MS)`.
    pub fn sample_due(&mut self, now: u64, busy: bool) -> bool {
        if self.requested.is_none() || !busy {
            return false;
        }
        match self.next_log_ms {
            None => {
                self.next_log_ms = Some(now.saturating_add(FREQ_LOG_MS));
                true
            }
            Some(due) if now >= due => {
                self.next_log_ms = Some(now.saturating_add(FREQ_LOG_MS));
                true
            }
            Some(_) => false,
        }
    }
}