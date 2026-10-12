// SPDX-License-Identifier: GPL-2.0-only
//! The vocabulary of an ordered transaction: [`Stage`]s whose forward and rollback bodies are either a plan in `display-clock-core`'s
//! vocabulary, a plan in `display-link-core`'s, a named INTERACTIVE state machine the executor drives with feedback (PPS sequencing,
//! link training - they need AUX replies between writes), or an explicit empty body that says why Linux does nothing there.
//!
//! Nothing here touches a register. `devices/i915` is the only crate that executes a [`Body`], writing through a `WriteGuard`.
use alloc::vec::Vec;
use i915_common_core::Reg;
use i915_display_clock_core::{Step, WritePlan};
use i915_display_link_core::plan::{Effect, PollPlan};

/// A plan in `display-link-core`'s vocabulary (`Write`, `Rmw`, `ClearIfSet`, `PostingRead`, bounded `Poll`, `DelayUs`).
pub type Plan = Vec<Effect>;

/// A state machine of `display-link-core` the executor runs with feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Interactive {
    /// `pps::Kind::PanelOn` (`intel_pps_on`).
    PpsPanelOn,
    /// `pps::Kind::PanelOff` (`intel_pps_off`).
    PpsPanelOff,
    /// `pps::Kind::BacklightOn` (`intel_pps_backlight_on`).
    PpsBacklightOn,
    /// `pps::Kind::BacklightOff`.
    PpsBacklightOff,
    /// `sinkpower::SetPower` D0 (`intel_dp_set_power(DP_SET_POWER_D0)`).
    SinkPowerD0,
    /// `sinkpower::SetPower` D3.
    SinkPowerD3,
    /// `train::LinkTrainer` (`intel_dp_start_link_train`, which begins with `prepare_link_retrain`).
    LinkTraining,
}

impl Interactive {
    pub const fn name(self) -> &'static str {
        match self {
            Interactive::PpsPanelOn => "PPS-PANEL-ON",
            Interactive::PpsPanelOff => "PPS-PANEL-OFF",
            Interactive::PpsBacklightOn => "PPS-BACKLIGHT-ON",
            Interactive::PpsBacklightOff => "PPS-BACKLIGHT-OFF",
            Interactive::SinkPowerD0 => "SINK-POWER-D0",
            Interactive::SinkPowerD3 => "SINK-POWER-D3",
            Interactive::LinkTraining => "LINK-TRAINING",
        }
    }
}

/// What a stage (or its rollback) does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Linux does nothing here for this configuration (or it is a named non-port); the string says why.
    Empty(&'static str),
    Clock(WritePlan),
    Link(Plan),
    Interactive(Interactive),
}

impl Body {
    /// The registers this body WRITES, in order (`Write`, `Rmw`, `ClearIfSet`; clock `Step::Write` / `Rmw`). Interactive
    /// bodies write through their own plans and are not listed.
    pub fn written_regs(&self) -> Vec<Reg> {
        let mut v = Vec::new();
        match self {
            Body::Empty(_) | Body::Interactive(_) => {}
            Body::Clock(p) => {
                for s in p.iter() {
                    match s {
                        Step::Write { reg, .. } | Step::Rmw { reg, .. } => v.push(*reg),
                        _ => {}
                    }
                }
            }
            Body::Link(p) => v.extend(i915_display_link_core::plan::written_regs(p)),
        }
        v
    }
    /// Every bounded wait in the body, as `(sample bound, what)` - the executor is held to these.
    pub fn polls(&self) -> Vec<(u32, &'static str)> {
        let mut v = Vec::new();
        match self {
            Body::Empty(_) | Body::Interactive(_) => {}
            Body::Clock(p) => {
                for s in p.iter() {
                    if let Step::Poll(pp) = s {
                        v.push((pp.max_polls, pp.what.name()));
                    }
                }
            }
            Body::Link(p) => {
                for e in p {
                    if let Effect::Poll(pp) = e {
                        v.push((pp.max_polls, pp.what));
                    }
                }
            }
        }
        v
    }
}

/// One sample of a register after the stage, checked against `(value & mask) == want`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Readback {
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
}

/// One step of an ordered transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stage {
    pub id: StageId,
    /// The Linux function this stage is, e.g. `"intel_set_pipe_src_size"`.
    pub linux: &'static str,
    pub forward: Body,
    /// Undoes `forward` and is safe to run whether or not `forward` finished.
    pub rollback: Body,
    /// Read-after-write checks of what `forward` wrote (empty for composed sub-plans, which carry their own posting reads and polls).
    pub readback: Vec<Readback>,
    /// A bounded wait for the hardware to settle after `forward` (TRANSCONF state, DBUF power, PLANE_SURFLIVE).
    pub settle: Option<PollPlan>,
}

/// The stages of the eDP enable transaction, in the order `hsw_crtc_enable()` (and the hooks it calls) performs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StageId {
    PowerDomains,
    CdclkSet,
    DbufSlice,
    AuxPower,
    DpllEnable,
    PpsPanelOn,
    DdiClockEnable,
    DdiIoPower,
    DdiBufTrans,
    SignalLevels,
    SinkPowerD0,
    LinkTrain,
    StopLinkTrain,
    TranscoderClock,
    PipeSrc,
    PipeMisc,
    LinkMn,
    TranscoderTimings,
    FrameStartDelay,
    Transconf,
    PanelFit,
    ColorModeset,
    LinetimeWm,
    DdiFuncCtl,
    TranscoderEnable,
    BacklightPwm,
    PpsBacklightOn,
    PlaneNoarm,
    PlaneArm,
    CursorWm,
}

impl StageId {
    pub const fn name(self) -> &'static str {
        match self {
            StageId::PowerDomains => "POWER-DOMAINS",
            StageId::CdclkSet => "CDCLK-SET",
            StageId::DbufSlice => "DBUF-SLICE",
            StageId::AuxPower => "AUX-POWER",
            StageId::DpllEnable => "DPLL-ENABLE",
            StageId::PpsPanelOn => "PPS-PANEL-ON",
            StageId::DdiClockEnable => "DDI-CLOCK-ENABLE",
            StageId::DdiIoPower => "DDI-IO-POWER",
            StageId::DdiBufTrans => "DDI-BUF-TRANS",
            StageId::SignalLevels => "SIGNAL-LEVELS",
            StageId::SinkPowerD0 => "SINK-POWER-D0",
            StageId::LinkTrain => "LINK-TRAIN",
            StageId::StopLinkTrain => "STOP-LINK-TRAIN",
            StageId::TranscoderClock => "TRANSCODER-CLOCK",
            StageId::PipeSrc => "PIPE-SRC",
            StageId::PipeMisc => "PIPE-MISC",
            StageId::LinkMn => "LINK-MN",
            StageId::TranscoderTimings => "TRANSCODER-TIMINGS",
            StageId::FrameStartDelay => "FRAME-START-DELAY",
            StageId::Transconf => "TRANSCONF",
            StageId::PanelFit => "PANEL-FIT",
            StageId::ColorModeset => "COLOR-MODESET",
            StageId::LinetimeWm => "LINETIME-WM",
            StageId::DdiFuncCtl => "DDI-FUNC-CTL",
            StageId::TranscoderEnable => "TRANSCODER-ENABLE",
            StageId::BacklightPwm => "BACKLIGHT-PWM",
            StageId::PpsBacklightOn => "PPS-BACKLIGHT-ON",
            StageId::PlaneNoarm => "PLANE-NOARM",
            StageId::PlaneArm => "PLANE-ARM",
            StageId::CursorWm => "CURSOR-WM",
        }
    }
}

/// Restore plan: for every register `plan` writes, a `Write` of its pre-transaction value `snap(reg)`, in REVERSE order of first
/// write, each register once. The snapshot is what the executor read before the transaction began, so the rollback restores what
/// was there, not what a planner guesses was there.
pub fn restore_plan(plan: &Plan, snap: &dyn Fn(u32) -> u32) -> Plan {
    let mut seen: Vec<Reg> = Vec::new();
    for r in i915_display_link_core::plan::written_regs(plan) {
        if !seen.contains(&r) {
            seen.push(r);
        }
    }
    seen.iter().rev().map(|r| Effect::Write { reg: *r, value: snap(r.offset()) }).collect()
}

/// Readbacks for a plan's plain writes: each `Write` register must read back with at least the bits written as 1 set (`mask =
/// value`), or the whole word when `full` says the register is a fully-defined pair/field word.
pub fn readbacks_of(plan: &Plan, full: bool) -> Vec<Readback> {
    let mut out: Vec<Readback> = Vec::new();
    for e in plan {
        if let Effect::Write { reg, value } = e {
            let mask = if full { u32::MAX } else { *value };
            if mask != 0 {
                // the LAST write to a register is what must read back
                out.retain(|r| r.reg != *reg);
                out.push(Readback { reg: *reg, mask, want: *value & mask });
            }
        }
    }
    out
}
