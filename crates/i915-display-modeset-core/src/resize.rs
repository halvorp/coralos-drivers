// SPDX-License-Identifier: GPL-2.0-only
//! THE ADOPTED PIPE'S LOGICAL SCANOUT SIZE, as a FASTSET: the pipe source (PIPESRC), the primary plane's size / stride / watermarks and
//! the pipe scaler's phases change on the LIVE pipe; the transcoder, its timings, the link, the clocks and the eDP input mux do not.
//!
//! WHY A FASTSET IS ENOUGH (Linux v7.0-rc6, drivers/gpu/drm/i915/display/intel_display.c). `intel_pipe_config_compare()` checks
//! `pipe_src`, `pch_pfit.enabled`, `pch_pfit.dst`, `scaler_state.scaler_id` and `pixel_rate` only `if (!fastset)` (:5356-5363): a change
//! of any of them does NOT force a full modeset. `pch_pfit.force_thru` IS checked in a fastset (:5350-5354, "Changing the EDP
//! transcoder input mux (A_ONOFF vs. A_ON) requires a full modeset"), so the fitter's on/off state must not change on an eDP transcoder
//! - which is why the eDP case is planned only when the firmware already left the fitter on. The fastset itself is
//! `intel_pipe_fastset()` (:6638-6683): `intel_set_pipe_src_size()` (PIPESRC = (w-1, h-1), :2820-2833), then on display ver >= 9
//! `skl_pfit_enable()`, then `hsw_set_linetime_wm()` (unchanged here: an UPSCALE leaves the pipe pixel rate alone, `ilk_pipe_pixel_rate`
//! :2189-2207 with `intel_adjusted_rate` taking `min(src, dst)`, intel_plane.c:206-223 - which also leaves CDCLK alone); the plane update
//! that follows in the same commit is `skl_plane_update_noarm()` (PLANE_STRIDE, PLANE_POS, PLANE_SIZE, then `skl_write_plane_wm`,
//! skl_universal_plane.c:1404-1432) and `skl_plane_update_arm()` (PLANE_SURF last, which arms the plane, :1435-1500). All of it is
//! written inside one vblank-evasion window (`intel_vblank_evade`, intel_vblank.c:682-760) so the pipe, the scaler and the plane
//! latch on the same vblank: [`VblankEvade`].
//!
//! THE WATERMARKS ARE A BOUND, NOT LINUX'S NUMBER. Linux recomputes them from the PCU's memory latencies (`skl_compute_plane_wm`,
//! skl_watermark.c:1793-1935); i915.device does not execute the PCU mailbox read (`latency` plans it; the PCU mailbox registers are not on its write allow-list), so it
//! has no latencies to compute from. Every width-dependent term of that function is
//! proportional to `plane_blocks_per_line` (method2, the `blocks >= plane_blocks_per_line` floor), every other term is width-independent,
//! and `lines` does not grow with the width. So the firmware's own level, its blocks scaled by `new_width / old_width` (rounded up) plus
//! one block per line of the level and one more, is never below what Linux would program for the wider plane ([`rescale_wm`]; the host test checks it against the
//! Rust port of `skl_compute_wm_levels`, itself checked against GCC-compiled Linux). A level that then no longer fits the plane's
//! DDB is DISABLED exactly as `skl_check_wm_level` does (:1353-1357); level 0 not fitting is a named refusal (`ddb-too-small`).
//!
//! THE DDB IS RECOMPUTED AS LINUX ALLOCATES IT, where that needs no latency. The fastset runs `skl_compute_ddb` and `skl_write_plane_wm`
//! writes PLANE_BUF_CFG / PLANE_NV12_BUF_CFG after the watermark words (skl_universal_plane.c:821-860), latched with PLANE_SURF. With
//! ONE active pipe its DDB is the whole DBUF (`intel_crtc_ddb_weight` = the pipe mode's hdisplay, the only weight: skl_watermark.c:471-486,
//! :526-560; `dbuf.size` 896 less 4); `skl_crtc_allocate_plane_ddb` puts the cursor's fixed reserve at the top (:1452-1458,
//! `skl_cursor_allocation` = `max(32, ..)` with one pipe, :633-668) and grants the only plane with a data rate every other block
//! (:1505-1530). So the plane gets `[0, dbuf - cursor)` and the cursor `[dbuf - cursor, dbuf)` - WHEREVER the firmware left either
//! (silicon, 2026-10-29: PLANE_1 0..152 and the cursor's reserve off the top refused a 2560-wide plane that Linux's 860 holds). A
//! cursor entry that changes is written as Linux does: `skl_ddb_add_affected_planes` adds the cursor to the commit (:2426-2459) and
//! `i9xx_cursor_update_arm` (NULL state: hidden) writes `skl_write_cursor_wm` - zero CUR_WM levels, CUR_WM_TRANS, CUR_BUF_CFG - then
//! CURPOS and CURBASE, which arms it (intel_cursor.c:616-716). A HIDDEN cursor's reserve is the firmware's when it is already at the
//! top and at least the floor (no cursor write), else Linux's one-pipe floor of 32 (its latency term needs the PCU latencies this port
//! does not read; a hidden cursor's watermarks are zero, so the floor is all it uses). KEPT, named in `ddb_how`: another active pipe
//! (the DDB is split by `intel_crtc_dbuf_weights`, :488-520, not ported) and a VISIBLE cursor (CURCNTR & MCURSOR_MODE_MASK,
//! `i9xx_cursor_get_hw_state`, intel_cursor.c:744-746) whose reserve is not at the top - its size is `skl_cursor_allocation`'s from
//! those latencies, so it is never resized here; a visible cursor already at the top keeps its reserve and the plane gets the rest. `hsw_set_linetime_wm` is rewritten with the fastset (intel_display.c:6662-6677; `skl_linetime_wm`
//! :4210-4229 from the pipe pixel rate, which an upscale leaves at the dotclock).
//!
//! Nothing here performs MMIO: [`read_firmware_geometry`] asks a closure for registers, and the plans are effect lists the device
//! executes through its write guard ([`RESIZE_WRITE_ALLOWLIST`]). Copyright (c) Intel Corporation.
use crate::initial::AdoptedDisplay;
use alloc::vec::Vec;
use i915_common_core::Reg;
use i915_display_link_core::plan::{written_regs, Effect, Plan};
use i915_kbl_display_core::plane::Tiling;
use i915_kbl_display_core::mode_write::{linetime_reg_value, linetime_wm};
use i915_kbl_display_core::platform::DisplayCaps;
use i915_kbl_display_core::readout::{PipeState, ReadoutState};
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::regs_write::*;
use i915_kbl_display_core::skl_pfit::{aspect_window, pfit_enable_writes, PfitError, Window};
use i915_kbl_display_core::skl_plane::WriteTiling;
use i915_kbl_display_core::skl_wm::{Ddb, NUM_LEVELS};
use crate::plane_plan::cursor_hidden_writes;
use i915_kbl_display_core::transcoder::DdiMode;
use i915_kbl_display_core::types::{Pipe, Transcoder};

/// `VBLANK_EVASION_TIME_US` is 100 in Linux (intel_crtc.h:29). This device's MMIO path is a ring-3 window whose per-write cost is not
/// yet measured on silicon, and the burst is ~20 writes, so the window it evades is ten times Linux's: still well under one frame.
pub const EVADE_US: u32 = 1000;

/// How the logical (pipe source) size is chosen. A CONFIG value: the host passes it, the default derives it from the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogicalSize {
    /// Keep the firmware's framebuffer size (no resize).
    Adopted,
    /// `panel * num / den` in each direction, each rounded down to even (default: 2/3, i.e. panel / 1.5) - but never smaller than
    /// the firmware's framebuffer: on a panel that is not high-density the fraction would shrink the desktop, so the adopted size stays.
    Fraction { num: u32, den: u32 },
    /// Exactly this size.
    Exact { width: u32, height: u32 },
}

impl Default for LogicalSize {
    fn default() -> Self {
        LogicalSize::Fraction { num: 2, den: 3 }
    }
}

impl LogicalSize {
    /// The config text a host carries: `adopted`, `N/D` (a fraction of the panel, e.g. `2/3`, `3/4`), or `WxH`. `None` for anything
    /// else (the host then names the bad value and keeps its default).
    pub fn parse(s: &str) -> Option<LogicalSize> {
        let s = s.trim();
        if s == "adopted" {
            return Some(LogicalSize::Adopted);
        }
        let num = |t: &str| t.trim().parse::<u32>().ok();
        if let Some((n, d)) = s.split_once('/') {
            let (num, den) = (num(n)?, num(d)?);
            return (num != 0 && den != 0 && num <= den).then_some(LogicalSize::Fraction { num, den });
        }
        let (w, h) = s.split_once('x')?;
        let (width, height) = (num(w)?, num(h)?);
        (width != 0 && height != 0).then_some(LogicalSize::Exact { width, height })
    }

    /// The size this policy picks for a panel of `panel` with the firmware's framebuffer `adopted`.
    pub fn resolve(self, panel: (u32, u32), adopted: (u32, u32)) -> (u32, u32) {
        match self {
            LogicalSize::Adopted => adopted,
            LogicalSize::Fraction { num, den } if den != 0 => {
                let f = |v: u32| ((v as u64 * num as u64 / den as u64) as u32) & !1;
                let (w, h) = (f(panel.0), f(panel.1));
                if w < adopted.0 || h < adopted.1 {
                    adopted
                } else {
                    (w, h)
                }
            }
            LogicalSize::Fraction { .. } => adopted,
            LogicalSize::Exact { width, height } => (width, height),
        }
    }
}

/// THE resize write allow-list: the pipe source of pipe A, both of pipe A's scaler blocks (CTRL, VPHASE, HPHASE, WIN_POS, WIN_SZ),
/// PLANE_1's STRIDE / POS / SIZE, its eight watermark levels and its transition watermark. The PLANE_SURF write is
/// `adopt::ADOPT_WRITE_ALLOWLIST`'s. Pipe A / plane 1 only - the same pipe and plane the adoption's flip is limited to.
/// PLANE_1's PLANE_BUF_CFG / PLANE_NV12_BUF_CFG (`skl_write_plane_wm`) and pipe A's WM_LINETIME (`hsw_set_linetime_wm`) with them.
/// The cursor block of pipe A (`skl_write_cursor_wm` + CURPOS / CURBASE) when its DDB entry moves.
pub const RESIZE_WRITE_ALLOWLIST: [Reg; 38] = [
    Reg::new(pipesrc(Transcoder::A)),
    ps_a(PS_1A_CTRL, 0),
    ps_a(PS_VPHASE_1A, 0),
    ps_a(PS_HPHASE_1A, 0),
    ps_a(PS_WIN_POS_1A, 0),
    ps_a(PS_WIN_SZ_1A, 0),
    ps_a(PS_1A_CTRL, 1),
    ps_a(PS_VPHASE_1A, 1),
    ps_a(PS_HPHASE_1A, 1),
    ps_a(PS_WIN_POS_1A, 1),
    ps_a(PS_WIN_SZ_1A, 1),
    Reg::new(PLANE_STRIDE_1_A),
    Reg::new(PLANE_POS_1_A),
    Reg::new(PLANE_SIZE_1_A),
    wm_a(0),
    wm_a(1),
    wm_a(2),
    wm_a(3),
    wm_a(4),
    wm_a(5),
    wm_a(6),
    wm_a(7),
    Reg::new(PLANE_WM_TRANS_1_A),
    Reg::new(PLANE_BUF_CFG_1_A),
    Reg::new(PLANE_NV12_BUF_CFG_1_A),
    Reg::new(WM_LINETIME_A),
    Reg::new(cur_wm(Pipe::A, 0)),
    Reg::new(cur_wm(Pipe::A, 1)),
    Reg::new(cur_wm(Pipe::A, 2)),
    Reg::new(cur_wm(Pipe::A, 3)),
    Reg::new(cur_wm(Pipe::A, 4)),
    Reg::new(cur_wm(Pipe::A, 5)),
    Reg::new(cur_wm(Pipe::A, 6)),
    Reg::new(cur_wm(Pipe::A, 7)),
    Reg::new(CUR_WM_TRANS_A),
    Reg::new(CUR_BUF_CFG_A),
    Reg::new(CURAPOS),
    Reg::new(CURABASE),
];
const fn ps_a(base: u32, id: u32) -> Reg {
    Reg::new(scaler_reg(base, Pipe::A, id))
}
const fn wm_a(level: u32) -> Reg {
    Reg::new(plane_wm(Pipe::A, 0, level))
}

/// What the firmware programmed for the adopted pipe, read raw: what the rollback writes back, and the starting point of the plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareGeometry {
    pub pipe: Pipe,
    /// The scaler the plan uses: the firmware's pipe-bound fitter, or scaler 0 when there is none (`intel_atomic_setup_scaler`'s
    /// display-ver-9 choice for a single scaler user).
    pub scaler_id: u8,
    /// Whether the firmware left that scaler on as the pipe's panel fitter.
    pub fitter_on: bool,
    pub pipesrc: u32,
    /// The scaler's PS_CTRL, PS_VPHASE, PS_HPHASE, PS_WIN_POS, PS_WIN_SZ.
    pub ps: [u32; 5],
    pub plane_stride: u32,
    pub plane_pos: u32,
    pub plane_size: u32,
    pub wm: [u32; NUM_LEVELS],
    pub wm_trans: u32,
    pub buf_cfg: u32,
    pub nv12_buf_cfg: u32,
    /// The pipe's CUR_BUF_CFG: where the cursor's DDB reserve sits (never moved by the resize).
    pub cur_buf_cfg: u32,
    /// The cursor's CURCNTR, CUR_WM levels, CUR_WM_TRANS, CURPOS and CURBASE: what the restore writes back when the cursor's entry moves.
    pub curcntr: u32,
    pub cur_wm: [u32; NUM_LEVELS],
    pub cur_wm_trans: u32,
    pub curpos: u32,
    pub curbase: u32,
    /// WM_LINETIME as the firmware left it.
    pub linetime: u32,
    /// Another pipe is active: the DDB is split by weight among the pipes (not ported), so the plane's DDB is kept.
    pub other_pipes_active: bool,
    /// The transcoder timing the vblank evasion needs.
    pub vtotal: u32,
    pub vblank_start: u32,
    pub htotal: u32,
    pub dotclock_khz: Option<u32>,
    pub hdmi: bool,
    pub transcoder: Transcoder,
    pub scalers_on_pipe: u8,
}

/// Read the adopted pipe's geometry registers through `rd` (every one is a read; nothing is written). `None` when the readout's pipe
/// is not active (the adoption would not have been made).
pub fn read_firmware_geometry<R: FnMut(u32) -> u32>(mut rd: R, caps: &DisplayCaps, r: &ReadoutState, a: &AdoptedDisplay) -> Option<FirmwareGeometry> {
    let pr = &r.pipes[a.pipe.index()];
    let PipeState::Active(act) = pr.state else { return None };
    let (scaler_id, fitter_on) = match a.fitter {
        Some(f) => (f.id, true),
        None => (0, false),
    };
    let s = |base| scaler_reg(base, a.pipe, scaler_id as u32);
    let p = |base| plane_reg(base, a.pipe, a.fb.plane_id as u32);
    let mut wm = [0u32; NUM_LEVELS];
    for (level, v) in wm.iter_mut().enumerate() {
        *v = rd(plane_wm(a.pipe, a.fb.plane_id as u32, level as u32));
    }
    Some(FirmwareGeometry {
        pipe: a.pipe,
        scaler_id,
        fitter_on,
        pipesrc: rd(pipesrc(Transcoder::of_pipe(a.pipe))),
        ps: [rd(s(PS_1A_CTRL)), rd(s(PS_VPHASE_1A)), rd(s(PS_HPHASE_1A)), rd(s(PS_WIN_POS_1A)), rd(s(PS_WIN_SZ_1A))],
        plane_stride: rd(p(PLANE_STRIDE_1_A)),
        plane_pos: rd(p(PLANE_POS_1_A)),
        plane_size: rd(p(PLANE_SIZE_1_A)),
        wm,
        wm_trans: rd(p(PLANE_WM_TRANS_1_A)),
        buf_cfg: rd(p(PLANE_BUF_CFG_1_A)),
        nv12_buf_cfg: rd(p(PLANE_NV12_BUF_CFG_1_A)),
        cur_buf_cfg: rd(cur_buf_cfg(a.pipe)),
        curcntr: rd(curcntr(a.pipe)),
        cur_wm: core::array::from_fn(|level| rd(cur_wm(a.pipe, level as u32))),
        cur_wm_trans: rd(cur_wm_trans(a.pipe)),
        curpos: rd(curpos(a.pipe)),
        curbase: rd(curbase(a.pipe)),
        linetime: rd(wm_linetime(a.pipe)),
        other_pipes_active: r.pipes.iter().enumerate().any(|(i, p)| i != a.pipe.index() && matches!(p.state, PipeState::Active(_))),
        vtotal: act.timings.vtotal,
        vblank_start: act.timings.vblank_start,
        htotal: act.timings.htotal,
        dotclock_khz: act.dotclock_khz,
        hdmi: act.func.mode == DdiMode::Hdmi,
        transcoder: act.transcoder,
        scalers_on_pipe: caps.scalers_of(a.pipe),
    })
}

/// A resize that is not planned, named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeRefusal {
    /// A zero size.
    SizeInvalid { w: u32, h: u32 },
    /// Only pipe A / plane 1 is on the write allow-list (as for the adoption's flip).
    NotInAllowList { pipe: Pipe, plane_id: u8 },
    /// Bigger than the panel: a DOWNSCALE raises the pipe pixel rate (`intel_adjusted_rate`), hence CDCLK, the line time and every
    /// watermark - not a fastset this port makes.
    Downscale { w: u32, h: u32, panel_w: u32, panel_h: u32 },
    /// No pipe fitter on an eDP transcoder: turning one on flips the eDP input mux (A_ON -> A_ONOFF), a full modeset
    /// (intel_display.c:5350-5354).
    EdpMuxNeedsModeset,
    /// The pipe has no scaler at all (`intel_allocate_scaler` finds none free).
    NoScalerFree,
    /// The tiling has no stride rule here (Yf).
    TilingNotPorted,
    /// Outside `skl_plane_max_width` / `max_height` / `skl_plane_max_stride`.
    PlaneOutOfRange { w: u32, h: u32, stride: u32 },
    /// The scaler refused (range or scale factor), by its own name.
    Scaler(PfitError),
    /// The plane has no DDB allocation to hold any watermark.
    NoDdb,
    /// The wider plane's level-0 watermark needs more blocks than its DDB holds (`skl_crtc_allocate_plane_ddb`: no level fits ->
    /// -EINVAL "Requested display configuration exceeds system DDB limitations", skl_watermark.c:1494-1499).
    DdbTooSmall { need: u32, have: u32 },
}

impl ResizeRefusal {
    pub const fn name(&self) -> &'static str {
        match self {
            ResizeRefusal::SizeInvalid { .. } => "RESIZE-SIZE-INVALID",
            ResizeRefusal::NotInAllowList { .. } => "RESIZE-PLANE-NOT-IN-ALLOW-LIST",
            ResizeRefusal::Downscale { .. } => "RESIZE-DOWNSCALE-NOT-FASTSET",
            ResizeRefusal::EdpMuxNeedsModeset => "RESIZE-EDP-MUX-NEEDS-MODESET",
            ResizeRefusal::NoScalerFree => "RESIZE-NO-SCALER-FREE",
            ResizeRefusal::TilingNotPorted => "RESIZE-TILING-NOT-PORTED",
            ResizeRefusal::PlaneOutOfRange { .. } => "RESIZE-PLANE-OUT-OF-RANGE",
            ResizeRefusal::Scaler(e) => e.name(),
            ResizeRefusal::NoDdb => "RESIZE-PLANE-NO-DDB",
            ResizeRefusal::DdbTooSmall { .. } => "ddb-too-small",
        }
    }
}

/// One decoded PLANE_WM word (skl_universal_plane_regs.h:438-442).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WmWord {
    pub enable: bool,
    pub ignore_lines: bool,
    pub lines: u32,
    pub blocks: u32,
}

impl WmWord {
    pub const fn decode(v: u32) -> WmWord {
        WmWord { enable: v & PLANE_WM_EN != 0, ignore_lines: v & PLANE_WM_IGNORE_LINES != 0, lines: (v & PLANE_WM_LINES_MASK) >> PLANE_WM_LINES_SHIFT, blocks: v & PLANE_WM_BLOCKS_MASK }
    }
    pub const fn encode(&self) -> u32 {
        if !self.enable {
            return 0; // skl_check_wm_level's memset: a disabled level is an all-zero word
        }
        PLANE_WM_EN | if self.ignore_lines { PLANE_WM_IGNORE_LINES } else { 0 } | ((self.lines << PLANE_WM_LINES_SHIFT) & PLANE_WM_LINES_MASK) | (self.blocks & PLANE_WM_BLOCKS_MASK)
    }
    /// `result->min_ddb_alloc` for display ver < 11 (skl_watermark.c:1906, :1928): `max(blocks + DIV_ROUND_UP(blocks, 10), blocks) + 1`.
    pub const fn min_ddb_alloc(&self) -> u32 {
        self.blocks + self.blocks.div_ceil(10) + 1
    }
}

/// The DDB entry PLANE_BUF_CFG / CUR_BUF_CFG holds as `(start, end)` half-open (`skl_ddb_entry_init_from_hw`: the register stores
/// `end - 1`; 0 = no allocation).
pub const fn ddb_entry(buf_cfg: u32) -> (u32, u32) {
    if buf_cfg == 0 {
        return (0, 0);
    }
    (buf_cfg & PLANE_BUF_START_MASK, ((buf_cfg & PLANE_BUF_END_MASK) >> PLANE_BUF_END_SHIFT) + 1)
}

/// The DDB entry PLANE_BUF_CFG holds, as a size in blocks.
pub const fn ddb_size(buf_cfg: u32) -> u32 {
    let (start, end) = ddb_entry(buf_cfg);
    end.saturating_sub(start)
}

/// `skl_cursor_allocation`'s floor with one active pipe (skl_watermark.c:667).
pub const CURSOR_DDB_ONE_PIPE: u32 = 32;

/// `MCURSOR_MODE_MASK` (intel_cursor_regs.h:33): any of these set in CURCNTR is a visible cursor (`i9xx_cursor_get_hw_state`).
pub const MCURSOR_MODE_MASK: u32 = 0x27;

/// How the plane's (and the cursor's) DDB entries were chosen, and every input the choice read - printed on the plan and refusal lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdbChoice {
    pub how: &'static str,
    /// `dbuf.size` of the platform, in blocks.
    pub dbuf: u32,
    /// The entries after the resize, half-open.
    pub plane: (u32, u32),
    pub cursor: (u32, u32),
    /// The entries the firmware left.
    pub fw_plane: (u32, u32),
    pub fw_cursor: (u32, u32),
    pub other_pipes_active: bool,
    pub cursor_visible: bool,
}

impl DdbChoice {
    /// Does the cursor's entry change (its block is then written with the fastset)?
    pub fn moves_cursor(&self) -> bool {
        self.cursor != self.fw_cursor
    }
}

impl core::fmt::Display for DdbChoice {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ddb_how={} dbuf={} plane={}..{} cursor={}..{} fw_plane={}..{} fw_cursor={}..{} other_pipes={} cursor_visible={}",
            self.how, self.dbuf, self.plane.0, self.plane.1, self.cursor.0, self.cursor.1, self.fw_plane.0, self.fw_plane.1,
            self.fw_cursor.0, self.fw_cursor.1, self.other_pipes_active as u8, self.cursor_visible as u8
        )
    }
}

/// The plane's DDB entry as Linux allocates it for the single plane of the single active pipe (module doc), or the firmware's own
/// when that allocation is not one this port computes - with how, and from what.
pub fn plane_ddb(caps: &DisplayCaps, fw: &FirmwareGeometry) -> DdbChoice {
    let dbuf = caps.dbuf_blocks as u32;
    let (fw_plane, fw_cursor) = (ddb_entry(fw.buf_cfg), ddb_entry(fw.cur_buf_cfg));
    let cursor_visible = fw.curcntr & MCURSOR_MODE_MASK != 0;
    let mut c = DdbChoice { how: "", dbuf, plane: fw_plane, cursor: fw_cursor, fw_plane, fw_cursor, other_pipes_active: fw.other_pipes_active, cursor_visible };
    let at_top = fw_cursor.1 == dbuf && fw_cursor.0 < fw_cursor.1;
    let size = fw_cursor.1.saturating_sub(fw_cursor.0);
    let cursor = if fw.other_pipes_active {
        c.how = "kept-other-pipes-active";
        return c;
    } else if at_top && (cursor_visible || size >= CURSOR_DDB_ONE_PIPE) {
        size
    } else if cursor_visible {
        c.how = "kept-cursor-visible-not-at-top";
        return c;
    } else {
        CURSOR_DDB_ONE_PIPE
    };
    if cursor >= dbuf {
        c.how = "kept-cursor-fills-dbuf";
        return c;
    }
    c.how = "linux-single-plane";
    c.plane = (0, dbuf - cursor);
    c.cursor = (dbuf - cursor, dbuf);
    c
}

/// The watermark bound for a plane `new_w` wide from the firmware's level for `old_w` (see the module doc): blocks scaled up by the
/// width ratio, rounded up, plus one block per line and one more; lines and ignore-lines kept; never below the level beneath (`max(blocks, result_prev->blocks)`).
/// A narrower or equal plane keeps the firmware's level (every term is non-decreasing in the width).
pub fn rescale_wm(fw: &[u32; NUM_LEVELS], old_w: u32, new_w: u32) -> [WmWord; NUM_LEVELS] {
    let mut out = [WmWord::decode(0); NUM_LEVELS];
    let mut prev = 0u32;
    for (level, v) in fw.iter().enumerate() {
        let mut l = WmWord::decode(*v);
        if l.enable && new_w > old_w && old_w != 0 {
            // + one block per line for the per-line round-up of `plane_blocks_per_line` (X tiling rounds each line up to whole
            // blocks, which the width ratio does not scale), + one for the fixed-point round-up
            let scaled = (l.blocks as u64 * new_w as u64).div_ceil(old_w as u64) as u32 + l.lines.max(1) + 1;
            l.blocks = scaled.max(prev).min(PLANE_WM_BLOCKS_MASK);
        }
        if l.enable {
            prev = l.blocks;
        }
        out[level] = l;
    }
    out
}

/// The planned resize.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanoutResize {
    pub pipe: Pipe,
    pub plane_id: u8,
    pub logical_w: u32,
    pub logical_h: u32,
    /// The new framebuffer's pitch in bytes and its size (pitch x aligned height), for the scanout images.
    pub stride: u32,
    pub fb_size: u64,
    pub panel_w: u32,
    pub panel_h: u32,
    pub scaler_id: u8,
    pub window: Window,
    /// 16.16 scale factors source / window.
    pub hscale: u32,
    pub vscale: u32,
    /// How many of the watermark levels stay enabled after the DDB check.
    pub wm_levels: u8,
    /// The plane's DDB after the resize, in blocks, how it was chosen (`plane_ddb`), and what the wider plane's level 0 needs of it.
    pub ddb_blocks: u32,
    pub ddb_how: &'static str,
    /// The DDB choice with its inputs, for the marker.
    pub ddb: DdbChoice,
    pub wm0_need: u32,
    /// The raster, for how long the health readback waits ([`ScanoutResize::health_after_ms`]).
    pub vtotal: u32,
    pub htotal: u32,
    pub dotclock_khz: Option<u32>,
    /// The register writes before PLANE_SURF, in Linux's order (pipe source, scaler, plane, watermarks).
    pub forward: Vec<(u32, u32)>,
    /// The firmware's values of the same registers, in the same order: what puts the firmware's geometry back.
    pub back: Vec<(u32, u32)>,
    pub surf_reg: u32,
    pub evade: Option<VblankEvade>,
}

impl ScanoutResize {
    /// When the post-resize health readback is due after the latch: [`HEALTH_FRAMES`] frames of the raster, rounded up, plus 1 ms
    /// ([`HEALTH_FALLBACK_MS`] without a dotclock).
    pub fn health_after_ms(&self) -> u64 {
        match self.dotclock_khz {
            Some(khz) if khz != 0 => (HEALTH_FRAMES as u64 * self.vtotal as u64 * self.htotal as u64).div_ceil(khz as u64) + 1,
            _ => HEALTH_FALLBACK_MS,
        }
    }
    /// Upscale ratio panel / logical, in thousandths (horizontal), for the marker.
    pub const fn ratio_milli(&self) -> u32 {
        (self.panel_w as u64 * 1000 / self.logical_w as u64) as u32
    }
    fn with_surf(writes: &[(u32, u32)], surf_reg: u32, surf: u32) -> Plan {
        let mut p: Plan = writes.iter().map(|&(r, v)| Effect::Write { reg: Reg::new(r), value: v }).collect();
        let reg = Reg::new(surf_reg);
        p.push(Effect::Write { reg, value: surf }); // PLANE_SURF last: it arms the plane (skl_plane_update_arm)
        p.push(Effect::PostingRead { reg });
        p
    }
    /// The forward plan for the first image at GGTT `surf`: everything, then PLANE_SURF, then a posting read of it.
    pub fn forward_plan(&self, surf: u32) -> Plan {
        Self::with_surf(&self.forward, self.surf_reg, surf)
    }
    /// The plan that puts the firmware's geometry and framebuffer back (`fw_surf_raw` = the raw PLANE_SURF it left).
    pub fn restore_plan(&self, fw_surf_raw: u32) -> Plan {
        Self::with_surf(&self.back, self.surf_reg, fw_surf_raw)
    }
    /// Every register either plan writes.
    pub fn written(&self) -> Vec<Reg> {
        written_regs(&self.forward_plan(0))
    }
}

/// How many frames after the latch the health readback reads the underrun status: an underrun the new watermarks cause shows within
/// the first frames of scanout at the new size.
pub const HEALTH_FRAMES: u32 = 3;
/// Three frames at 60 Hz, when the raster has no dotclock to compute them from.
pub const HEALTH_FALLBACK_MS: u64 = 51;

/// `skl_plane_max_height()` for display ver < 11 (skl_universal_plane.c): 4096.
const PLANE_MAX_HEIGHT: u32 = 4096;
/// `skl_plane_max_stride()`: 8192 pixels and 32 KiB (skl_universal_plane.c:496-503).
const PLANE_MAX_STRIDE_PIXELS: u32 = 8192;
const PLANE_MAX_STRIDE_BYTES: u32 = 32 * 1024;

/// Plan the adopted pipe's logical scanout size `w x h`. `Ok(None)`: it is already that size (nothing to do).
pub fn plan_scanout_resize(caps: &DisplayCaps, a: &AdoptedDisplay, fw: &FirmwareGeometry, w: u32, h: u32) -> Result<Option<ScanoutResize>, ResizeRefusal> {
    if w == 0 || h == 0 {
        return Err(ResizeRefusal::SizeInvalid { w, h });
    }
    if (w, h) == (a.fb.width, a.fb.height) {
        return Ok(None);
    }
    if a.pipe != Pipe::A || a.fb.plane_id != 0 {
        return Err(ResizeRefusal::NotInAllowList { pipe: a.pipe, plane_id: a.fb.plane_id });
    }
    let (panel_w, panel_h) = (a.active_w, a.active_h);
    if w > panel_w || h > panel_h {
        return Err(ResizeRefusal::Downscale { w, h, panel_w, panel_h });
    }
    if !fw.fitter_on {
        if fw.scalers_on_pipe == 0 {
            return Err(ResizeRefusal::NoScalerFree);
        }
        if fw.transcoder == Transcoder::Edp {
            return Err(ResizeRefusal::EdpMuxNeedsModeset);
        }
    }
    // the plane: stride rule of the tiling, then skl_plane_max_width / height / stride
    let tiling = match a.fb.tiling {
        Tiling::Linear => WriteTiling::Linear,
        Tiling::X => WriteTiling::X,
        Tiling::Y => WriteTiling::Y,
        Tiling::Yf => return Err(ResizeRefusal::TilingNotPorted),
    };
    let mult = tiling.stride_mult();
    let stride = (w * a.fb.cpp).div_ceil(mult) * mult;
    if w > tiling.max_width() || h > PLANE_MAX_HEIGHT || w > PLANE_MAX_STRIDE_PIXELS || stride > PLANE_MAX_STRIDE_BYTES || stride / mult > PLANE_STRIDE_MASK {
        return Err(ResizeRefusal::PlaneOutOfRange { w, h, stride });
    }
    let aligned_h = crate::initial::align_height(a.fb.tiling, a.fb.cpp, h).ok_or(ResizeRefusal::TilingNotPorted)?;
    let fb_size = stride as u64 * aligned_h as u64;
    // the scaler: the fitter the firmware left (its mode kept), or scaler 0 - HQ when the pipe has two (display ver 9), else DYN
    let mode = if fw.fitter_on {
        fw.ps[0] & SKL_PS_SCALER_MODE_MASK
    } else if caps.display_ver == 9 && fw.scalers_on_pipe > 1 {
        SKL_PS_SCALER_MODE_HQ
    } else {
        SKL_PS_SCALER_MODE_DYN
    };
    let window = aspect_window(w, h, panel_w, panel_h);
    let pf = pfit_enable_writes(caps.display_ver, a.pipe, fw.scaler_id as u32, mode, w, h, window).map_err(ResizeRefusal::Scaler)?;
    // the DDB (Linux's single-plane allocation, or the firmware's), then the watermarks: the bound, then skl_check_wm_level against it
    let ddb_choice = plane_ddb(caps, fw);
    let ((ddb_start, ddb_end), ddb_how) = (ddb_choice.plane, ddb_choice.how);
    let ddb = ddb_end.saturating_sub(ddb_start);
    if ddb == 0 {
        return Err(ResizeRefusal::NoDdb);
    }
    let mut levels = rescale_wm(&fw.wm, a.fb.width, w);
    let wm0_need = levels[0].min_ddb_alloc();
    let mut kept = 0u8;
    let mut cut = false;
    for (i, l) in levels.iter_mut().enumerate() {
        if !l.enable {
            continue;
        }
        if cut || l.min_ddb_alloc() > ddb {
            if i == 0 {
                return Err(ResizeRefusal::DdbTooSmall { need: l.min_ddb_alloc(), have: ddb });
            }
            cut = true; // the levels above a level that does not fit are not possible either (skl_allocate_plane_ddb's level search)
            l.enable = false;
        } else {
            kept += 1;
        }
    }
    // skl_compute_transition_wm: display ver 9 never enables it (WaDisableTWM); later versions keep the firmware's, bounded the same way
    let trans = if caps.display_ver == 9 {
        0
    } else {
        let t = rescale_wm(&[fw.wm_trans, 0, 0, 0, 0, 0, 0, 0], a.fb.width, w)[0];
        if t.enable && t.min_ddb_alloc() <= ddb { t.encode() } else { 0 }
    };
    let pipe = a.pipe;
    let pl = |base| plane_reg(base, pipe, a.fb.plane_id as u32);
    let src_reg = pipesrc(Transcoder::of_pipe(pipe));
    let mut forward = Vec::new();
    let mut back = Vec::new();
    // intel_set_pipe_src_size: PIPESRC_WIDTH(w - 1) | PIPESRC_HEIGHT(h - 1)
    forward.push((src_reg, ((w - 1) << 16) | (h - 1)));
    back.push((src_reg, fw.pipesrc));
    let fw_ps = if fw.fitter_on { fw.ps } else { [0; 5] }; // skl_detach_scaler: CTRL, WIN_POS, WIN_SZ = 0 (and the phases with them)
    for (k, &(r, v)) in pf.writes.iter().enumerate() {
        forward.push((r, v));
        back.push((r, fw_ps[k]));
    }
    // hsw_set_linetime_wm: skl_linetime_wm from the pipe pixel rate, which an upscale leaves at the dotclock (no dotclock: kept)
    if let Some(khz) = fw.dotclock_khz {
        if let Ok(lt) = linetime_wm(fw.htotal, khz) {
            forward.push((wm_linetime(pipe), linetime_reg_value(lt)));
            back.push((wm_linetime(pipe), fw.linetime));
        }
    }
    // skl_plane_update_noarm: STRIDE, POS, SIZE, then skl_write_plane_wm
    forward.push((pl(PLANE_STRIDE_1_A), (stride / mult) & PLANE_STRIDE_MASK));
    back.push((pl(PLANE_STRIDE_1_A), fw.plane_stride));
    forward.push((pl(PLANE_POS_1_A), 0));
    back.push((pl(PLANE_POS_1_A), fw.plane_pos));
    forward.push((pl(PLANE_SIZE_1_A), ((h - 1) << 16) | (w - 1)));
    back.push((pl(PLANE_SIZE_1_A), fw.plane_size));
    for (level, l) in levels.iter().enumerate() {
        let r = plane_wm(pipe, a.fb.plane_id as u32, level as u32);
        forward.push((r, l.encode()));
        back.push((r, fw.wm[level]));
    }
    forward.push((pl(PLANE_WM_TRANS_1_A), trans));
    back.push((pl(PLANE_WM_TRANS_1_A), fw.wm_trans));
    // skl_write_plane_wm: PLANE_BUF_CFG = the plane's DDB (end - 1 high), PLANE_NV12_BUF_CFG = no Y plane on display ver < 11
    forward.push((pl(PLANE_BUF_CFG_1_A), ((((ddb_end - 1) << PLANE_BUF_END_SHIFT) & PLANE_BUF_END_MASK) | (ddb_start & PLANE_BUF_START_MASK))));
    back.push((pl(PLANE_BUF_CFG_1_A), fw.buf_cfg));
    forward.push((pl(PLANE_NV12_BUF_CFG_1_A), 0));
    back.push((pl(PLANE_NV12_BUF_CFG_1_A), fw.nv12_buf_cfg));
    // the hidden cursor's block when its entry moves: skl_write_cursor_wm, CURPOS, CURBASE (arms) - i9xx_cursor_update_arm(NULL)
    if ddb_choice.moves_cursor() {
        let cur = Ddb { start: ddb_choice.cursor.0 as u16, end: ddb_choice.cursor.1 as u16 };
        for e in cursor_hidden_writes(pipe, &cur) {
            if let Effect::Write { reg, value } = e {
                let r = reg.offset();
                let fw_v = match r {
                    _ if r == cur_buf_cfg(pipe) => fw.cur_buf_cfg,
                    _ if r == cur_wm_trans(pipe) => fw.cur_wm_trans,
                    _ if r == curpos(pipe) => fw.curpos,
                    _ if r == curbase(pipe) => fw.curbase,
                    _ => (0..NUM_LEVELS).find(|&l| cur_wm(pipe, l as u32) == r).map_or(0, |l| fw.cur_wm[l]),
                };
                forward.push((r, value));
                back.push((r, fw_v));
            }
        }
    }
    let evade = VblankEvade::new(fw.vblank_start, fw.vtotal, fw.htotal, fw.dotclock_khz, scanline_offset(caps.display_ver, fw.hdmi), EVADE_US);
    Ok(Some(ScanoutResize {
        pipe,
        plane_id: a.fb.plane_id,
        logical_w: w,
        logical_h: h,
        stride,
        fb_size,
        panel_w,
        panel_h,
        scaler_id: fw.scaler_id,
        window,
        hscale: pf.hscale,
        vscale: pf.vscale,
        wm_levels: kept,
        ddb_blocks: ddb,
        ddb_how,
        ddb: ddb_choice,
        wm0_need,
        vtotal: fw.vtotal,
        htotal: fw.htotal,
        dotclock_khz: fw.dotclock_khz,
        forward,
        back,
        surf_reg: pl(PLANE_SURF_1_A),
        evade,
    }))
}

/// `intel_crtc_scanline_offset()` (intel_vblank.c:199-238): what PIPEDSL lags the line by - 1, or 2 on HDMI from display ver 9 up to 19.
pub const fn scanline_offset(display_ver: u8, hdmi: bool) -> u32 {
    if display_ver >= 9 && display_ver < 20 && hdmi {
        2
    } else {
        1
    }
}

/// `intel_vblank_evade_init()` + `intel_vblank_evade()` (intel_vblank.c:682-760) as a judge over PIPEDSL reads: write only while the
/// scanline is outside `[vblank_start - usecs_to_scanlines(evade_us), vblank_start - 1]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VblankEvade {
    pub min: u32,
    pub max: u32,
    pub vtotal: u32,
    pub offset: u32,
}

impl VblankEvade {
    /// `None` when there is nothing to evade by (no dotclock, no timing, or `min <= 0`: Linux returns at once).
    pub fn new(vblank_start: u32, vtotal: u32, htotal: u32, dotclock_khz: Option<u32>, offset: u32, evade_us: u32) -> Option<VblankEvade> {
        let clock = dotclock_khz?;
        if htotal == 0 || vtotal == 0 {
            return None;
        }
        // intel_usecs_to_scanlines: DIV_ROUND_UP(usecs * crtc_clock, 1000 * crtc_htotal)
        let lines = (evade_us as u64 * clock as u64).div_ceil(1000 * htotal as u64) as u32;
        if vblank_start <= lines || vblank_start <= 1 {
            return None;
        }
        Some(VblankEvade { min: vblank_start - lines, max: vblank_start - 1, vtotal, offset })
    }
    /// `__intel_get_crtc_scanline`: `(PIPEDSL & PIPEDSL_LINE_MASK + vtotal + scanline_offset) % vtotal`.
    pub const fn scanline(&self, dsl: u32) -> u32 {
        ((dsl & PIPEDSL_LINE_MASK) + self.vtotal + self.offset) % self.vtotal
    }
    /// May the burst be written now?
    pub const fn clear(&self, dsl: u32) -> bool {
        let s = self.scanline(dsl);
        s < self.min || s > self.max
    }
}
