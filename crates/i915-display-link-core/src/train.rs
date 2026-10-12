// SPDX-License-Identifier: GPL-2.0-only
//! DP link training (clock recovery, then channel equalisation) for an 8b/10b eDP link, as a state machine with Linux's
//! exact loop limits. Every sink answer and every register effect goes through the caller: [`LinkTrainer::step`] returns the
//! next [`TrainAction`], the executor performs it and hands back a [`TrainReply`].
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dp_link_training.c (v7.0-rc6): `dp_voltage_max` (:285-298),
//! `has_per_lane_signal_levels` (:380-388), `intel_dp_get_lane_adjust_vswing_preemph` (:410-444),
//! `intel_dp_get_adjust_train` (:483-519), `intel_dp_set_link_train` (:529-548), `intel_dp_reset_link_train` (:631-640),
//! `intel_dp_update_link_train` (:642-659), `intel_dp_lane_max_vswing_reached` / `intel_dp_link_max_vswing_reached`
//! (:677-711), `intel_dp_link_training_set_mode` / `_set_bw` (:713-774), `intel_dp_training_pattern` (:776-826),
//! `intel_dp_prepare_link_train` (:840-888), `intel_dp_adjust_request_changed` (:890-914),
//! `intel_dp_link_training_clock_recovery` (:930-1022), `intel_dp_link_training_channel_equalization` (:1024-1092),
//! `intel_dp_disable_dpcd_training_pattern` (:1094-1101), `intel_dp_link_train_all_phys` (:1368-1396) and the training
//! entry `intel_dp_start_link_train` (:1640-1709). Copyright (c) 2008-2020 Intel Corporation.
//!
//! BOUNDS (each enforced in `LinkTrainer` and pinned by a test that fails when the check is removed):
//!  * clock recovery: [`CR_MAX_TRIES_DP14`] = 10 status reads for DPCD >= 1.4 sinks, [`CR_MAX_TRIES_PRE14`] = 80 before
//!    ("4 voltage levels x 4 preemphasis levels x x 5 identical voltage retries", :953-960);
//!  * the same voltage request 5 times in a row ([`CR_SAME_VOLTAGE_TRIES`], :981);
//!  * channel equalisation: [`EQ_MAX_TRIES`] = 5 status reads (:1051).
//! A failure is a NAMED [`TrainFailure`]; nothing is retried silently. Linux's fallback to lower link parameters after
//! `MAX_SEQ_TRAIN_FAILURES` (2, :61) re-runs a whole modeset and is the caller's: the pure reduction is in
//! `caps::reduce_link_params`.
//!
//! NOT PORTED (named): LTTPR handling (`lttpr_count` is 0 on an eDP panel, so `intel_dp_link_train_all_phys` trains only
//! `DP_PHY_DPRX`), 128b/132b training, TPS4 (the source supports it from display ver 10, :1511-1520), per-lane signal levels
//! (`has_per_lane_signal_levels` is false on display ver 9: the request is the maximum across lanes), `intel_hpd_block`, the
//! `link.force_train_failure` debugfs hook, and the delayed link check work queued by `intel_dp_stop_link_train`.
use crate::caps::{compute_rate, dp_tps3_supported, RateSelectError, SinkCaps};
use crate::ddi::{ddi_dp_preemph_max, ddi_dp_voltage_max, DdiError, DdiLink};
use crate::dpcd::{DpcdFail, DpcdOk, DpcdRequest};
use crate::dpdefs::*;
use crate::linkstatus::*;
use crate::plan::Plan;
use alloc::vec;
use alloc::vec::Vec;

/// `max_cr_tries = 10` for a DPCD >= 1.4 sink — intel_dp_link_training.c:962.
pub const CR_MAX_TRIES_DP14: u32 = 10;
/// `max_cr_tries = 80` for older sinks — intel_dp_link_training.c:964.
pub const CR_MAX_TRIES_PRE14: u32 = 80;
/// `if (voltage_tries == 5)` — intel_dp_link_training.c:981.
pub const CR_SAME_VOLTAGE_TRIES: u32 = 5;
/// `for (tries = 0; tries < 5; tries++)` — intel_dp_link_training.c:1051.
pub const EQ_MAX_TRIES: u32 = 5;
/// `#define MAX_SEQ_TRAIN_FAILURES 2` — intel_dp_link_training.c:61 (the caller's retry budget).
pub const MAX_SEQ_TRAIN_FAILURES: u32 = 2;

/// Why training failed. Names are stable marker tokens (`name()`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainFailure {
    /// The eDP 1.4 rate select could not find the port clock in the sink's table (`intel_dp_rate_select`, :1587).
    RateNotInSinkTable { port_clock: u32 },
    /// A DDI planner refused (signal level not in the 10-row index, unknown pattern).
    Ddi(DdiError),
    /// "Failed to enable link training" — writing TPS1 + the zeroed lane settings failed (:944-948).
    ClockRecoveryStartFailed,
    /// "Failed to get link status" during clock recovery (:974-977).
    ClockRecoveryStatusReadFailed,
    /// "Same voltage tried 5 times" (:981-986).
    ClockRecoverySameVoltage5Times,
    /// "Max Voltage Swing reached" (:988-993).
    ClockRecoveryMaxSwingReached,
    /// "Failed to update link training" during clock recovery (:996-1001).
    ClockRecoveryUpdateFailed,
    /// "Failed clock recovery %d times, giving up!" (:1016-1019): `max_tries` status reads without CR done.
    ClockRecoveryExhausted { max_tries: u32 },
    /// "Failed to start channel equalization" (:1041-1047).
    ChannelEqStartFailed,
    /// "Failed to get link status" during equalisation (:1053-1059).
    ChannelEqStatusReadFailed,
    /// "Clock recovery check failed, cannot continue channel equalization" (:1062-1070).
    ChannelEqClockRecoveryLost,
    /// "Failed to update link training" during equalisation (:1084-1089).
    ChannelEqUpdateFailed,
    /// "Channel equalization failed 5 times" (:1105-1109).
    ChannelEqExhausted,
}

impl TrainFailure {
    pub const fn name(&self) -> &'static str {
        match self {
            TrainFailure::RateNotInSinkTable { .. } => "LT-RATE-NOT-IN-SINK-TABLE",
            TrainFailure::Ddi(_) => "LT-DDI-REFUSED",
            TrainFailure::ClockRecoveryStartFailed => "LT-CR-START-FAILED",
            TrainFailure::ClockRecoveryStatusReadFailed => "LT-CR-STATUS-READ-FAILED",
            TrainFailure::ClockRecoverySameVoltage5Times => "LT-CR-SAME-VOLTAGE-5-TIMES",
            TrainFailure::ClockRecoveryMaxSwingReached => "LT-CR-MAX-SWING-REACHED",
            TrainFailure::ClockRecoveryUpdateFailed => "LT-CR-UPDATE-FAILED",
            TrainFailure::ClockRecoveryExhausted { .. } => "LT-CR-EXHAUSTED",
            TrainFailure::ChannelEqStartFailed => "LT-EQ-START-FAILED",
            TrainFailure::ChannelEqStatusReadFailed => "LT-EQ-STATUS-READ-FAILED",
            TrainFailure::ChannelEqClockRecoveryLost => "LT-EQ-CR-LOST",
            TrainFailure::ChannelEqUpdateFailed => "LT-EQ-UPDATE-FAILED",
            TrainFailure::ChannelEqExhausted => "LT-EQ-EXHAUSTED",
        }
    }
}

/// A trained link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrainSuccess {
    /// Link status reads made in clock recovery / equalisation.
    pub cr_reads: u32,
    pub eq_reads: u32,
    /// The final `DP_TRAINING_LANEx_SET` values.
    pub train_set: [u8; 4],
    /// The pattern equalisation used (`DP_TRAINING_PATTERN_2` or `_3`).
    pub eq_pattern: u8,
}

pub type TrainOutcome = Result<TrainSuccess, TrainFailure>;

/// The link being trained.
#[derive(Clone, Copy, Debug)]
pub struct LinkParams {
    /// `crtc_state->port_clock`.
    pub port_clock: u32,
    /// `crtc_state->lane_count` (1, 2 or 4).
    pub lane_count: u8,
    /// `crtc_state->enhanced_framing`.
    pub enhanced_framing: bool,
    /// `crtc_state->vrr.in_range` -> `DP_MSA_TIMING_PAR_IGNORE_EN` (:717).
    pub msa_timing_par_ignore: bool,
}

/// One thing the executor does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TrainAction {
    /// A DPCD access (`DpcdXfer`); answer `TrainReply::Dpcd`.
    Dpcd(DpcdRequest),
    /// Register effects (apply in order); answer `TrainReply::Done`.
    Mmio(Plan),
    /// `fsleep(us)`; answer `TrainReply::Done`.
    DelayUs(u32),
    /// Training is over (also after a failure — the DPCD pattern is already disabled and the DDI idled).
    Finished(TrainOutcome),
}

#[derive(Clone, Copy, Debug)]
pub enum TrainReply {
    Start,
    Done,
    Dpcd(Result<DpcdOk, DpcdFail>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum After {
    Begin,
    PrepDone,
    CrResetDone,
    CrStatus,
    CrUpdateDone,
    EqStartDone,
    EqStatus,
    EqUpdateDone,
    Finalized,
    Over,
}

/// The state machine.
#[derive(Clone, Debug)]
pub struct LinkTrainer {
    link: LinkParams,
    caps: SinkCaps,
    ddi: DdiLink,
    queue: Vec<TrainAction>,
    qi: usize,
    after: After,
    train_set: [u8; 4],
    // clock recovery
    max_cr_tries: u32,
    cr_tries: u32,
    voltage_tries: u32,
    max_vswing_reached: bool,
    old_link_status: LinkStatus,
    link_status: LinkStatus,
    cr_reads: u32,
    // channel equalisation
    eq_tries: u32,
    eq_reads: u32,
    eq_pattern: u8,
    outcome: Option<TrainOutcome>,
}

/// `dp_voltage_max()` — intel_dp_link_training.c:285-298: the highest voltage swing allowed with pre-emphasis `preemph`.
pub const fn dp_voltage_max(preemph: u8) -> u8 {
    match preemph & DP_TRAIN_PRE_EMPHASIS_MASK {
        DP_TRAIN_PRE_EMPH_LEVEL_0 => DP_TRAIN_VOLTAGE_SWING_LEVEL_3,
        DP_TRAIN_PRE_EMPH_LEVEL_1 => DP_TRAIN_VOLTAGE_SWING_LEVEL_2,
        DP_TRAIN_PRE_EMPH_LEVEL_2 => DP_TRAIN_VOLTAGE_SWING_LEVEL_1,
        _ => DP_TRAIN_VOLTAGE_SWING_LEVEL_0,
    }
}

/// `intel_dp_get_lane_adjust_vswing_preemph()` with `has_per_lane_signal_levels() == false` (display ver 9, no LTTPR) —
/// intel_dp_link_training.c:410-444: the sink's per-lane requests collapse to the maximum across the active lanes.
pub fn lane_adjust_vswing_preemph(ls: &LinkStatus, lane_count: u32, voltage_max: u8, preemph_max: u8) -> u8 {
    let (mut v, mut p) = (0u8, 0u8);
    for lane in 0..lane_count {
        v = v.max(adjust_request_voltage(ls, lane));
        p = p.max(adjust_request_pre_emphasis(ls, lane));
    }
    if p >= preemph_max {
        p = preemph_max | DP_TRAIN_MAX_PRE_EMPHASIS_REACHED;
    }
    v = v.min(dp_voltage_max(p));
    if v >= voltage_max {
        v = voltage_max | DP_TRAIN_MAX_SWING_REACHED;
    }
    v | p
}

/// `intel_dp_get_adjust_train()` — intel_dp_link_training.c:483-519: updates all FOUR `train_set` entries, returns whether any
/// changed.
pub fn get_adjust_train(train_set: &mut [u8; 4], ls: &LinkStatus, lane_count: u32, voltage_max: u8, preemph_max: u8) -> bool {
    let mut changed = false;
    for slot in train_set.iter_mut() {
        let new = lane_adjust_vswing_preemph(ls, lane_count, voltage_max, preemph_max);
        if *slot != new {
            *slot = new;
            changed = true;
        }
    }
    changed
}

/// `intel_dp_lane_max_vswing_reached()` — intel_dp_link_training.c:677-691.
pub const fn lane_max_vswing_reached(train_set_lane: u8) -> bool {
    let v = (train_set_lane & DP_TRAIN_VOLTAGE_SWING_MASK) >> DP_TRAIN_VOLTAGE_SWING_SHIFT;
    let p = (train_set_lane & DP_TRAIN_PRE_EMPHASIS_MASK) >> DP_TRAIN_PRE_EMPHASIS_SHIFT;
    if train_set_lane & DP_TRAIN_MAX_SWING_REACHED == 0 {
        return false;
    }
    v + p == 3
}

/// `intel_dp_link_max_vswing_reached()` (8b/10b arm) — intel_dp_link_training.c:693-711.
pub fn link_max_vswing_reached(train_set: &[u8; 4], lane_count: u32) -> bool {
    (0..lane_count as usize).all(|l| lane_max_vswing_reached(train_set[l]))
}

/// `intel_dp_adjust_request_changed()` (8b/10b arm) — intel_dp_link_training.c:890-914.
pub fn adjust_request_changed(old: &LinkStatus, new: &LinkStatus, lane_count: u32) -> bool {
    (0..lane_count).any(|lane| {
        let o = adjust_request_voltage(old, lane) | adjust_request_pre_emphasis(old, lane);
        let n = adjust_request_voltage(new, lane) | adjust_request_pre_emphasis(new, lane);
        o != n
    })
}

/// `intel_dp_training_pattern()` for a display-ver-9 source and the DPRX — intel_dp_link_training.c:776-826: TPS3 when the
/// sink supports it (the source does from ver 9), else TPS2. (TPS4 needs a ver >= 10 source, :1517-1520.)
pub fn training_pattern(caps: &SinkCaps) -> u8 {
    if dp_tps3_supported(&caps.dpcd) {
        DP_TRAINING_PATTERN_3
    } else {
        DP_TRAINING_PATTERN_2
    }
}

fn dpcd_write_ok(reply: &TrainReply, want_len: usize) -> bool {
    matches!(reply, TrainReply::Dpcd(Ok(ok)) if ok.len == want_len)
}

impl LinkTrainer {
    /// Set up training for `link` against `caps`, driving `ddi`. `Err` if the port clock is not selectable.
    pub fn new(link: LinkParams, caps: SinkCaps, ddi: DdiLink) -> Result<LinkTrainer, TrainFailure> {
        // intel_dp_compute_rate() runs in prepare_link_train (:849); refuse here so nothing is sent for a bad rate.
        compute_rate(&caps, link.port_clock).map_err(|e| match e {
            RateSelectError::RateNotInSinkTable { port_clock } => TrainFailure::RateNotInSinkTable { port_clock },
        })?;
        let max_cr_tries = if caps.dpcd[DP_DPCD_REV as usize] >= DP_DPCD_REV_14 { CR_MAX_TRIES_DP14 } else { CR_MAX_TRIES_PRE14 };
        Ok(LinkTrainer {
            link,
            caps,
            ddi,
            queue: Vec::new(),
            qi: 0,
            after: After::Begin,
            train_set: [0; 4],
            max_cr_tries,
            cr_tries: 0,
            voltage_tries: 1,
            max_vswing_reached: false,
            old_link_status: [0; DP_LINK_STATUS_SIZE],
            link_status: [0; DP_LINK_STATUS_SIZE],
            cr_reads: 0,
            eq_tries: 0,
            eq_reads: 0,
            eq_pattern: DP_TRAINING_PATTERN_2,
            outcome: None,
        })
    }

    /// The DDI state (`intel_dp->DP` etc.) as training left it.
    pub fn ddi(&self) -> &DdiLink {
        &self.ddi
    }

    /// The clock-recovery bound in force (10 or 80).
    pub fn max_cr_tries(&self) -> u32 {
        self.max_cr_tries
    }

    fn lanes(&self) -> u32 {
        self.link.lane_count as u32
    }

    fn issue(&mut self, batch: Vec<TrainAction>, after: After) -> TrainAction {
        self.queue = batch;
        self.qi = 1;
        self.after = after;
        self.queue[0].clone()
    }

    /// Advance: pass `TrainReply::Start` first, then the executor's answer to the previous action.
    pub fn step(&mut self, reply: TrainReply) -> TrainAction {
        if self.qi < self.queue.len() {
            // More of the batch to issue; the reply to a non-final action is ignored (Linux ignores MMIO / delay results).
            let a = self.queue[self.qi].clone();
            self.qi += 1;
            return a;
        }
        self.dispatch(reply)
    }

    /// `intel_dp_prepare_link_train()` — intel_dp_link_training.c:840-888: DDI enable with pattern 1, WaEdpLinkRateDataReload,
    /// downspread / coding, then link bw or rate select + lane count. DPCD write results are not checked in Linux.
    fn prepare_actions(&mut self) -> Vec<TrainAction> {
        let mut v = Vec::new();
        v.push(TrainAction::Mmio(self.ddi.prepare_link_retrain()));
        // Cannot fail: validated in new().
        let sel = compute_rate(&self.caps, self.link.port_clock).unwrap_or(crate::caps::RateSel { link_bw: 0, rate_select: 0 });
        if sel.link_bw == 0 {
            // "Reloading eDP link rates" (:855-868)
            v.push(TrainAction::Dpcd(DpcdRequest::read(DP_SUPPORTED_LINK_RATES, 16)));
        }
        // intel_dp_link_training_set_mode(): DP_DOWNSPREAD_CTRL <- { MSA ignore, 8b/10b } (:713-721)
        let cfg0 = if self.link.msa_timing_par_ignore { DP_MSA_TIMING_PAR_IGNORE_EN } else { 0 };
        v.push(TrainAction::Dpcd(DpcdRequest::write(DP_DOWNSPREAD_CTRL, &[cfg0, DP_SET_ANSI_8B10B])));
        // intel_dp_link_training_set_bw() — :743-774
        let mut lane_count = self.link.lane_count;
        if self.link.enhanced_framing {
            lane_count |= DP_LANE_COUNT_ENHANCED_FRAME_EN;
        }
        if sel.link_bw != 0 {
            v.push(TrainAction::Dpcd(DpcdRequest::write(DP_LINK_BW_SET, &[sel.link_bw, lane_count])));
        } else {
            v.push(TrainAction::Dpcd(DpcdRequest::write(DP_LANE_COUNT_SET, &[lane_count])));
            v.push(TrainAction::Dpcd(DpcdRequest::write(DP_LINK_RATE_SET, &[sel.rate_select])));
        }
        v
    }

    /// `intel_dp_set_signal_levels()` for the DPRX: the DDI half (:605-629).
    fn signal_levels_plan(&mut self) -> Result<Plan, TrainFailure> {
        self.ddi.set_signal_levels(&self.train_set).map_err(TrainFailure::Ddi)
    }

    /// `intel_dp_set_link_train()` (:529-548): DDI pattern select, then the DPCD write `{ pattern, train_set[..lanes] }`.
    fn set_link_train_actions(&mut self, pat: u8) -> Result<Vec<TrainAction>, TrainFailure> {
        let plan = self.ddi.set_link_train(pat).map_err(TrainFailure::Ddi)?;
        let mut buf = [0u8; 5];
        buf[0] = pat;
        let n = self.link.lane_count as usize;
        buf[1..1 + n].copy_from_slice(&self.train_set[..n]);
        Ok(vec![TrainAction::Mmio(plan), TrainAction::Dpcd(DpcdRequest::write(DP_TRAINING_PATTERN_SET, &buf[..n + 1]))])
    }

    /// `intel_dp_update_link_train()` (:642-659): signal levels, then `DP_TRAINING_LANE0_SET` <- `train_set[..lanes]`.
    fn update_link_train_actions(&mut self) -> Result<Vec<TrainAction>, TrainFailure> {
        let plan = self.signal_levels_plan()?;
        let n = self.link.lane_count as usize;
        Ok(vec![TrainAction::Mmio(plan), TrainAction::Dpcd(DpcdRequest::write(DP_TRAINING_LANE0_SET, &self.train_set[..n]))])
    }

    /// Everything after a failure or success: disable the DPCD pattern, idle the DDI, report.
    fn finalize(&mut self, outcome: TrainOutcome) -> TrainAction {
        self.outcome = Some(outcome);
        let batch = vec![
            TrainAction::Dpcd(DpcdRequest::write(DP_TRAINING_PATTERN_SET, &[DP_TRAINING_PATTERN_DISABLE])),
            TrainAction::Mmio(self.ddi.set_idle_link_train()),
        ];
        self.issue(batch, After::Finalized)
    }

    fn fail(&mut self, f: TrainFailure) -> TrainAction {
        self.finalize(Err(f))
    }

    fn cr_poll_batch(&self) -> Vec<TrainAction> {
        vec![
            TrainAction::DelayUs(clock_recovery_delay_us(&self.caps.dpcd)),
            TrainAction::Dpcd(DpcdRequest::read(DP_LANE0_1_STATUS, DP_LINK_STATUS_SIZE)),
        ]
    }

    fn eq_poll_batch(&self) -> Vec<TrainAction> {
        vec![
            TrainAction::DelayUs(channel_eq_delay_us(&self.caps.dpcd)),
            TrainAction::Dpcd(DpcdRequest::read(DP_LANE0_1_STATUS, DP_LINK_STATUS_SIZE)),
        ]
    }

    fn status_of(reply: &TrainReply) -> Option<LinkStatus> {
        match reply {
            TrainReply::Dpcd(Ok(ok)) if ok.len == DP_LINK_STATUS_SIZE => {
                let mut ls = [0u8; DP_LINK_STATUS_SIZE];
                ls.copy_from_slice(&ok.data[..DP_LINK_STATUS_SIZE]);
                Some(ls)
            }
            _ => None,
        }
    }

    fn dispatch(&mut self, reply: TrainReply) -> TrainAction {
        let lanes = self.lanes();
        match self.after {
            After::Begin => {
                let batch = self.prepare_actions();
                self.issue(batch, After::PrepDone)
            }
            After::PrepDone => {
                // intel_dp_link_training_clock_recovery(): reset_link_train(TPS1 | scrambling disabled), :940-948.
                self.train_set = [0; 4];
                let pat = DP_TRAINING_PATTERN_1 | DP_LINK_SCRAMBLING_DISABLE;
                let mut batch = Vec::new();
                match self.signal_levels_plan() {
                    Ok(p) => batch.push(TrainAction::Mmio(p)),
                    Err(f) => return self.fail(f),
                }
                match self.set_link_train_actions(pat) {
                    Ok(a) => batch.extend(a),
                    Err(f) => return self.fail(f),
                }
                self.issue(batch, After::CrResetDone)
            }
            After::CrResetDone => {
                if !dpcd_write_ok(&reply, self.link.lane_count as usize + 1) {
                    return self.fail(TrainFailure::ClockRecoveryStartFailed);
                }
                self.cr_tries = 0;
                self.voltage_tries = 1;
                self.max_vswing_reached = false;
                self.old_link_status = [0; DP_LINK_STATUS_SIZE];
                let b = self.cr_poll_batch();
                self.issue(b, After::CrStatus)
            }
            After::CrStatus => {
                let Some(ls) = Self::status_of(&reply) else {
                    return self.fail(TrainFailure::ClockRecoveryStatusReadFailed);
                };
                self.cr_reads += 1;
                self.link_status = ls;
                if clock_recovery_ok(&ls, lanes) {
                    return self.begin_eq();
                }
                if self.voltage_tries == CR_SAME_VOLTAGE_TRIES {
                    return self.fail(TrainFailure::ClockRecoverySameVoltage5Times);
                }
                if self.max_vswing_reached {
                    return self.fail(TrainFailure::ClockRecoveryMaxSwingReached);
                }
                let vmax = ddi_dp_voltage_max(self.ddi.cfg.trans.n_entries);
                get_adjust_train(&mut self.train_set, &ls, lanes, vmax, ddi_dp_preemph_max());
                match self.update_link_train_actions() {
                    Ok(b) => self.issue(b, After::CrUpdateDone),
                    Err(f) => self.fail(f),
                }
            }
            After::CrUpdateDone => {
                if !dpcd_write_ok(&reply, self.link.lane_count as usize) {
                    return self.fail(TrainFailure::ClockRecoveryUpdateFailed);
                }
                let ls = self.link_status;
                if !adjust_request_changed(&self.old_link_status, &ls, lanes) {
                    self.voltage_tries += 1;
                } else {
                    self.voltage_tries = 1;
                }
                self.old_link_status = ls;
                if link_max_vswing_reached(&self.train_set, lanes) {
                    self.max_vswing_reached = true;
                }
                self.cr_tries += 1;
                if self.cr_tries >= self.max_cr_tries {
                    let max_tries = self.max_cr_tries;
                    return self.fail(TrainFailure::ClockRecoveryExhausted { max_tries });
                }
                let b = self.cr_poll_batch();
                self.issue(b, After::CrStatus)
            }
            After::EqStartDone => {
                if !dpcd_write_ok(&reply, self.link.lane_count as usize + 1) {
                    return self.fail(TrainFailure::ChannelEqStartFailed);
                }
                self.eq_tries = 0;
                let b = self.eq_poll_batch();
                self.issue(b, After::EqStatus)
            }
            After::EqStatus => {
                let Some(ls) = Self::status_of(&reply) else {
                    return self.fail(TrainFailure::ChannelEqStatusReadFailed);
                };
                self.eq_reads += 1;
                // "Make sure clock is still ok"
                if !clock_recovery_ok(&ls, lanes) {
                    return self.fail(TrainFailure::ChannelEqClockRecoveryLost);
                }
                if channel_eq_ok(&ls, lanes) {
                    let ok = TrainSuccess { cr_reads: self.cr_reads, eq_reads: self.eq_reads, train_set: self.train_set, eq_pattern: self.eq_pattern };
                    return self.finalize(Ok(ok));
                }
                let vmax = ddi_dp_voltage_max(self.ddi.cfg.trans.n_entries);
                get_adjust_train(&mut self.train_set, &ls, lanes, vmax, ddi_dp_preemph_max());
                match self.update_link_train_actions() {
                    Ok(b) => self.issue(b, After::EqUpdateDone),
                    Err(f) => self.fail(f),
                }
            }
            After::EqUpdateDone => {
                if !dpcd_write_ok(&reply, self.link.lane_count as usize) {
                    return self.fail(TrainFailure::ChannelEqUpdateFailed);
                }
                self.eq_tries += 1;
                if self.eq_tries >= EQ_MAX_TRIES {
                    return self.fail(TrainFailure::ChannelEqExhausted);
                }
                let b = self.eq_poll_batch();
                self.issue(b, After::EqStatus)
            }
            After::Finalized => {
                self.after = After::Over;
                TrainAction::Finished(self.outcome.unwrap_or(Err(TrainFailure::ChannelEqExhausted)))
            }
            After::Over => TrainAction::Finished(self.outcome.unwrap_or(Err(TrainFailure::ChannelEqExhausted))),
            // `CrResetDone`/`CrStatus` arms above cover the clock-recovery states; `EqStartDone` is entered from `begin_eq`.
        }
    }

    /// Channel equalisation start: `intel_dp_set_link_train(training_pattern)` (:1029-1047).
    fn begin_eq(&mut self) -> TrainAction {
        let mut pat = training_pattern(&self.caps);
        self.eq_pattern = pat;
        // "Scrambling is disabled for TPS2/3 and enabled for TPS4" — :1037-1039
        if pat != DP_TRAINING_PATTERN_4 {
            pat |= DP_LINK_SCRAMBLING_DISABLE;
        }
        match self.set_link_train_actions(pat) {
            Ok(b) => self.issue(b, After::EqStartDone),
            Err(f) => self.fail(f),
        }
    }
}
