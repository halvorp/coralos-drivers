// SPDX-License-Identifier: GPL-2.0-only
//! DDI port sequencing for a DP / eDP output on a display-version-9 part, as plans: the `DDI_BUF_CTL` value Linux keeps
//! in `intel_dp->DP`, buffer-translation programming, signal levels (+ SKL I_boost), the training-pattern select in
//! `DP_TP_CTL`, the idle pattern, and buffer disable.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_ddi.c (v7.0-rc6): `index_to_dp_signal_levels` (:94-105),
//! `has_buf_trans_select` / `has_iboost` (:119-127), `hsw_prepare_dp_ddi_buffers` (:134-157), `intel_wait_ddi_buf_idle`
//! (:199-220), `intel_wait_ddi_buf_active` (:221-244, the display-ver < 10 arm), `intel_ddi_init_dp_buf_reg` (:353-377),
//! `_skl_ddi_set_iboost` / `skl_ddi_set_iboost` (:1078-1126), `intel_ddi_dp_voltage_max` / `intel_ddi_dp_preemph_max`
//! (:1128-1156), `translate_signal_level` (:1467-1487), `intel_ddi_dp_level` / `intel_ddi_level` (:1491-1529),
//! `hsw_set_signal_levels` (:1531-1560), `intel_ddi_buf_enable` / `intel_ddi_buf_disable` (:3088-3123),
//! `intel_ddi_prepare_link_retrain` (:3785-3815), `intel_ddi_set_link_train` (:3817-3846), `intel_ddi_set_idle_link_train`
//! (:3848-3874). Copyright (c) 2012 Intel Corporation.
//!
//! WHAT THIS DOES NOT SEQUENCE: the DDI clock select (`intel_ddi_enable_clock`, owned by the DPLL / clock crate), the DDI IO
//! power domain (display-power), the transcoder clock/function (`i915-kbl-display-core` write planners), HDMI, MST, FEC,
//! `intel_ddi_power_up_lanes` (ICL+), and the `drm_WARN_ON(dp_tp_ctl & DP_TP_CTL_ENABLE)` read in `prepare_link_retrain`
//! (a diagnostic; the planner has no read to make). The orchestrator calls these primitives in Linux's `hsw_ddi_pre_enable_dp`
//! order (intel_ddi.c:2909-2954).
use crate::buf_trans::BufTrans;
use crate::dpdefs::*;
use crate::plan::{Effect, Plan, PollPlan};
use crate::regs::*;
use alloc::vec;
use i915_kbl_display_core::regs::{ddi_buf_ctl, DDI_A_4_LANES, DDI_BUF_CTL_ENABLE, DDI_BUF_EMP_MASK, DDI_BUF_IS_IDLE, DDI_BUF_PORT_REVERSAL};
use i915_kbl_display_core::Ddi;

/// `index_to_dp_signal_levels[]` — intel_ddi.c:94-105: the 10 (vswing, pre-emphasis) pairs a DDI buffer-translation index
/// stands for.
pub const INDEX_TO_DP_SIGNAL_LEVELS: [u8; 10] = [
    DP_TRAIN_VOLTAGE_SWING_LEVEL_0 | DP_TRAIN_PRE_EMPH_LEVEL_0, // intel_ddi.c:95
    DP_TRAIN_VOLTAGE_SWING_LEVEL_0 | DP_TRAIN_PRE_EMPH_LEVEL_1, // intel_ddi.c:96
    DP_TRAIN_VOLTAGE_SWING_LEVEL_0 | DP_TRAIN_PRE_EMPH_LEVEL_2, // intel_ddi.c:97
    DP_TRAIN_VOLTAGE_SWING_LEVEL_0 | DP_TRAIN_PRE_EMPH_LEVEL_3, // intel_ddi.c:98
    DP_TRAIN_VOLTAGE_SWING_LEVEL_1 | DP_TRAIN_PRE_EMPH_LEVEL_0, // intel_ddi.c:99
    DP_TRAIN_VOLTAGE_SWING_LEVEL_1 | DP_TRAIN_PRE_EMPH_LEVEL_1, // intel_ddi.c:100
    DP_TRAIN_VOLTAGE_SWING_LEVEL_1 | DP_TRAIN_PRE_EMPH_LEVEL_2, // intel_ddi.c:101
    DP_TRAIN_VOLTAGE_SWING_LEVEL_2 | DP_TRAIN_PRE_EMPH_LEVEL_0, // intel_ddi.c:102
    DP_TRAIN_VOLTAGE_SWING_LEVEL_2 | DP_TRAIN_PRE_EMPH_LEVEL_1, // intel_ddi.c:103
    DP_TRAIN_VOLTAGE_SWING_LEVEL_3 | DP_TRAIN_PRE_EMPH_LEVEL_0, // intel_ddi.c:104
];

/// A DDI planner refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DdiError {
    /// `translate_signal_level()` found no row (the `drm_WARN` at intel_ddi.c:1481-1484): `signal_levels` is not one of the 10.
    UnsupportedSignalLevel { signal_levels: u8 },
    /// `intel_ddi_set_link_train()` has no case for the pattern (intel_ddi.c:3827-3842).
    UnknownTrainingPattern { pattern: u8 },
    /// The translation table row is missing (never for a validated `BufTrans`).
    NoTransEntry { level: usize },
}

impl DdiError {
    pub const fn name(&self) -> &'static str {
        match self {
            DdiError::UnsupportedSignalLevel { .. } => "DDI-UNSUPPORTED-SIGNAL-LEVEL",
            DdiError::UnknownTrainingPattern { .. } => "DDI-UNKNOWN-TRAINING-PATTERN",
            DdiError::NoTransEntry { .. } => "DDI-NO-TRANS-ENTRY",
        }
    }
}

/// `translate_signal_level()` — intel_ddi.c:1467-1487: the row index of `signal_levels`, or `None` (Linux warns, returns 0).
pub fn translate_signal_level(signal_levels: u8) -> Option<usize> {
    INDEX_TO_DP_SIGNAL_LEVELS.iter().position(|&s| s == signal_levels)
}

/// `intel_ddi_dp_voltage_max()` — intel_ddi.c:1128-1150: the voltage swing of the LAST usable translation row.
/// `n_entries` is clamped to `1..=10` (the two `drm_WARN_ON`s).
pub fn ddi_dp_voltage_max(n_entries: usize) -> u8 {
    let n = n_entries.clamp(1, INDEX_TO_DP_SIGNAL_LEVELS.len());
    INDEX_TO_DP_SIGNAL_LEVELS[n - 1] & DP_TRAIN_VOLTAGE_SWING_MASK
}

/// `intel_ddi_dp_preemph_max()` — intel_ddi.c:1152-1156: every DDI platform may use the full pre-emphasis range.
pub const fn ddi_dp_preemph_max() -> u8 {
    DP_TRAIN_PRE_EMPH_LEVEL_3
}

/// `has_buf_trans_select()` — intel_ddi.c:119-122, display ver 9, not Broxton.
pub const HAS_BUF_TRANS_SELECT: bool = true; // intel_ddi.c:121
/// `has_iboost()` — intel_ddi.c:124-127: `DISPLAY_VER == 9 && !broxton`.
pub const HAS_IBOOST: bool = true; // intel_ddi.c:126

/// `intel_wait_ddi_buf_idle()` timeout for non-BXT: `intel_de_wait_for_set_ms(..., DDI_BUF_IS_IDLE, 10)` — intel_ddi.c:213-215.
pub const DDI_BUF_IDLE_TIMEOUT_US: u32 = 10_000;
/// `intel_ddi_set_idle_link_train()` wait: `intel_de_wait_for_set_ms(DP_TP_STATUS, DP_TP_STATUS_IDLE_DONE, 2)` — intel_ddi.c:3866.
pub const DP_IDLE_DONE_TIMEOUT_US: u32 = 2_000;
/// `intel_wait_ddi_buf_active()` for display ver < 10: `usleep_range(518, 1000)` — intel_ddi.c:244-246 (the lower bound).
pub const DDI_BUF_ACTIVE_WAIT_US: u32 = 518;
/// Poll spacing used for the register waits above (the `intel_de_wait_for_*` helpers spin 2 us then back off).
pub const DDI_POLL_INTERVAL_US: u32 = 10;

/// What defines a DDI DP output.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdiConfig {
    pub port: Ddi,
    pub lane_count: u8,
    /// `dig_port->lane_reversal` = VBT lane reversal || `DDI_BUF_PORT_REVERSAL` (intel_ddi.c:5413).
    pub lane_reversal: bool,
    /// `dig_port->ddi_a_4_lanes` = ver < 11 && `DDI_A_4_LANES` in DDI_BUF_CTL (intel_ddi.c:5416).
    pub ddi_a_4_lanes: bool,
    /// `dig_port->max_lanes` (port A: 4 or 2, `caps::ddi_a_max_lanes`).
    pub max_lanes: u8,
    /// `intel_bios_dp_boost_level(encoder->devdata)` = `ChildDevice::dp_boost_level()`: 0, or the TRANSLATED I_boost 1, 3 or 7.
    pub vbt_dp_boost: u8,
    pub trans: BufTrans,
    /// `crtc_state->enhanced_framing`.
    pub enhanced_framing: bool,
}

/// The DDI side of one link: `intel_dp->DP` and the config around it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdiLink {
    pub cfg: DdiConfig,
    /// `intel_dp->DP`: the `DDI_BUF_CTL` value to write.
    pub dp: u32,
}

impl DdiLink {
    /// `intel_ddi_init_dp_buf_reg()` — intel_ddi.c:353-377 (display ver < 14): "DDI_BUF_CTL_ENABLE will be set by
    /// intel_ddi_prepare_link_retrain() later".
    pub fn new(cfg: DdiConfig) -> DdiLink {
        let mut dp = ddi_port_width(cfg.lane_count as u32) | ddi_buf_trans_select(0);
        if cfg.lane_reversal {
            dp |= DDI_BUF_PORT_REVERSAL;
        }
        if cfg.ddi_a_4_lanes {
            dp |= DDI_A_4_LANES;
        }
        DdiLink { cfg, dp }
    }

    /// `hsw_prepare_dp_ddi_buffers()` — intel_ddi.c:134-157: every usable row into `DDI_BUF_TRANS_{LO,HI}`; row `trans1`
    /// gets `DDI_BUF_BALANCE_LEG_ENABLE` when the VBT asks for I_boost.
    pub fn prepare_dp_buffers(&self) -> Plan {
        let iboost_bit = if HAS_IBOOST && self.cfg.vbt_dp_boost != 0 { DDI_BUF_BALANCE_LEG_ENABLE } else { 0 };
        let mut p = Plan::new();
        for i in 0..self.cfg.trans.n_entries {
            let row = self.cfg.trans.entries[i];
            p.push(Effect::Write { reg: ddi_buf_trans_lo(self.cfg.port, i as u32), value: row.trans1 | iboost_bit });
            p.push(Effect::Write { reg: ddi_buf_trans_hi(self.cfg.port, i as u32), value: row.trans2 });
        }
        p
    }

    /// `intel_ddi_level()` for a DP output, lane 0 — intel_ddi.c:1491-1529.
    pub fn level(&self, train_set: &[u8; 4]) -> Result<usize, DdiError> {
        let sl = train_set[0] & (DP_TRAIN_VOLTAGE_SWING_MASK | DP_TRAIN_PRE_EMPHASIS_MASK);
        let level = translate_signal_level(sl).ok_or(DdiError::UnsupportedSignalLevel { signal_levels: sl })?;
        Ok(self.cfg.trans.clamp_level(level))
    }

    fn set_iboost_one(port: Ddi, iboost: u8) -> Effect {
        // `_skl_ddi_set_iboost()` — intel_ddi.c:1078-1090
        let clear = balance_leg_mask(port) | balance_leg_disable(port);
        let set = if iboost != 0 { (iboost as u32) << balance_leg_shift(port) } else { balance_leg_disable(port) };
        Effect::Rmw { reg: DISPIO_CR_TX_BMU_CR0, clear, set }
    }

    /// `skl_ddi_set_iboost()` — intel_ddi.c:1092-1126. An invalid I_boost is `drm_err`ed and NO register is written.
    fn iboost_plan(&self, level: usize) -> Result<Plan, DdiError> {
        let mut iboost = self.cfg.vbt_dp_boost;
        if iboost == 0 {
            iboost = self.cfg.trans.entry(level).map_err(|_| DdiError::NoTransEntry { level })?.i_boost;
        }
        let mut p = Plan::new();
        if iboost != 0 && iboost != 0x1 && iboost != 0x3 && iboost != 0x7 {
            return Ok(p);
        }
        p.push(Self::set_iboost_one(self.cfg.port, iboost));
        if self.cfg.port == Ddi::A && self.cfg.max_lanes == 4 {
            p.push(Self::set_iboost_one(Ddi::E, iboost));
        }
        Ok(p)
    }

    /// `hsw_set_signal_levels()` — intel_ddi.c:1531-1560, DP output: I_boost, then `intel_dp->DP` with the new
    /// `DDI_BUF_TRANS_SELECT` written to `DDI_BUF_CTL` and read back.
    pub fn set_signal_levels(&mut self, train_set: &[u8; 4]) -> Result<Plan, DdiError> {
        let level = self.level(train_set)?;
        let mut p = if HAS_IBOOST { self.iboost_plan(level)? } else { Plan::new() };
        self.dp &= !DDI_BUF_EMP_MASK;
        self.dp |= ddi_buf_trans_select(level as u32);
        let reg = ddi_buf_ctl_reg(self.cfg.port);
        p.push(Effect::Write { reg, value: self.dp });
        p.push(Effect::PostingRead { reg });
        Ok(p)
    }

    /// `intel_ddi_prepare_link_retrain()` + `intel_ddi_buf_enable()` — intel_ddi.c:3785-3815, 3088-3097: `DP_TP_CTL` enabled
    /// with pattern 1 (SST, enhanced framing as configured), then the DDI buffer, then the 518 us activation wait. After
    /// this `intel_dp->DP` carries `DDI_BUF_CTL_ENABLE`.
    pub fn prepare_link_retrain(&mut self) -> Plan {
        let tp = dp_tp_ctl(self.cfg.port);
        let mut dp_tp_ctl_val = DP_TP_CTL_ENABLE | DP_TP_CTL_LINK_TRAIN_PAT1 | DP_TP_CTL_MODE_SST;
        if self.cfg.enhanced_framing {
            dp_tp_ctl_val |= DP_TP_CTL_ENHANCED_FRAME_ENABLE;
        }
        let buf = ddi_buf_ctl_reg(self.cfg.port);
        let p = vec![
            Effect::Write { reg: tp, value: dp_tp_ctl_val },
            Effect::PostingRead { reg: tp },
            Effect::Write { reg: buf, value: self.dp | DDI_BUF_CTL_ENABLE },
            Effect::PostingRead { reg: buf },
            Effect::DelayUs(DDI_BUF_ACTIVE_WAIT_US),
        ];
        self.dp |= DDI_BUF_CTL_ENABLE;
        p
    }

    /// `intel_ddi_set_link_train()` — intel_ddi.c:3817-3846. `dp_train_pat` is the value written to
    /// `DP_TRAINING_PATTERN_SET` (scrambling bit ignored, `intel_dp_training_pattern_symbol`).
    pub fn set_link_train(&self, dp_train_pat: u8) -> Result<Plan, DdiError> {
        let sym = dp_train_pat & !DP_LINK_SCRAMBLING_DISABLE;
        let set = match sym {
            DP_TRAINING_PATTERN_DISABLE => DP_TP_CTL_LINK_TRAIN_NORMAL,
            DP_TRAINING_PATTERN_1 => DP_TP_CTL_LINK_TRAIN_PAT1,
            DP_TRAINING_PATTERN_2 => DP_TP_CTL_LINK_TRAIN_PAT2,
            DP_TRAINING_PATTERN_3 => DP_TP_CTL_LINK_TRAIN_PAT3,
            DP_TRAINING_PATTERN_4 => DP_TP_CTL_LINK_TRAIN_PAT4,
            other => return Err(DdiError::UnknownTrainingPattern { pattern: other }),
        };
        Ok(vec![Effect::Rmw { reg: dp_tp_ctl(self.cfg.port), clear: DP_TP_CTL_LINK_TRAIN_MASK, set }])
    }

    /// `intel_ddi_set_idle_link_train()` — intel_ddi.c:3848-3874. On port A before display ver 12 the idle-done wait is
    /// skipped ("Until TGL on PORT_A we can have only eDP in SST mode").
    pub fn set_idle_link_train(&self) -> Plan {
        let mut p = vec![Effect::Rmw { reg: dp_tp_ctl(self.cfg.port), clear: DP_TP_CTL_LINK_TRAIN_MASK, set: DP_TP_CTL_LINK_TRAIN_IDLE }];
        if self.cfg.port != Ddi::A {
            p.push(Effect::Poll(PollPlan::with_timeout(
                dp_tp_status(self.cfg.port),
                DP_TP_STATUS_IDLE_DONE,
                DP_TP_STATUS_IDLE_DONE,
                DDI_POLL_INTERVAL_US,
                DP_IDLE_DONE_TIMEOUT_US,
                "DP idle patterns",
            )));
        }
        p
    }

    /// `intel_dp_stop_link_train()`'s DDI half: program pattern DISABLE (normal transmission).
    pub fn stop_link_train(&self) -> Plan {
        // DP_TRAINING_PATTERN_DISABLE is always a known symbol.
        self.set_link_train(DP_TRAINING_PATTERN_DISABLE).unwrap_or_default()
    }

    /// `intel_ddi_buf_disable()` for DP, display ver < 14 — intel_ddi.c:3099-3123: buffer off, `DP_TP_CTL` off, then wait for
    /// the buffer to report idle (10 ms bound).
    pub fn buf_disable(&mut self) -> Plan {
        let buf = ddi_buf_ctl_reg(self.cfg.port);
        self.dp &= !DDI_BUF_CTL_ENABLE;
        vec![
            Effect::Rmw { reg: buf, clear: DDI_BUF_CTL_ENABLE, set: 0 },
            Effect::Rmw { reg: dp_tp_ctl(self.cfg.port), clear: DP_TP_CTL_ENABLE, set: 0 },
            Effect::Poll(PollPlan::with_timeout(buf, DDI_BUF_IS_IDLE, DDI_BUF_IS_IDLE, DDI_POLL_INTERVAL_US, DDI_BUF_IDLE_TIMEOUT_US, "DDI BUF idle")),
        ]
    }
}

/// `DDI_BUF_CTL(port)` as a [`Reg`](i915_common_core::Reg), from the one home of that register
/// (`i915-kbl-display-core::regs`).
pub fn ddi_buf_ctl_reg(port: Ddi) -> i915_common_core::Reg {
    i915_common_core::Reg::new(ddi_buf_ctl(port))
}
