// SPDX-License-Identifier: GPL-2.0-only
//! The eDP pipe-enable transaction for Kaby Lake: pipe A -> transcoder EDP -> DDI A, as an ORDERED list of [`Stage`]s, each with the
//! Linux function it is, a forward body, a rollback body, read-after-write checks and (where the hardware needs time) one bounded
//! settle wait.
//!
//! Ported from Linux drivers/gpu/drm/i915/display at v7.0-rc6. The order is `intel_atomic_commit_tail()` (crtc power domains,
//! `intel_set_cdclk_pre_plane_update`, `intel_dbuf_pre_plane_update`) -> `hsw_crtc_enable()` (intel_display.c) with the encoder hooks it
//! calls - `intel_ddi_pre_pll_enable`, `intel_dpll_enable`, `hsw_ddi_pre_enable_dp` (intel_ddi.c), `hsw_configure_cpu_transcoder`,
//! `skl_pfit_enable`, `hsw_set_linetime_wm`, `intel_ddi_enable` (func ctl, `intel_enable_transcoder`, `intel_ddi_enable_dp` ->
//! `intel_edp_backlight_on`) -> the plane update (`skl_plane_update_noarm`, `skl_plane_update_arm`, the cursor's
//! `skl_write_cursor_wm`). Copyright (c) Intel Corporation.
//!
//! COMPOSITION: the clock, link and power plans are built by their own crates from their own inputs (a DPLL snapshot, a VBT, a DPCD
//! reading ...) and handed in as [`SubPlans`]; this module orders them and adds the stages nobody else owns - DBUF slice, pipe
//! source / misc, M/N, transcoder timings, frame-start delay, TRANSCONF, line-time watermark, `TRANS_DDI_FUNC_CTL`, transcoder enable,
//! the plane / watermark / DDB / cursor plans. PPS sequencing, sink power and link training need AUX replies between writes, so they are
//! [`Interactive`] stages: the executor drives the `display-link-core` machine named there.
//!
//! NOT PORTED, each a stage with an `Empty` body that says so (never silently skipped): `intel_dmc_enable_pipe` (no pipe DMC before
//! display ver 12 - not even a stage), `intel_ddi_enable_transcoder_clock` (returns for TRANSCODER_EDP), `skl_pfit_enable` (no panel fit
//! on a native-mode eDP; any other source size is refused), `intel_color_modeset` (pipe gamma / CSC are left in the firmware's state
//! and every plane sets `PLANE_CTL_PIPE_GAMMA/CSC_ENABLE` to 0), `intel_initial_watermarks` (no such hook on display ver 9: the
//! watermarks are written by the plane update itself). Panel replay / PSR / FBC / audio / HDCP are not ported.
use crate::error::ModesetError;
use crate::plan::*;
use crate::plane_plan::*;
use alloc::vec;
use alloc::vec::Vec;
use i915_common_core::Reg;
use i915_display_clock_core::Transaction;
use i915_display_link_core::plan::{Effect, PollPlan};
use i915_kbl_display_core::mode_write::*;
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::regs_write::*;
use i915_kbl_display_core::skl_pfit::check_panel_fit;
use i915_kbl_display_core::skl_plane::{encode_plane, PlaneConfig, PlaneRegs, WriteFormat};
use i915_kbl_display_core::skl_wm::{
    compute_pipe_wm, max_level_for_vblank, single_pipe_alloc, Latencies, PipeWm, PipeWmInput, PlaneWmGeometry, VisiblePlane, WmTiming,
};
use i915_kbl_display_core::timing::ModeTimings;
use i915_kbl_display_core::types::{Ddi, Pipe, Transcoder};

/// Bound of the wait for `TRANSCONF_STATE_ENABLE` after `TRANSCONF_ENABLE` (Linux: `intel_wait_for_pipe_off` allows 100 ms on the way
/// down; one frame at 30 Hz is the bound the way up).
pub const PIPE_ENABLE_TIMEOUT_US: u32 = 100_000;
/// Bound of the wait for DBUF slice power (`gen9_dbuf_slice_set` waits `udelay(10)` and checks once).
pub const DBUF_POWER_TIMEOUT_US: u32 = 1_000;
/// Spacing of the settle polls.
pub const SETTLE_INTERVAL_US: u32 = 100;

/// The plans the other crates produce for THIS configuration, each already built from its own inputs.
#[derive(Clone, Debug)]
pub struct SubPlans {
    /// `intel_set_cdclk_pre_plane_update` -> `display-clock-core::cdclk::plan_set_cdclk` (`None`: CDCLK already right).
    pub cdclk: Option<Transaction>,
    /// `intel_modeset_get_crtc_power_domains`: PIPE_A + TRANSCODER_EDP (`PowerWells::get`).
    pub power_crtc: Transaction,
    /// `main_link_aux_power_domain_get` (`intel_ddi_pre_pll_enable`): AUX_A.
    pub power_aux: Transaction,
    /// `intel_dpll_enable` -> `dpll::plan_dpll0_edp` / `plan_enable`.
    pub dpll: Transaction,
    /// `intel_ddi_enable_clock` -> `dpll::plan_ddi_clock_enable`.
    pub ddi_clock: Transaction,
    /// `intel_display_power_get(ddi_io_power_domain)`: DDI_IO_A.
    pub power_ddi_io: Transaction,
    /// `hsw_prepare_dp_ddi_buffers` -> `DdiLink::prepare_dp_buffers`.
    pub ddi_buf_trans: Plan,
    /// `encoder->set_signal_levels` for the initial train set -> `DdiLink::set_signal_levels`.
    pub signal_levels: Plan,
    /// `intel_dp_stop_link_train`'s DDI half -> `DdiLink::stop_link_train`.
    pub stop_link_train: Plan,
    /// `intel_ddi_buf_disable` -> `DdiLink::buf_disable`: what undoes link training.
    pub ddi_buf_disable: Plan,
    /// `intel_backlight_enable` -> `BacklightState::enable_plan`.
    pub backlight_enable: Plan,
    /// `intel_backlight_disable` -> `BacklightState::disable_plan`.
    pub backlight_disable: Plan,
}

/// Everything the transaction needs.
pub struct EnableInputs<'a> {
    /// `adjusted_mode->crtc_*`.
    pub mode: ModeTimings,
    pub sync: SyncFlags,
    /// `crtc_clock`, kHz.
    pub pixel_clock_khz: u32,
    /// The DP port clock in 10 kbit/s units (270000 = HBR).
    pub port_clock: u32,
    pub lanes: u32,
    pub pipe_bpp: u32,
    /// The panel's native mode; the pipe source must equal it.
    pub panel_native: (u32, u32),
    /// The primary plane (PLANE_1 of pipe A).
    pub plane: PlaneConfig,
    /// Adjusted memory latencies (`skl_wm::adjust_latencies` output) and whether IPC is enabled.
    pub latencies: Latencies,
    pub ipc_enabled: bool,
    /// `calc_cdclk(min_cdclk)` for the prefill check.
    pub cdclk_khz: u32,
    /// The register values read BEFORE the transaction: the rollback restores these.
    pub snap: &'a dyn Fn(u32) -> u32,
    pub sub: SubPlans,
}

/// The built transaction.
#[derive(Clone, Debug)]
pub struct EnableTransaction {
    pub stages: Vec<Stage>,
    /// The watermark / DDB result the plane and cursor stages were built from.
    pub pipe_wm: PipeWm,
    pub plane_regs: PlaneRegs,
}

const EDP: Transcoder = Transcoder::Edp;
const PIPE: Pipe = Pipe::A;

fn wr(reg: u32, value: u32) -> Effect {
    Effect::Write { reg: Reg::new(reg), value }
}

fn settle(reg: u32, mask: u32, want: u32, timeout_us: u32, what: &'static str) -> PollPlan {
    PollPlan::with_timeout(Reg::new(reg), mask, want, SETTLE_INTERVAL_US, timeout_us, what)
}

/// A stage made of one local plan: rollback = the snapshot restore, readback = the written values.
fn local(id: StageId, linux: &'static str, fwd: Plan, snap: &dyn Fn(u32) -> u32, full: bool) -> Stage {
    let rollback = Body::Link(restore_plan(&fwd, snap));
    let readback = readbacks_of(&fwd, full);
    Stage { id, linux, forward: Body::Link(fwd), rollback, readback, settle: None }
}

fn composed(id: StageId, linux: &'static str, tx: &Transaction) -> Stage {
    Stage { id, linux, forward: Body::Clock(tx.forward), rollback: Body::Clock(tx.rollback), readback: vec![], settle: None }
}

fn empty(id: StageId, linux: &'static str, why: &'static str) -> Stage {
    Stage { id, linux, forward: Body::Empty(why), rollback: Body::Empty(why), readback: vec![], settle: None }
}

fn visible(c: &PlaneConfig) -> VisiblePlane {
    let nv12 = c.fb.format == WriteFormat::Nv12;
    let g = |color_plane: u8| PlaneWmGeometry {
        width: c.src_w,
        cpp: if nv12 { if color_plane == 0 { 1 } else { 2 } } else { c.fb.format.cpp() },
        x_tiled: c.fb.tiling == i915_kbl_display_core::skl_plane::WriteTiling::X,
        y_tiled: c.fb.tiling == i915_kbl_display_core::skl_plane::WriteTiling::Y,
        is_planar: nv12,
        color_plane,
    };
    VisiblePlane { plane_id: c.plane_id, y: g(0), uv: if nv12 { Some(g(1)) } else { None }, src_h: c.src_h }
}

/// Build the transaction. Everything that can be refused is refused here, before any stage exists.
pub fn build_enable(inp: &EnableInputs) -> Result<EnableTransaction, ModesetError> {
    let snap = inp.snap;
    let c = &inp.plane;
    if c.pipe != PIPE || c.plane_id != 0 {
        return Err(ModesetError::Mode(ModeError::NotEdpOnDdiA));
    }
    // Pipe source = the mode size; it must be the panel's native size (no scaler).
    let (src_w, src_h) = (inp.mode.hdisplay, inp.mode.vdisplay);
    check_panel_fit(src_w, src_h, inp.panel_native.0, inp.panel_native.1)?;

    // ---- values ----
    let plane_regs = encode_plane(c)?;
    let t = WmTiming {
        htotal: inp.mode.htotal,
        clock_khz: inp.pixel_clock_khz,
        pixel_rate_khz: inp.pixel_clock_khz,
        vblank_lines: inp.mode.vtotal.saturating_sub(inp.mode.vblank_start),
    };
    let mut pipe_wm = compute_pipe_wm(&PipeWmInput {
        lat: &inp.latencies,
        ipc_enabled: inp.ipc_enabled,
        timing: t,
        planes: [Some(visible(c)), None, None],
        alloc: single_pipe_alloc(),
        num_active_pipes: 1,
    })?;
    let max_level = max_level_for_vblank(&inp.latencies, &t, pipe_wm.max_wm0_lines(), inp.cdclk_khz)?;
    pipe_wm.limit_levels(max_level);
    let wm = plane_wm_writes(PIPE, 0, &pipe_wm.planes[0], &pipe_wm.plane_ddb[0], &pipe_wm.plane_ddb_y[0]);
    let commit = plane_commit(&plane_regs, &wm);
    let cursor = cursor_hidden_plan(PIPE, &pipe_wm);

    let pipesrc_v = pipesrc_value(src_w, src_h)?;
    let misc_v = pipe_misc_value(inp.pipe_bpp)?;
    let mn = compute_link_mn(inp.pixel_clock_khz, inp.port_clock, inp.lanes, inp.pipe_bpp)?;
    let timings = timing_writes(EDP, &inp.mode)?;
    let (fsd_clear, fsd_set) = frame_start_delay_rmw(i915_kbl_display_core::skl_wm::FRAMESTART_DELAY);
    let linetime = linetime_wm(inp.mode.htotal, inp.pixel_clock_khz)?;
    let func = ddi_func_ctl_value(Ddi::A, EDP, PIPE, inp.pipe_bpp, inp.sync, inp.lanes)?;
    let trans_pipe = Transcoder::of_pipe(PIPE);

    // ---- stages, in hsw_crtc_enable() order ----
    let mut st: Vec<Stage> = Vec::new();
    st.push(composed(StageId::PowerDomains, "intel_modeset_get_crtc_power_domains", &inp.sub.power_crtc));
    match &inp.sub.cdclk {
        Some(tx) => st.push(composed(StageId::CdclkSet, "intel_set_cdclk_pre_plane_update", tx)),
        None => st.push(empty(StageId::CdclkSet, "intel_set_cdclk_pre_plane_update", "CDCLK already at the target")),
    }
    // intel_dbuf_pre_plane_update -> gen9_dbuf_slice_set(S1): powered up only if the snapshot says it is down.
    if snap(DBUF_CTL_S0) & DBUF_POWER_STATE != 0 {
        st.push(empty(StageId::DbufSlice, "intel_dbuf_pre_plane_update", "DBUF slice 1 already powered"));
    } else {
        let fwd = vec![
            Effect::Rmw { reg: Reg::new(DBUF_CTL_S0), clear: 0, set: DBUF_POWER_REQUEST },
            Effect::PostingRead { reg: Reg::new(DBUF_CTL_S0) },
        ];
        st.push(Stage {
            id: StageId::DbufSlice,
            linux: "gen9_dbuf_slice_set",
            forward: Body::Link(fwd),
            // put the request bit back to what the snapshot had (not blindly cleared: the firmware may have requested it)
            rollback: Body::Link(vec![Effect::Rmw { reg: Reg::new(DBUF_CTL_S0), clear: DBUF_POWER_REQUEST, set: snap(DBUF_CTL_S0) & DBUF_POWER_REQUEST }]),
            readback: vec![],
            settle: Some(settle(DBUF_CTL_S0, DBUF_POWER_STATE, DBUF_POWER_STATE, DBUF_POWER_TIMEOUT_US, "DBUF slice power")),
        });
    }
    st.push(composed(StageId::AuxPower, "intel_ddi_pre_pll_enable (main_link_aux_power_domain_get)", &inp.sub.power_aux));
    st.push(composed(StageId::DpllEnable, "intel_dpll_enable", &inp.sub.dpll));
    // hsw_ddi_pre_enable_dp()
    st.push(Stage {
        id: StageId::PpsPanelOn,
        linux: "intel_pps_on",
        forward: Body::Interactive(Interactive::PpsPanelOn),
        rollback: Body::Interactive(Interactive::PpsPanelOff),
        readback: vec![],
        settle: None,
    });
    st.push(composed(StageId::DdiClockEnable, "intel_ddi_enable_clock", &inp.sub.ddi_clock));
    st.push(composed(StageId::DdiIoPower, "intel_display_power_get(ddi_io_power_domain)", &inp.sub.power_ddi_io));
    st.push(local(StageId::DdiBufTrans, "hsw_prepare_dp_ddi_buffers", inp.sub.ddi_buf_trans.clone(), snap, false));
    st.push(local(StageId::SignalLevels, "encoder->set_signal_levels", inp.sub.signal_levels.clone(), snap, false));
    st.push(Stage {
        id: StageId::SinkPowerD0,
        linux: "intel_dp_set_power(DP_SET_POWER_D0)",
        forward: Body::Interactive(Interactive::SinkPowerD0),
        rollback: Body::Interactive(Interactive::SinkPowerD3),
        readback: vec![],
        settle: None,
    });
    st.push(Stage {
        id: StageId::LinkTrain,
        linux: "intel_dp_start_link_train",
        forward: Body::Interactive(Interactive::LinkTraining),
        rollback: Body::Link(inp.sub.ddi_buf_disable.clone()),
        readback: vec![],
        settle: None,
    });
    st.push(local(StageId::StopLinkTrain, "intel_dp_stop_link_train", inp.sub.stop_link_train.clone(), snap, false));
    st.push(empty(StageId::TranscoderClock, "intel_ddi_enable_transcoder_clock", "returns for TRANSCODER_EDP: the eDP transcoder has no TRANS_CLK_SEL"));
    // hsw_crtc_enable(): pipe source / misc, then hsw_configure_cpu_transcoder()
    st.push(local(StageId::PipeSrc, "intel_set_pipe_src_size", vec![wr(pipesrc(trans_pipe), pipesrc_v)], snap, true));
    st.push(local(StageId::PipeMisc, "bdw_set_pipe_misc", vec![wr(pipe_misc(PIPE), misc_v)], snap, false));
    st.push(local(StageId::LinkMn, "intel_cpu_transcoder_set_m1_n1", writes_plan(&mn_writes(EDP, &mn)), snap, true));
    st.push(local(StageId::TranscoderTimings, "intel_set_transcoder_timings", writes_plan(&timings), snap, true));
    {
        let reg = chicken_trans(EDP);
        let fwd = vec![Effect::Rmw { reg: Reg::new(reg), clear: fsd_clear, set: fsd_set }, Effect::PostingRead { reg: Reg::new(reg) }];
        st.push(Stage {
            id: StageId::FrameStartDelay,
            linux: "hsw_set_frame_start_delay",
            forward: Body::Link(fwd),
            rollback: Body::Link(vec![wr(reg, snap(reg))]),
            readback: vec![Readback { reg: Reg::new(reg), mask: fsd_clear, want: fsd_set }],
            settle: None,
        });
    }
    {
        let reg = transconf(EDP);
        let fwd = vec![wr(reg, transconf_modeset_value()), Effect::PostingRead { reg: Reg::new(reg) }];
        st.push(Stage {
            id: StageId::Transconf,
            linux: "hsw_set_transconf",
            forward: Body::Link(fwd),
            rollback: Body::Link(vec![wr(reg, snap(reg))]),
            // the pipe must still be OFF after a modeset-time TRANSCONF write
            readback: vec![Readback { reg: Reg::new(reg), mask: TRANSCONF_ENABLE | TRANSCONF_STATE_ENABLE, want: 0 }],
            settle: None,
        });
    }
    st.push(empty(StageId::PanelFit, "skl_pfit_enable", "pch_pfit.enabled is false for a native-mode eDP: no scaler register is written"));
    st.push(empty(
        StageId::ColorModeset,
        "intel_color_modeset",
        "NOT PORTED: pipe gamma / CSC left in the firmware's state; the plane sets PIPE_GAMMA/CSC_ENABLE to 0",
    ));
    st.push(local(StageId::LinetimeWm, "hsw_set_linetime_wm", vec![wr(wm_linetime(PIPE), linetime_reg_value(linetime))], snap, true));
    // intel_ddi_enable(): func ctl, then the transcoder
    st.push(local(StageId::DdiFuncCtl, "intel_ddi_enable_transcoder_func", vec![wr(i915_kbl_display_core::regs::trans_ddi_func_ctl(EDP), func)], snap, false));
    {
        let reg = transconf(EDP);
        let fwd = vec![Effect::Rmw { reg: Reg::new(reg), clear: 0, set: TRANSCONF_ENABLE }, Effect::PostingRead { reg: Reg::new(reg) }];
        st.push(Stage {
            id: StageId::TranscoderEnable,
            linux: "intel_enable_transcoder",
            forward: Body::Link(fwd),
            // the enable bit back to what the snapshot had (the reverse of the only bit this stage sets)
            rollback: Body::Link(vec![Effect::Rmw { reg: Reg::new(reg), clear: TRANSCONF_ENABLE, set: snap(reg) & TRANSCONF_ENABLE }]),
            readback: vec![Readback { reg: Reg::new(reg), mask: TRANSCONF_ENABLE, want: TRANSCONF_ENABLE }],
            settle: Some(settle(reg, TRANSCONF_STATE_ENABLE, TRANSCONF_STATE_ENABLE, PIPE_ENABLE_TIMEOUT_US, "TRANSCONF pipe state")),
        });
    }
    // intel_ddi_enable_dp(): intel_edp_backlight_on() = PWM enable THEN panel-power backlight (intel_dp.c)
    st.push(Stage {
        id: StageId::BacklightPwm,
        linux: "intel_backlight_enable",
        forward: Body::Link(inp.sub.backlight_enable.clone()),
        rollback: Body::Link(inp.sub.backlight_disable.clone()),
        readback: vec![],
        settle: None,
    });
    st.push(Stage {
        id: StageId::PpsBacklightOn,
        linux: "intel_pps_backlight_on",
        forward: Body::Interactive(Interactive::PpsBacklightOn),
        rollback: Body::Interactive(Interactive::PpsBacklightOff),
        readback: vec![],
        settle: None,
    });
    // planes (after the pipe runs: intel_enable_transcoder asserts planes disabled)
    {
        let rb = restore_all(&[&commit.noarm], snap);
        st.push(Stage {
            id: StageId::PlaneNoarm,
            linux: "skl_plane_update_noarm",
            forward: Body::Link(commit.noarm.clone()),
            rollback: Body::Link(rb),
            readback: readbacks_of(&commit.noarm, true),
            settle: None,
        });
    }
    {
        let rb = restore_all(&[&commit.arm], snap);
        st.push(Stage {
            id: StageId::PlaneArm,
            linux: "skl_plane_update_arm",
            forward: Body::Link(commit.arm.clone()),
            rollback: Body::Link(rb),
            readback: vec![Readback { reg: Reg::new(plane_regs.surf_reg()), mask: PLANE_SURF_ADDR_MASK, want: plane_regs.surf & PLANE_SURF_ADDR_MASK }],
            settle: Some(commit.latch),
        });
    }
    st.push(local(StageId::CursorWm, "skl_write_cursor_wm", cursor, snap, true));

    Ok(EnableTransaction { stages: st, pipe_wm, plane_regs })
}

impl EnableTransaction {
    /// The stage ids in order.
    pub fn order(&self) -> Vec<StageId> {
        self.stages.iter().map(|s| s.id).collect()
    }
    pub fn stage(&self, id: StageId) -> Option<&Stage> {
        self.stages.iter().find(|s| s.id == id)
    }
    /// The rollback bodies to run when stage `failed` did not complete: that stage's own rollback (safe whether or not it finished),
    /// then every earlier stage's, newest first.
    pub fn rollback_from(&self, failed: usize) -> Result<Vec<&Body>, ModesetError> {
        if failed >= self.stages.len() {
            return Err(ModesetError::NoSuchStage { index: failed });
        }
        Ok(self.stages[..=failed].iter().rev().map(|s| &s.rollback).collect())
    }
    /// Every bounded wait in the transaction (stage bodies, rollbacks and settle polls) with its sample bound.
    pub fn all_polls(&self) -> Vec<(u32, &'static str)> {
        let mut v = Vec::new();
        for s in &self.stages {
            v.extend(s.forward.polls());
            v.extend(s.rollback.polls());
            if let Some(p) = &s.settle {
                v.push((p.max_polls, p.what));
            }
        }
        v
    }
}
