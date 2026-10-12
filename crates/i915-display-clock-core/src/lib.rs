// SPDX-License-Identifier: GPL-2.0-only
//! i915-display-clock-core: Kaby Lake (Gen9.5, display ver 9; SKL-compatible) display power wells, core display clock (CDCLK)
//! and display PLLs (DPLL0..3) as PURE constraint solvers plus bounded, rollback-carrying effect plans.
//!
//! No MMIO, no `unsafe`. A planner takes the software state and a register SNAPSHOT and returns a [`Transaction`]: a forward
//! [`WritePlan`] (register read-modify-writes, writes, posting reads, bounded [`PollPlan`]s, PCU mailbox exchanges) and the
//! rollback plan that undoes it. `devices/i915` is the only crate that executes one, through its `WriteGuard`; it drives every
//! poll with a [`PollBudget`] (sample bound AND time bound) and every PCU exchange with a [`PcodeSession`].
//!
//! Ported from Linux drivers/gpu/drm/i915 at v7.0-rc6: `display/intel_display_power.c`, `intel_display_power_well.c`,
//! `intel_display_power_map.c`, `intel_cdclk.c`, `intel_dpll_mgr.c`, `intel_ddi.c` (`skl_ddi_*_clock`), `intel_audio.c`
//! (`intel_audio_min_cdclk`), `intel_dpll.c` (the `hsw_crtc_compute_clock` dispatch KBL uses) and `intel_pcode.c`. Copyright (c) Intel Corporation and the i915 authors.
//!
//! Modules: [`power_map`] (domain -> well table), [`power`] (refcounted domains + well handshakes), [`cdclk`] (min-cdclk solver,
//! `skl_set_cdclk`), [`wrpll`] (HDMI divider solver), [`dpll`] (DP rates, DPLL enable/disable, DDI clock routing, allocator),
//! [`pcode`] (PCU mailbox retry machine), [`plan`] (the plan vocabulary), [`error`] (named refusals), [`regs`] (new registers).
//!
//! The READ side - `decode_cdclk`, `wrpll_freq_khz`, `PowerState`, the register constants - stays in `i915-kbl-display-core`
//! and is imported, never copied.

#![no_std]
#![forbid(unsafe_code)]

pub mod cdclk;
pub mod dpll;
pub mod error;
pub mod pcode;
pub mod plan;
pub mod power;
pub mod power_map;
pub mod regs;
pub mod wrpll;

pub use error::{pcode_status, Bound, ClockError, Handshake, PcodeErrno};
pub use pcode::{AttemptOutcome, PcodeOp, PcodeRequest, PcodeSession, PcodeWrite, SessionStep};
pub use plan::{OnTimeout, PollBudget, PollPlan, PollVerdict, Step, Transaction, WritePlan};
pub use power::PowerWells;
pub use power_map::Domain;
