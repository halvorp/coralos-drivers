// SPDX-License-Identifier: GPL-2.0-only
//! Skylake / Kaby Lake (display version 9) watermarks and display-buffer (DDB) allocation for ONE active pipe, as pure numbers.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_watermark.c at v7.0-rc6 (function line ranges are cited where each is
//! ported): `skl_read_wm_latency` / `adjust_wm_latency` / `skl_wm_latency` (:3278, :3229, :610), `skl_compute_wm_params` (:1657),
//! `skl_wm_method1/2` (:1614, :1633), `skl_compute_plane_wm` (:1793), `skl_compute_wm_levels` (:1937),
//! `skl_compute_transition_wm` (:1976), `skl_cursor_allocation` (:633), `skl_crtc_allocate_ddb` (:526),
//! `skl_crtc_allocate_plane_ddb` + `skl_allocate_plane_ddb` (:1421, :1394), `skl_check_wm_level` (:1353),
//! `skl_wm_check_vblank` / `skl_max_wm_level_for_vblank` (:2266, :2237) with skl_prefill.c's `prefill_*`, and the register encoders
//! `skl_plane_wm_reg_val` / `skl_plane_ddb_reg_val` (skl_universal_plane.c:779-820) and the cursor ones (intel_cursor.c:593-614).
//! Fixed-point helpers are intel_fixed.h. Copyright (c) Intel Corporation.
//!
//! DEVIATIONS FROM LINUX, each deliberate:
//!  * Linux's `clamp_u64_to_fixed16` / `u16` stores silently truncate (it only `WARN_ON`s); here an out-of-range intermediate is a
//!    named [`WmError::Overflow`], because a truncated watermark is an underrun on the glass, not a log line.
//!  * The memory latencies are INPUTS (the two PCU mailbox words), so the host can test the arithmetic; reading them is the
//!    mailbox plan in `i915-display-modeset-core::latency`. DRAM facts (`symmetric_memory`, `has_16gb_dimms`) are inputs too.
//!  * No SAGV (`display->sagv.block_time_us` is 0 on a part with no SAGV table), no IPC transition watermarks on ver 9
//!    (`skl_compute_transition_wm` returns early for `DISPLAY_VER == 9`, WaDisableTWM), no Xe3 auto min alloc, no FBC.
//!  * One active pipe: `skl_compute_dbuf_slices` gives slice S1 (:1246), the pipe's weight is its whole slice (:489, :526).
use crate::regs_write::*;

/// `display->wm.num_levels` for a part without HW SAGV watermarks (`HAS_HW_SAGV_WM` is display ver >= 12).
pub const NUM_LEVELS: usize = 8;
/// `INTEL_INFO->display.dbuf.size` of `skl_display` — intel_display_device.c:662 (`896 - 4`, "4 blocks for bypass path allocation").
pub const DBUF_SIZE_BLOCKS: u16 = 896 - 4;
/// `mode_config->cursor_width` for everything newer than i915g.
pub const CURSOR_WIDTH: u32 = 256;
/// `skl_wm_max_lines()` for display ver < 13.
pub const WM_MAX_LINES: u32 = 31;
/// `crtc_state->framestart_delay` for a non-VRR hsw+ pipe.
pub const FRAMESTART_DELAY: u32 = 1;

/// Named refusals. `name()` strings are stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WmError {
    /// A 16.16 fixed-point or `u16` value left its range (Linux truncates; see the module doc).
    Overflow { what: &'static str },
    /// `DDB`: even the lowest watermark level does not fit ("Requested display configuration exceeds system DDB limitations",
    /// skl_crtc_allocate_plane_ddb).
    DdbExceeded { needed: u32, available: u32 },
    /// A `u32` divisor was zero (a mode with a zero `htotal` / `clock`, a zero pixel rate).
    ZeroDivisor { what: &'static str },
    /// `skl_max_wm_level_for_vblank`: not even level 0 fits the vertical blank ("Not enough time in vblank for prefill",
    /// skl_max_wm_level_for_vblank).
    VblankTooShort,
}

impl WmError {
    pub const fn name(&self) -> &'static str {
        match self {
            WmError::Overflow { .. } => "WM-OVERFLOW",
            WmError::DdbExceeded { .. } => "WM-DDB-EXCEEDED",
            WmError::ZeroDivisor { .. } => "WM-ZERO-DIVISOR",
            WmError::VblankTooShort => "WM-VBLANK-TOO-SHORT",
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// intel_fixed.h: uint_fixed_16_16_t
// ---------------------------------------------------------------------------------------------------------------
/// `uint_fixed_16_16_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Fx(pub u32);

impl Fx {
    /// `FP_16_16_MAX`.
    pub const MAX: Fx = Fx(u32::MAX);
    fn clamp(v: u64, what: &'static str) -> Result<Fx, WmError> {
        u32::try_from(v).map(Fx).map_err(|_| WmError::Overflow { what })
    }
    /// `u32_to_fixed16()`.
    pub fn from_u32(v: u32) -> Result<Fx, WmError> {
        if v > 0xffff {
            return Err(WmError::Overflow { what: "u32_to_fixed16" });
        }
        Ok(Fx(v << 16))
    }
    /// `fixed16_to_u32_round_up()`.
    pub const fn round_up(self) -> u32 {
        self.0.div_ceil(1 << 16)
    }
    /// `div_fixed16(val, d)`.
    pub fn div(val: u32, d: u32) -> Result<Fx, WmError> {
        if d == 0 {
            return Err(WmError::ZeroDivisor { what: "div_fixed16" });
        }
        Fx::clamp(((val as u64) << 16).div_ceil(d as u64), "div_fixed16")
    }
    /// `mul_u32_fixed16()`.
    pub fn mul_u32(val: u32, mul: Fx) -> Result<Fx, WmError> {
        Fx::clamp(val as u64 * mul.0 as u64, "mul_u32_fixed16")
    }
    /// `add_fixed16_u32()`.
    pub fn add_u32(self, add: u32) -> Result<Fx, WmError> {
        Fx::clamp(self.0 as u64 + Fx::from_u32(add)?.0 as u64, "add_fixed16_u32")
    }
    /// `div_round_up_fixed16(val, d)` (a plain integer ratio of two fixed values).
    pub fn ratio_round_up(self, d: Fx) -> Result<u32, WmError> {
        if d.0 == 0 {
            return Err(WmError::ZeroDivisor { what: "div_round_up_fixed16" });
        }
        Ok(self.0.div_ceil(d.0))
    }
    /// `mul_round_up_u32_fixed16()`.
    pub fn mul_round_up(val: u32, mul: Fx) -> Result<u32, WmError> {
        u32::try_from((val as u64 * mul.0 as u64).div_ceil(1 << 16)).map_err(|_| WmError::Overflow { what: "mul_round_up_u32_fixed16" })
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Latencies
// ---------------------------------------------------------------------------------------------------------------
/// What `intel_dram_info()` tells the watermark code (intel_dram.c): only two facts are consulted, both for Kaby Lake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DramFacts {
    /// `dram_info->symmetric_memory` — gates IPC (`skl_watermark_ipc_can_enable`, skl_watermark.c, Display WA #1141).
    pub symmetric_memory: bool,
    /// `dram_info->has_16gb_dimms` — `need_16gb_dimm_wa`.
    pub has_16gb_dimms: bool,
}

impl DramFacts {
    /// "If we could not get dimm info enable this WA to prevent from any underrun. If not able to get DIMM info assume 16Gb+ DIMM
    /// to avoid any underrun." — and unknown memory is not known-symmetric, so IPC stays off.
    pub const UNKNOWN: DramFacts = DramFacts { symmetric_memory: false, has_16gb_dimms: true };
    /// `skl_watermark_ipc_can_enable()` for Kaby Lake.
    pub const fn ipc_enabled(&self) -> bool {
        self.symmetric_memory
    }
}

/// The memory latency of each watermark level in microseconds (`display->wm.skl_latency`).
pub type Latencies = [u16; NUM_LEVELS];

const fn lane(v: u32, mask: u32) -> u16 {
    ((v & mask) >> mask.trailing_zeros()) as u16
}

/// `skl_read_wm_latency()` decode: the two mailbox replies (data0 = 0 for levels 0-3, data0 = 1 for levels 4-7).
pub const fn latencies_from_pcode(first: u32, second: u32) -> Latencies {
    [
        lane(first, GEN9_MEM_LATENCY_LEVEL_0_4_MASK),
        lane(first, GEN9_MEM_LATENCY_LEVEL_1_5_MASK),
        lane(first, GEN9_MEM_LATENCY_LEVEL_2_6_MASK),
        lane(first, GEN9_MEM_LATENCY_LEVEL_3_7_MASK),
        lane(second, GEN9_MEM_LATENCY_LEVEL_0_4_MASK),
        lane(second, GEN9_MEM_LATENCY_LEVEL_1_5_MASK),
        lane(second, GEN9_MEM_LATENCY_LEVEL_2_6_MASK),
        lane(second, GEN9_MEM_LATENCY_LEVEL_3_7_MASK),
    ]
}

/// `increase_wm_latency()`: always level 0, then each following level until the first zero.
fn increase_wm_latency(wm: &mut Latencies, inc: u16) {
    wm[0] += inc;
    for l in wm.iter_mut().skip(1) {
        if *l == 0 {
            break;
        }
        *l += inc;
    }
}

/// `adjust_wm_latency()` for Kaby Lake: `sanitize_wm_latency`, `make_wm_latency_monotonic`, WaWmMemoryReadLatency (+2 when level 0
/// reports 0, `wm_read_latency`), and the 16Gb DIMM WA (+1).
pub fn adjust_latencies(wm: &mut Latencies, dram: &DramFacts) {
    // sanitize: once a level n > 0 reports 0 us, every level after it is disabled.
    let mut level = 1;
    while level < NUM_LEVELS {
        if wm[level] == 0 {
            break;
        }
        level += 1;
    }
    for l in wm.iter_mut().skip(level + 1) {
        *l = 0;
    }
    // monotonic
    for level in 1..NUM_LEVELS {
        if wm[level] == 0 {
            break;
        }
        wm[level] = wm[level].max(wm[level - 1]);
    }
    if wm[0] == 0 {
        increase_wm_latency(wm, 2);
    }
    if dram.has_16gb_dimms {
        // `need_16gb_dimm_wa`: skylake || kabylake || coffeelake || cometlake || ver 11, && has_16gb_dimms.
        increase_wm_latency(wm, 1);
    }
}

/// `skl_wm_latency()`. `x_tiled` is `wp->x_tiled`; pass `false` where Linux passes `wp == NULL`.
pub fn wm_latency(wm: &Latencies, level: usize, ipc_enabled: bool, x_tiled: bool) -> u32 {
    let mut latency = wm[level] as u32;
    if latency == 0 {
        return 0;
    }
    // WaIncreaseLatencyIPCEnabled: kbl,cfl / Display WA #1141.
    if ipc_enabled {
        latency += 4;
    }
    // skl_needs_memory_bw_wa(): DISPLAY_VER == 9; applies when the plane is X tiled.
    if x_tiled {
        latency += 15;
    }
    latency
}

// ---------------------------------------------------------------------------------------------------------------
// Per-plane watermark computation
// ---------------------------------------------------------------------------------------------------------------
/// `struct skl_wm_level` (skl_watermark.h) as far as display ver 9 uses it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WmLevel {
    pub enable: bool,
    pub ignore_lines: bool,
    pub blocks: u16,
    pub lines: u16,
    pub min_ddb_alloc: u16,
}

impl WmLevel {
    /// "reject it": `result->min_ddb_alloc = U16_MAX` and nothing else is written.
    pub fn rejected() -> WmLevel {
        // `result` is a zeroed element of `levels[]`; only `min_ddb_alloc` changes.
        WmLevel { min_ddb_alloc: u16::MAX, ..WmLevel::default() }
    }
}

/// Which plane geometry the watermarks are computed for (`struct skl_wm_params`' inputs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneWmGeometry {
    /// Source width in pixels of THIS colour plane's luma-equivalent (`drm_rect_width(&uapi.src) >> 16`).
    pub width: u32,
    /// `fb->format->cpp[color_plane]`.
    pub cpp: u32,
    pub x_tiled: bool,
    pub y_tiled: bool,
    /// `intel_format_info_is_yuv_semiplanar()`.
    pub is_planar: bool,
    /// 0 = Y / packed, 1 = UV.
    pub color_plane: u8,
}

/// `struct skl_wm_params`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WmParams {
    pub x_tiled: bool,
    pub y_tiled: bool,
    pub is_planar: bool,
    pub width: u32,
    pub cpp: u32,
    pub plane_pixel_rate: u32,
    pub dbuf_block_size: u32,
    pub y_min_scanlines: u32,
    pub plane_bytes_per_line: u32,
    pub plane_blocks_per_line: Fx,
    pub y_tile_minimum: Fx,
    pub linetime_us: u32,
}

/// The crtc timing the watermark code reads (`pipe_mode->crtc_*`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WmTiming {
    pub htotal: u32,
    /// `crtc_clock`, kHz.
    pub clock_khz: u32,
    /// `crtc_state->pixel_rate`, kHz (the pipe pixel rate; equals `clock_khz` with no panel fitter, `ilk_pipe_pixel_rate`).
    pub pixel_rate_khz: u32,
    /// `intel_crtc_vblank_length()` = `crtc_vtotal - crtc_vblank_start` lines.
    pub vblank_lines: u32,
}

/// `skl_compute_wm_params()` for rotation 0, no render compression.
pub fn compute_wm_params(t: &WmTiming, g: &PlaneWmGeometry) -> Result<WmParams, WmError> {
    if t.pixel_rate_khz == 0 {
        return Err(WmError::ZeroDivisor { what: "pixel_rate" });
    }
    let mut width = g.width;
    if g.color_plane == 1 && g.is_planar {
        width /= 2; 
    }
    let dbuf_block_size = 512; // (the Yf/cpp 1 exception is display ver >= 11)
    let mut y_min_scanlines = 4; // (not 90/270)
    y_min_scanlines *= 2; // skl_needs_memory_bw_wa(): ver 9
    let plane_bytes_per_line = width * g.cpp;
    let plane_blocks_per_line;
    if g.y_tiled {
        let i = (plane_bytes_per_line * y_min_scanlines).div_ceil(dbuf_block_size); 
        // `DISPLAY_VER >= 10` / `>= 30` additions do not apply.
        plane_blocks_per_line = Fx::div(i, y_min_scanlines)?;
    } else {
        let mut i = plane_bytes_per_line.div_ceil(dbuf_block_size); 
        if !g.x_tiled {
            i += 1; // `!wp->x_tiled || DISPLAY_VER >= 10`
        }
        plane_blocks_per_line = Fx::from_u32(i)?;
    }
    let y_tile_minimum = Fx::mul_u32(y_min_scanlines, plane_blocks_per_line)?;
    // skl_wm_linetime_us(): DIV_ROUND_UP(crtc_htotal * 1000, pixel_rate)
    let linetime_us = (t.htotal * 1000).div_ceil(t.pixel_rate_khz);
    Ok(WmParams {
        x_tiled: g.x_tiled,
        y_tiled: g.y_tiled,
        is_planar: g.is_planar,
        width,
        cpp: g.cpp,
        plane_pixel_rate: t.pixel_rate_khz,
        dbuf_block_size,
        y_min_scanlines,
        plane_bytes_per_line,
        plane_blocks_per_line,
        y_tile_minimum,
        linetime_us,
    })
}

/// `skl_wm_method1()` (the `DISPLAY_VER >= 10` +1 does not apply).
fn method1(pixel_rate: u32, cpp: u32, latency: u32, block: u32) -> Result<Fx, WmError> {
    if latency == 0 {
        return Ok(Fx::MAX);
    }
    let v = (latency as u64) * (pixel_rate as u64) * (cpp as u64);
    let v = u32::try_from(v).map_err(|_| WmError::Overflow { what: "wm_method1" })?;
    Fx::div(v, 1000 * block)
}

/// `skl_wm_method2()`.
fn method2(pixel_rate: u32, htotal: u32, latency: u32, plane_blocks_per_line: Fx) -> Result<Fx, WmError> {
    if latency == 0 {
        return Ok(Fx::MAX);
    }
    if htotal == 0 {
        return Err(WmError::ZeroDivisor { what: "htotal" });
    }
    let v = u32::try_from((latency as u64) * (pixel_rate as u64)).map_err(|_| WmError::Overflow { what: "wm_method2" })?;
    let v = v.div_ceil(htotal * 1000);
    Fx::mul_u32(v, plane_blocks_per_line)
}

/// `skl_compute_plane_wm()` for display ver 9. `prev` is `result_prev` (the level below, or level 0
/// itself for level 0). Returns the level; a rejected one has `min_ddb_alloc == u16::MAX`.
pub fn compute_plane_wm(
    level: usize,
    latency: u32,
    wp: &WmParams,
    t: &WmTiming,
    prev: &WmLevel,
) -> Result<WmLevel, WmError> {
    if latency == 0 {
        return Ok(WmLevel::rejected()); 
    }
    let m1 = method1(wp.plane_pixel_rate, wp.cpp, latency, wp.dbuf_block_size)?;
    let m2 = method2(wp.plane_pixel_rate, t.htotal, latency, wp.plane_blocks_per_line)?;
    let selected = if wp.y_tiled {
        core::cmp::max(m2, wp.y_tile_minimum) 
    } else if wp.cpp * t.htotal / wp.dbuf_block_size < 1 && wp.plane_bytes_per_line / wp.dbuf_block_size < 1 {
        m2 
    } else if latency >= wp.linetime_us {
        core::cmp::min(m1, m2) // DISPLAY_VER == 9
    } else {
        m1
    };
    let mut blocks = selected.round_up() + 1; // DISPLAY_VER < 30
    // skl_wm_has_lines(): ver < 10 -> level > 0
    let has_lines = level > 0;
    if has_lines {
        blocks = blocks.max(wp.plane_blocks_per_line.round_up()); 
    }
    let mut lines = selected.ratio_round_up(wp.plane_blocks_per_line)?; 
    // DISPLAY_VER == 9: WA #1125 (rc_surface, not ported) and WA #1126
    if (1..=7).contains(&level) {
        if wp.y_tiled {
            blocks += wp.y_tile_minimum.round_up();
            lines += wp.y_min_scanlines;
        } else {
            blocks += 1;
        }
    }
    blocks = blocks.max(prev.blocks as u32); 
    lines = lines.max(prev.lines as u32);
    // `DISPLAY_VER >= 11` min_ddb_alloc block does not apply: min_ddb_alloc stays 0.
    if !has_lines {
        lines = 0; 
    }
    if lines > WM_MAX_LINES {
        return Ok(WmLevel::rejected()); 
    }
    let blocks16 = u16::try_from(blocks).map_err(|_| WmError::Overflow { what: "wm_blocks" })?;
    Ok(WmLevel {
        enable: true,
        ignore_lines: false,
        blocks: blocks16,
        lines: lines as u16,
        // "Bspec says: value >= plane ddb allocation -> invalid, hence the +1 here"
        min_ddb_alloc: blocks16.checked_add(1).ok_or(WmError::Overflow { what: "wm_min_ddb" })?,
    })
}

/// `skl_compute_wm_levels()`.
pub fn compute_wm_levels(
    lat: &Latencies,
    ipc_enabled: bool,
    wp: &WmParams,
    t: &WmTiming,
) -> Result<[WmLevel; NUM_LEVELS], WmError> {
    let mut levels = [WmLevel::default(); NUM_LEVELS];
    let mut prev = WmLevel::default();
    for level in 0..NUM_LEVELS {
        let latency = wm_latency(lat, level, ipc_enabled, wp.x_tiled);
        let r = compute_plane_wm(level, latency, wp, t, &prev)?;
        levels[level] = r;
        prev = r;
    }
    Ok(levels)
}

/// `skl_compute_transition_wm()`. For display version 9 it returns before touching the output
/// ("WaDisableTWM: skl,kbl,cfl,bxt — Transition WM are not recommended by HW team for GEN9"): the transition watermark
/// stays all-zero whatever IPC says.
pub const fn compute_transition_wm() -> WmLevel {
    WmLevel { enable: false, ignore_lines: false, blocks: 0, lines: 0, min_ddb_alloc: 0 }
}

/// One plane's raw watermarks: the Y/packed levels, the UV levels (NV12 only), and the transition level.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlaneWm {
    pub wm: [WmLevel; NUM_LEVELS],
    pub uv_wm: [WmLevel; NUM_LEVELS],
    pub trans_wm: WmLevel,
    pub is_planar: bool,
}

/// What one visible plane needs: its Y/packed geometry, optionally the UV geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisiblePlane {
    pub plane_id: u8,
    pub y: PlaneWmGeometry,
    pub uv: Option<PlaneWmGeometry>,
    /// `drm_rect_height(&uapi.src) >> 16` — for the relative data rate.
    pub src_h: u32,
}

/// `skl_build_plane_wm()` for a visible plane.
pub fn build_plane_wm(
    lat: &Latencies,
    ipc: bool,
    t: &WmTiming,
    p: &VisiblePlane,
) -> Result<PlaneWm, WmError> {
    let wp = compute_wm_params(t, &p.y)?;
    let wm = compute_wm_levels(lat, ipc, &wp, t)?;
    let mut out = PlaneWm { wm, uv_wm: [WmLevel::default(); NUM_LEVELS], trans_wm: compute_transition_wm(), is_planar: false };
    if let Some(uv) = &p.uv {
        out.is_planar = true;
        let wpuv = compute_wm_params(t, uv)?;
        out.uv_wm = compute_wm_levels(lat, ipc, &wpuv, t)?;
    }
    Ok(out)
}

/// `skl_cursor_allocation()`: the blocks reserved at the top of the pipe's DDB for a 256-wide ARGB8888
/// linear cursor at the best level that still computes, `max(num_active == 1 ? 32 : 8, min_ddb_alloc)`.
pub fn cursor_allocation(lat: &Latencies, ipc: bool, t: &WmTiming, num_active: u32) -> Result<u16, WmError> {
    let g = PlaneWmGeometry { width: CURSOR_WIDTH, cpp: 4, x_tiled: false, y_tiled: false, is_planar: false, color_plane: 0 };
    let wp = compute_wm_params(t, &g)?;
    let mut wm = WmLevel::default();
    let mut min_ddb_alloc = 0u16;
    for level in 0..NUM_LEVELS {
        let latency = wm_latency(lat, level, ipc, wp.x_tiled);
        wm = compute_plane_wm(level, latency, &wp, t, &wm)?;
        if wm.min_ddb_alloc == u16::MAX {
            break;
        }
        min_ddb_alloc = wm.min_ddb_alloc;
    }
    Ok(core::cmp::max(if num_active == 1 { 32 } else { 8 }, min_ddb_alloc))
}

// ---------------------------------------------------------------------------------------------------------------
// DDB
// ---------------------------------------------------------------------------------------------------------------
/// `struct skl_ddb_entry` — a half-open block range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ddb {
    pub start: u16,
    pub end: u16,
}

impl Ddb {
    pub const fn size(&self) -> u16 {
        self.end - self.start
    }
}

/// The whole result for one pipe: per-plane DDB / watermarks for planes 1..3 plus the cursor reserve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeWm {
    pub planes: [PlaneWm; 3],
    pub plane_ddb: [Ddb; 3],
    pub plane_ddb_y: [Ddb; 3],
    pub cursor_ddb: Ddb,
    /// The highest level for which every plane fits (`level` after the search loop).
    pub max_level: usize,
}

/// Input to the allocator: what each of planes 1..3 is (`None` = not visible, all-zero watermarks, no DDB).
#[derive(Clone, Copy, Debug)]
pub struct PipeWmInput<'a> {
    pub lat: &'a Latencies,
    pub ipc_enabled: bool,
    pub timing: WmTiming,
    pub planes: [Option<VisiblePlane>; 3],
    /// Total blocks this pipe may use (`alloc` = the pipe's slice share; with one active pipe, the whole `DBUF_SIZE_BLOCKS`).
    pub alloc: Ddb,
    pub num_active_pipes: u32,
}

fn rel_data_rate(p: &VisiblePlane, color_plane: u8) -> u64 {
    // intel_plane_relative_data_rate(): UV plane is subsampled 2x2; skl_plane_relative_data_rate
    // = width * height * cpp; 1:1 scaling so intel_adjusted_rate is the identity.
    let (g, mut w, mut h) = if color_plane == 1 {
        (p.uv.unwrap_or(p.y), p.y.width, p.src_h)
    } else {
        (p.y, p.y.width, p.src_h)
    };
    if color_plane == 1 {
        w /= 2;
        h /= 2;
    }
    w as u64 * h as u64 * g.cpp as u64
}

struct Iter {
    data_rate: u64,
    start: u16,
    size: u16,
}

/// `skl_allocate_plane_ddb()`.
fn allocate_plane_ddb(it: &mut Iter, ddb: &mut Ddb, wm: &WmLevel, data_rate: u64) {
    let mut extra: u16 = 0;
    if data_rate != 0 && it.data_rate != 0 {
        // DIV64_U64_ROUND_UP(iter->size * data_rate, iter->data_rate), min'd with the remaining size as u16.
        let want = (it.size as u64 * data_rate).div_ceil(it.data_rate);
        extra = core::cmp::min(it.size as u64, want) as u16;
        it.size -= extra;
        it.data_rate -= data_rate;
    }
    // Keep ddb entry of all disabled planes explicitly zeroed.
    let size = wm.min_ddb_alloc.wrapping_add(extra);
    if size != 0 {
        *ddb = Ddb { start: it.start, end: it.start + size };
        it.start = ddb.end;
    }
}

/// `skl_check_wm_level()`.
fn check_wm_level(wm: &mut WmLevel, ddb: &Ddb) {
    if wm.min_ddb_alloc > ddb.size() {
        *wm = WmLevel::default();
    }
}

/// `skl_crtc_allocate_plane_ddb()` for display ver 9, with the planes' watermarks built first
/// (`skl_build_pipe_wm`) and the cursor treated as not visible (zero watermarks).
pub fn compute_pipe_wm(inp: &PipeWmInput) -> Result<PipeWm, WmError> {
    let mut planes = [PlaneWm::default(); 3];
    for (i, p) in inp.planes.iter().enumerate() {
        if let Some(p) = p {
            planes[i] = build_plane_wm(inp.lat, inp.ipc_enabled, &inp.timing, p)?;
        }
    }
    let mut plane_ddb = [Ddb::default(); 3];
    let mut plane_ddb_y = [Ddb::default(); 3];
    let alloc = inp.alloc;
    let mut it = Iter { data_rate: 0, start: alloc.start, size: alloc.size() };
    if it.size == 0 {
        return Ok(PipeWm { planes, plane_ddb, plane_ddb_y, cursor_ddb: Ddb::default(), max_level: 0 });
    }

    // Allocate fixed number of blocks for cursor.
    let cursor_size = cursor_allocation(inp.lat, inp.ipc_enabled, &inp.timing, inp.num_active_pipes)?;
    if cursor_size > it.size {
        return Err(WmError::DdbExceeded { needed: cursor_size as u32, available: it.size as u32 });
    }
    it.size -= cursor_size;
    let cursor_ddb = Ddb { start: alloc.end - cursor_size, end: alloc.end };

    // skl_total_relative_data_rate(): sum of rel_data_rate and (ver < 11) rel_data_rate_y.
    let mut rel = [0u64; 3];
    let mut rel_y = [0u64; 3];
    for (i, p) in inp.planes.iter().enumerate() {
        if let Some(p) = p {
            if p.uv.is_some() {
                rel_y[i] = rel_data_rate(p, 0);
                rel[i] = rel_data_rate(p, 1);
            } else {
                rel[i] = rel_data_rate(p, 0);
            }
            it.data_rate += rel[i] + rel_y[i];
        }
    }

    // Find the highest watermark level for which we can satisfy the block requirement of active planes.
    let mut level = NUM_LEVELS as i32 - 1;
    let mut blocks;
    loop {
        if level < 0 {
            break;
        }
        blocks = 0u32;
        for pl in planes.iter() {
            blocks += pl.wm[level as usize].min_ddb_alloc as u32;
            blocks += pl.uv_wm[level as usize].min_ddb_alloc as u32;
        }
        if blocks <= it.size as u32 {
            it.size -= blocks as u16;
            break;
        }
        level -= 1;
    }
    if level < 0 {
        return Err(WmError::DdbExceeded { needed: planes.iter().map(|p| p.wm[0].min_ddb_alloc as u32 + p.uv_wm[0].min_ddb_alloc as u32).sum(), available: it.size as u32 });
    }
    let level = level as usize;
    // "avoid the WARN later when we don't allocate any extra DDB"
    if it.data_rate == 0 {
        it.size = 0;
    }

    // Grant each plane the blocks it requires plus a proportional share of the leftovers.
    for i in 0..3 {
        let wm = planes[i];
        if inp.planes[i].is_none() {
            continue;
        }
        if wm.is_planar {
            allocate_plane_ddb(&mut it, &mut plane_ddb_y[i], &wm.wm[level], rel_y[i]);
            allocate_plane_ddb(&mut it, &mut plane_ddb[i], &wm.uv_wm[level], rel[i]);
        } else {
            allocate_plane_ddb(&mut it, &mut plane_ddb[i], &wm.wm[level], rel[i]);
        }
    }

    // Disable the levels above the one we reached, then apply the copy WA.
    for lv in level + 1..NUM_LEVELS {
        for i in 0..3 {
            if inp.planes[i].is_none() {
                continue;
            }
            let (ddb, ddb_y) = (plane_ddb[i], plane_ddb_y[i]);
            let w = &mut planes[i];
            if w.is_planar {
                // skl_check_nv12_wm_level(): both halves go together.
                if w.wm[lv].min_ddb_alloc > ddb_y.size() || w.uv_wm[lv].min_ddb_alloc > ddb.size() {
                    w.wm[lv] = WmLevel::default();
                    w.uv_wm[lv] = WmLevel::default();
                }
            } else {
                check_wm_level(&mut w.wm[lv], &ddb);
            }
            // skl_need_wm_copy_wa(): level > 0 && !enable -> copy blocks / lines / ignore_lines from the level below.
            if lv > 0 && !w.wm[lv].enable {
                w.wm[lv].blocks = w.wm[lv - 1].blocks;
                w.wm[lv].lines = w.wm[lv - 1].lines;
                w.wm[lv].ignore_lines = w.wm[lv - 1].ignore_lines;
            }
        }
    }
    // The transition watermark is all-zero on ver 9; `skl_check_wm_level` on zero is a no-op.
    for i in 0..3 {
        if inp.planes[i].is_none() {
            continue;
        }
        let ddb_for_trans = if planes[i].is_planar { plane_ddb_y[i] } else { plane_ddb[i] };
        check_wm_level(&mut planes[i].trans_wm, &ddb_for_trans);
    }
    Ok(PipeWm { planes, plane_ddb, plane_ddb_y, cursor_ddb, max_level: level })
}

impl PipeWm {
    /// The tail of `skl_wm_check_vblank()`: every level above `max_level` is switched off (`wm->wm[level].enable = false;
    /// wm->uv_wm[level].enable = false`) - only the enable bit, the numbers stay as the register copy WA left them.
    pub fn limit_levels(&mut self, max_level: usize) {
        for p in self.planes.iter_mut() {
            for lv in max_level + 1..NUM_LEVELS {
                p.wm[lv].enable = false;
                p.uv_wm[lv].enable = false;
            }
        }
    }
    /// `skl_max_wm0_lines()`: the largest level-0 line count over the pipe's planes.
    pub fn max_wm0_lines(&self) -> u32 {
        self.planes.iter().map(|p| p.wm[0].lines as u32).max().unwrap_or(0)
    }
}

/// `skl_ddb_entry_for_slices()` for slice S1 of a one-slice part (`skl_crtc_allocate_ddb`'s single-pipe result: start =
/// `ddb_range_size * 0 / w`, end = `ddb_range_size * w / w`).
pub const fn single_pipe_alloc() -> Ddb {
    Ddb { start: 0, end: DBUF_SIZE_BLOCKS }
}

// ---------------------------------------------------------------------------------------------------------------
// Vblank prefill limit (skl_watermark.c `skl_wm_check_vblank` with skl_prefill.c)
// ---------------------------------------------------------------------------------------------------------------
/// `prefill_usecs_to_lines()`, 16.16 lines.
fn prefill_usecs_to_lines(t: &WmTiming, usecs: u32) -> u64 {
    ((t.clock_khz as u64) * ((usecs as u64) << 16)).div_ceil(t.htotal as u64 * 1000)
}

/// `skl_wm_check_vblank()` — returns the highest level the vertical blank can feed, or `VblankTooShort`. `wm0_lines` is
/// `skl_max_wm0_lines()` (the largest `wm[0].lines` over the pipe's planes), `cdclk_khz` is `calc_cdclk(min_cdclk)` (the CDCLK the
/// pipe will run at; `intel_cdclk_prefill_adjustment_worst`, intel_cdclk.c with ppc 1 on display ver 9).
pub fn max_level_for_vblank(
    lat: &Latencies,
    t: &WmTiming,
    wm0_lines: u32,
    cdclk_khz: u32,
) -> Result<usize, WmError> {
    if t.htotal == 0 || cdclk_khz == 0 {
        return Err(WmError::ZeroDivisor { what: "prefill" });
    }
    // prefill_init(): fixed = framestart_delay << 16 + 20 us worth of lines; dsc 0.
    let fixed = ((FRAMESTART_DELAY as u64) << 16) + prefill_usecs_to_lines(t, 20);
    // No scalers: 1st/2nd prefill 0 and the scaler adjustments are 0x10000.
    let nocdclk = (wm0_lines as u64) << 16;
    // _intel_cdclk_prefill_adj(): min(0x10000, DIV_ROUND_UP((u64)clock << 16, ppc * cdclk)) with ppc = 1.
    let adj = core::cmp::min(0x10000u64, (((t.clock_khz as u64) << 16)).div_ceil(cdclk_khz as u64));
    // prefill_adjust(value, factor) = DIV_ROUND_UP_ULL(value * factor, 0x10000)
    let cdclk_part = (nocdclk * adj).div_ceil(0x10000);
    let full = fixed + cdclk_part;
    let guardband = (t.vblank_lines as u64) << 16;
    let mut level = NUM_LEVELS as i32 - 1;
    while level >= 0 {
        // skl_max_wm_level_for_vblank(): FIXME'd "should we care about the latency w/a's" -> skl_wm_latency(.., NULL)
        let l = lat[level as usize] as u32;
        if l != 0 {
            let latency = if level == 0 { 0 } else { l };
            let prefill = full + prefill_usecs_to_lines(t, latency);
            if guardband >= prefill {
                return Ok(level as usize);
            }
        }
        level -= 1;
    }
    Err(WmError::VblankTooShort)
}

// ---------------------------------------------------------------------------------------------------------------
// Register encoders
// ---------------------------------------------------------------------------------------------------------------
/// `skl_plane_wm_reg_val()`.
pub const fn plane_wm_reg_val(l: &WmLevel) -> u32 {
    let mut v = 0;
    if l.enable {
        v |= PLANE_WM_EN;
    }
    if l.ignore_lines {
        v |= PLANE_WM_IGNORE_LINES;
    }
    v |= ((l.blocks as u32) << 0) & PLANE_WM_BLOCKS_MASK;
    v |= ((l.lines as u32) << PLANE_WM_LINES_SHIFT) & PLANE_WM_LINES_MASK;
    v
}

/// `skl_plane_ddb_reg_val()`.
pub const fn plane_ddb_reg_val(e: &Ddb) -> u32 {
    if e.end == 0 {
        return 0;
    }
    ((((e.end - 1) as u32) << PLANE_BUF_END_SHIFT) & PLANE_BUF_END_MASK) | ((e.start as u32) & PLANE_BUF_START_MASK)
}

/// `skl_cursor_wm_reg_val()`.
pub const fn cursor_wm_reg_val(l: &WmLevel) -> u32 {
    let mut v = 0;
    if l.enable {
        v |= CUR_WM_EN;
    }
    if l.ignore_lines {
        v |= CUR_WM_IGNORE_LINES;
    }
    v |= (l.blocks as u32) & CUR_WM_BLOCKS_MASK;
    v |= ((l.lines as u32) << CUR_WM_LINES_SHIFT) & CUR_WM_LINES_MASK;
    v
}

/// `skl_cursor_ddb_reg_val()`.
pub const fn cursor_ddb_reg_val(e: &Ddb) -> u32 {
    if e.end == 0 {
        return 0;
    }
    ((((e.end - 1) as u32) << CUR_BUF_END_SHIFT) & CUR_BUF_END_MASK) | ((e.start as u32) & CUR_BUF_START_MASK)
}
