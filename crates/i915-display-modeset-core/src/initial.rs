// SPDX-License-Identifier: GPL-2.0-only
//! The INITIAL PLANE CONFIG and the ADOPTED DISPLAY: what the firmware (GOP) is scanning out, the GGTT extent its framebuffer
//! occupies, and the one mode a driver that adopts it - and does not modeset - can honestly offer. Read-only: everything here is
//! computed from the takeover readout (`i915_kbl_display_core::readout`); nothing is written, and nothing here can write.
//!
//! WHY. A driver that owns the GPU function must not disturb the running scanout while it brings up the rest of the device: the
//! framebuffer's GGTT entries and the pipe / plane / scaler / power state are load-bearing from the first instruction. Linux keeps
//! them by reading the plane at load (`skl_get_initial_plane_config`), pinning the framebuffer at the base the plane reads before it
//! clears the rest of the GGTT (`initial_plane_vma`, `init_ggtt`), and inheriting the pipe's state. The adoption PREDICATE below is
//! narrower than Linux's on purpose: one active pipe, its primary plane full-source and unpositioned, RGB, unrotated, not
//! compressed, and at most a pipe-bound panel fitter between the source and the panel - anything else is refused BY NAME, and the
//! device then adopts nothing (it still never writes the display).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_universal_plane.c (`skl_get_initial_plane_config`, :3044-3210: format and
//! modifier from PLANE_CTL, the 90/270 refusal, `base = PLANE_SURF & PLANE_SURF_ADDR_MASK`, `size = pitch * aligned_height`),
//! intel_fb.c (`intel_tile_size` :769-772, `intel_tile_width_bytes` :774-842, `intel_tile_height` :843-849, `intel_fb_align_height`
//! :891-897), i915_initial_plane.c (`initial_plane_vma` :86-215: the extent rounded down to `I915_GTT_MIN_ALIGNMENT` and up to the
//! region's `min_page_size`), and include/uapi/drm/drm_fourcc.h (`fourcc_code` :105, the modifiers :544, :573, :591, :606).
//! OWED to the device's GGTT bring-up (D17, 2026-10-16): `initial_plane_phys` (the framebuffer PTE must be present and inside stolen memory,
//! i915_initial_plane.c:35-84) reads the GGTT, which only the device maps, so it is the device's step, not this crate's. Deliberately
//! absent: the "larger than half of stolen" fbdev discard (:116-120), which is fbdev policy, not adoption. Copyright (c) Intel Corporation.
use i915_kbl_display_core::plane::{fourcc_name, PlaneFormat, PlaneState, Rotation, ScalerBinding, ScalerState, Tiling};
use i915_kbl_display_core::power::Well;
use i915_kbl_display_core::readout::{PipeState, ReadoutState};
use i915_kbl_display_core::types::{Pipe, Transcoder};

/// `I915_GTT_MIN_ALIGNMENT` = `I915_GTT_PAGE_SIZE` = 4 KiB (gt/intel_gtt.h:45, :49, :54): the base is rounded down to it.
pub const GTT_MIN_ALIGNMENT: u64 = 1 << 12;
/// `intel_tile_size()` for display version != 2 (intel_fb.c:769-772).
pub const TILE_SIZE: u32 = 4096;
/// `DRM_FORMAT_MOD_LINEAR` = `fourcc_mod_code(NONE, 0)` (drm_fourcc.h:544).
pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;
/// `fourcc_mod_code(INTEL, n)` = `(DRM_FORMAT_MOD_VENDOR_INTEL << 56) | n` (drm_fourcc.h:468, :491-492).
const fn intel_mod(n: u64) -> u64 {
    (0x01 << 56) | n
}
/// `I915_FORMAT_MOD_X_TILED` (drm_fourcc.h:573).
pub const I915_FORMAT_MOD_X_TILED: u64 = intel_mod(1);
/// `I915_FORMAT_MOD_Y_TILED` (drm_fourcc.h:591).
pub const I915_FORMAT_MOD_Y_TILED: u64 = intel_mod(2);
/// `I915_FORMAT_MOD_Yf_TILED` (drm_fourcc.h:606).
pub const I915_FORMAT_MOD_YF_TILED: u64 = intel_mod(3);

/// The modifier `skl_get_initial_plane_config` gives an uncompressed plane of this tiling (skl_universal_plane.c:3102-3140).
pub const fn modifier(t: Tiling) -> u64 {
    match t {
        Tiling::Linear => DRM_FORMAT_MOD_LINEAR,
        Tiling::X => I915_FORMAT_MOD_X_TILED,
        Tiling::Y => I915_FORMAT_MOD_Y_TILED,
        Tiling::Yf => I915_FORMAT_MOD_YF_TILED,
    }
}

/// `intel_tile_width_bytes` for colour plane 0 of an uncompressed display-ver-9 framebuffer (intel_fb.c:780-841): linear = the tile
/// size; X = 512; Y = 128 (`HAS_128B_Y_TILING` is every part past i915g/gm); Yf by bytes per pixel. `None` where Linux has no case.
pub const fn tile_width_bytes(t: Tiling, cpp: u32) -> Option<u32> {
    match t {
        Tiling::Linear => Some(TILE_SIZE),
        Tiling::X => Some(512),
        Tiling::Y => Some(128),
        Tiling::Yf => match cpp {
            1 => Some(64),
            2 | 4 => Some(128),
            8 | 16 => Some(256),
            _ => None,
        },
    }
}

/// `intel_fb_align_height`: `ALIGN(height, intel_tile_height)`, `intel_tile_height = intel_tile_size / intel_tile_width_bytes`.
pub const fn align_height(t: Tiling, cpp: u32, height: u32) -> Option<u32> {
    match tile_width_bytes(t, cpp) {
        Some(w) => {
            let th = TILE_SIZE / w;
            Some(height.div_ceil(th) * th)
        }
        None => None,
    }
}

/// The DRM fourcc of an RGB scanout format (`skl_format_to_fourcc`, skl_universal_plane.c:173-231, as the four characters of its
/// name: `fourcc_code(a, b, c, d) = a | b << 8 | c << 16 | d << 24`, drm_fourcc.h:105). `None` for a format whose name is not four
/// characters - the YUV and indexed ones, which the adoption refuses.
pub fn fourcc(f: PlaneFormat, rgb_order: bool, alpha: bool) -> Option<u32> {
    let n = fourcc_name(f, rgb_order, alpha).as_bytes();
    if n.len() != 4 || f.cpp().is_none() || f == PlaneFormat::Indexed {
        return None;
    }
    Some(u32::from_le_bytes([n[0], n[1], n[2], n[3]]))
}

/// Why the firmware's scanout was not adopted. Each has a stable `name()`; the device answers DISPLAY-QUERY with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdoptRefusal {
    /// No pipe is active: there is nothing to adopt (the firmware lit no display).
    NoActivePipe,
    /// More than one pipe is active (`mask` = bit per pipe): this adoption keeps exactly one.
    MultipleActivePipes { mask: u8 },
    /// The active pipe's primary plane could not be read (its register held a reserved value).
    PlaneUnreadable,
    /// The primary plane is disabled (`plane->get_hw_state` false, skl_universal_plane.c:3058).
    PlaneDisabled,
    /// The plane's format is not an RGB scanout format (YUV / planar / indexed).
    FormatNotRgb,
    /// Render or media decompression is on: the framebuffer has an aux plane this adoption does not account.
    Compressed,
    /// 90/270 rotation "would require extra work" (skl_universal_plane.c:3180-3182); 180 is refused too (layout must be 0).
    Rotated,
    /// `PLANE_OFFSET` is not 0 (Linux WARNs, skl_universal_plane.c:3187-3188).
    OffsetNonZero { x: u32, y: u32 },
    /// The stride or the tile height could not be determined from PLANE_CTL / PLANE_STRIDE.
    StrideUnknown,
    /// The plane does not cover the pipe source exactly from (0, 0).
    PlaneNotPipeSource { plane_w: u32, plane_h: u32, pos_x: u32, pos_y: u32, src_w: u32, src_h: u32 },
    /// A scaler is bound to a PLANE (not the pipe): the adoption keeps only a pipe-bound panel fitter.
    PlaneScaler { scaler: u8 },
    /// The pipe source and the panel's active size differ, and no pipe-bound panel fitter covers the panel.
    UnscaledSizeMismatch { src_w: u32, src_h: u32, active_w: u32, active_h: u32 },
    /// The transcoder is interlaced.
    Interlaced,
    /// The framebuffer extent does not fit 32-bit GGTT addressing.
    ExtentOverflow,
}

impl AdoptRefusal {
    pub const fn name(&self) -> &'static str {
        match self {
            AdoptRefusal::NoActivePipe => "ADOPT-NO-ACTIVE-PIPE",
            AdoptRefusal::MultipleActivePipes { .. } => "ADOPT-MULTIPLE-ACTIVE-PIPES",
            AdoptRefusal::PlaneUnreadable => "ADOPT-PLANE-UNREADABLE",
            AdoptRefusal::PlaneDisabled => "ADOPT-PLANE-DISABLED",
            AdoptRefusal::FormatNotRgb => "ADOPT-FORMAT-NOT-RGB",
            AdoptRefusal::Compressed => "ADOPT-COMPRESSED",
            AdoptRefusal::Rotated => "ADOPT-ROTATED",
            AdoptRefusal::OffsetNonZero { .. } => "ADOPT-OFFSET-NONZERO",
            AdoptRefusal::StrideUnknown => "ADOPT-STRIDE-UNKNOWN",
            AdoptRefusal::PlaneNotPipeSource { .. } => "ADOPT-PLANE-NOT-PIPE-SOURCE",
            AdoptRefusal::PlaneScaler { .. } => "ADOPT-PLANE-SCALER",
            AdoptRefusal::UnscaledSizeMismatch { .. } => "ADOPT-UNSCALED-SIZE-MISMATCH",
            AdoptRefusal::Interlaced => "ADOPT-INTERLACED",
            AdoptRefusal::ExtentOverflow => "ADOPT-EXTENT-OVERFLOW",
        }
    }
}

/// `struct intel_initial_plane_config` as `skl_get_initial_plane_config` fills it, for colour plane 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialPlaneConfig {
    pub pipe: Pipe,
    /// 0 = PLANE_1 (the primary).
    pub plane_id: u8,
    /// `plane_config->base`: PLANE_SURF with the low 12 bits masked - the framebuffer's GGTT address.
    pub base: u32,
    /// `plane_config->size` = `pitches[0] * intel_fb_align_height(height)` (skl_universal_plane.c:3200).
    pub size: u64,
    pub width: u32,
    pub height: u32,
    /// `fb->pitches[0]` in bytes.
    pub stride: u32,
    pub cpp: u32,
    pub format: PlaneFormat,
    pub rgb_order: bool,
    pub alpha: bool,
    pub tiling: Tiling,
    pub modifier: u64,
    pub fourcc: u32,
}

/// The primary plane of `pipe` as the firmware programmed it, or why it is not a framebuffer this adoption keeps.
pub fn initial_plane_config(r: &ReadoutState, pipe: Pipe) -> Result<InitialPlaneConfig, AdoptRefusal> {
    let p: PlaneState = match r.pipes[pipe.index()].planes[0] {
        Some(Ok(p)) => p,
        _ => return Err(AdoptRefusal::PlaneUnreadable),
    };
    if !p.enabled {
        return Err(AdoptRefusal::PlaneDisabled);
    }
    let (Some(format), Some(tiling)) = (p.format, p.tiling) else { return Err(AdoptRefusal::PlaneUnreadable) };
    let fourcc = fourcc(format, p.rgb_order, p.alpha).ok_or(AdoptRefusal::FormatNotRgb)?;
    let cpp = format.cpp().ok_or(AdoptRefusal::FormatNotRgb)?;
    if p.render_decompression {
        return Err(AdoptRefusal::Compressed);
    }
    if p.rotation != Rotation::R0 {
        return Err(AdoptRefusal::Rotated);
    }
    if p.offset_x != 0 || p.offset_y != 0 {
        return Err(AdoptRefusal::OffsetNonZero { x: p.offset_x, y: p.offset_y });
    }
    let stride = p.stride_bytes.filter(|s| *s != 0).ok_or(AdoptRefusal::StrideUnknown)?;
    let aligned = align_height(tiling, cpp, p.height).ok_or(AdoptRefusal::StrideUnknown)?;
    Ok(InitialPlaneConfig {
        pipe,
        plane_id: p.plane_id,
        base: p.surf,
        size: stride as u64 * aligned as u64,
        width: p.width,
        height: p.height,
        stride,
        cpp,
        format,
        rgb_order: p.rgb_order,
        alpha: p.alpha,
        tiling,
        modifier: modifier(tiling),
        fourcc,
    })
}

/// The GGTT range the framebuffer occupies: `[round_down(base, I915_GTT_MIN_ALIGNMENT), round_up(base + size, min_page_size))`
/// (`initial_plane_vma`, i915_initial_plane.c:105-108). `min_page_size` is the region's (`I915_GTT_PAGE_SIZE_4K` for stolen
/// system memory; 64 KiB only where `HAS_64K_PAGES`, i915_gem_stolen.c:985-986) and must be a power of two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GgttExtent {
    pub start: u64,
    pub size: u64,
}

impl InitialPlaneConfig {
    pub fn ggtt_extent(&self, min_page_size: u64) -> Result<GgttExtent, AdoptRefusal> {
        if !min_page_size.is_power_of_two() || min_page_size < GTT_MIN_ALIGNMENT {
            return Err(AdoptRefusal::ExtentOverflow);
        }
        let base = self.base as u64 & !(GTT_MIN_ALIGNMENT - 1);
        let end = (self.base as u64).checked_add(self.size).and_then(|e| e.checked_add(min_page_size - 1)).ok_or(AdoptRefusal::ExtentOverflow)? & !(min_page_size - 1);
        if end > 1 << 32 {
            return Err(AdoptRefusal::ExtentOverflow);
        }
        Ok(GgttExtent { start: base, size: end - base })
    }
}

/// The ONE mode a driver that adopts the firmware's display offers: the source size the clients draw, at the panel's refresh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OfferedMode {
    pub width: u32,
    pub height: u32,
    /// 0 = unknown (the readout had no dotclock); a request then must say 0 too.
    pub refresh_millihz: u32,
}

/// A mode request that is not the adopted one: named, with both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeNotAdopted {
    pub asked: OfferedMode,
    pub adopted: OfferedMode,
}

impl ModeNotAdopted {
    pub const fn name(&self) -> &'static str {
        "MODE-NOT-ADOPTED"
    }
}

/// Everything the adoption keeps: the pipe and transcoder, the framebuffer, its GGTT extent, the panel's active size, the refresh,
/// the panel fitter, and which power wells were on (the dependencies the device must not release while it scans out).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdoptedDisplay {
    pub pipe: Pipe,
    pub transcoder: Transcoder,
    pub fb: InitialPlaneConfig,
    pub extent: GgttExtent,
    /// The transcoder's active size: what the panel shows (`hdisplay x vdisplay`).
    pub active_w: u32,
    pub active_h: u32,
    pub refresh_millihz: Option<u32>,
    /// The pipe-bound panel fitter, when the source is scaled onto the panel.
    pub fitter: Option<ScalerState>,
    /// Bit `w as u32` per power well that was enabled (`Well` order).
    pub wells_on: u32,
}

impl AdoptedDisplay {
    pub const fn scaled(&self) -> bool {
        self.fitter.is_some()
    }
    pub fn mode(&self) -> OfferedMode {
        OfferedMode { width: self.fb.width, height: self.fb.height, refresh_millihz: self.refresh_millihz.unwrap_or(0) }
    }
    /// The adopted mode is the only one: anything else is refused by name (a native modeset is not this adoption's).
    pub fn check_mode(&self, asked: OfferedMode) -> Result<(), ModeNotAdopted> {
        if asked == self.mode() {
            Ok(())
        } else {
            Err(ModeNotAdopted { asked, adopted: self.mode() })
        }
    }
}

/// THE ADOPTION PREDICATE over the takeover readout. `min_page_size` is the framebuffer region's (see [`GgttExtent`]).
pub fn adopt(r: &ReadoutState, min_page_size: u64) -> Result<AdoptedDisplay, AdoptRefusal> {
    let mut mask = 0u8;
    for p in Pipe::ALL {
        if matches!(r.pipes[p.index()].state, PipeState::Active(_)) {
            mask |= 1 << p.index();
        }
    }
    let pipe = match mask.count_ones() {
        0 => return Err(AdoptRefusal::NoActivePipe),
        1 => Pipe::ALL[mask.trailing_zeros() as usize],
        _ => return Err(AdoptRefusal::MultipleActivePipes { mask }),
    };
    let pr = &r.pipes[pipe.index()];
    let PipeState::Active(a) = pr.state else { return Err(AdoptRefusal::NoActivePipe) };
    if a.timings.interlaced {
        return Err(AdoptRefusal::Interlaced);
    }
    let fb = initial_plane_config(r, pipe)?;
    let plane = match pr.planes[0] {
        Some(Ok(p)) => p,
        _ => return Err(AdoptRefusal::PlaneUnreadable),
    };
    if fb.width != a.src_w || fb.height != a.src_h || plane.pos_x != 0 || plane.pos_y != 0 {
        return Err(AdoptRefusal::PlaneNotPipeSource { plane_w: fb.width, plane_h: fb.height, pos_x: plane.pos_x, pos_y: plane.pos_y, src_w: a.src_w, src_h: a.src_h });
    }
    let mut fitter = None;
    for s in pr.scalers.iter().flatten() {
        if !s.enabled {
            continue;
        }
        match s.binding {
            ScalerBinding::Plane(_) => return Err(AdoptRefusal::PlaneScaler { scaler: s.id }),
            ScalerBinding::Pipe => fitter = Some(*s),
        }
    }
    let (active_w, active_h) = (a.timings.hdisplay, a.timings.vdisplay);
    let covered = |s: &ScalerState| s.win_x == 0 && s.win_y == 0 && s.win_w == active_w && s.win_h == active_h;
    let unscaled = a.src_w == active_w && a.src_h == active_h;
    if !unscaled && !fitter.as_ref().is_some_and(covered) {
        return Err(AdoptRefusal::UnscaledSizeMismatch { src_w: a.src_w, src_h: a.src_h, active_w, active_h });
    }
    let extent = fb.ggtt_extent(min_page_size)?;
    let mut wells_on = 0u32;
    for w in Well::ALL {
        if r.power.well(w).enabled {
            wells_on |= 1 << w as u32;
        }
    }
    Ok(AdoptedDisplay { pipe, transcoder: a.transcoder, fb, extent, active_w, active_h, refresh_millihz: a.refresh_millihz(), fitter, wells_on })
}
