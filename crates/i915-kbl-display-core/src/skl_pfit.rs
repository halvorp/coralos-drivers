// SPDX-License-Identifier: GPL-2.0-only
//! The pipe scaler / panel fitter: is one NEEDED for a native-mode eDP (the modeset's question), and the VALUES `skl_pfit_enable`
//! writes when a pipe source smaller than the panel is scaled onto it (the adopted pipe's logical-size fastset).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_scaler.c at v7.0-rc6 (`skl_scaler_calc_phase` :60-82,
//! `skl_scaler_min/max_src_size` :85-110, `skl_scaler_min/max_dst_size` :115-136, `skl_update_scaler`'s range checks :227-256,
//! `calculate_max_scale` :350-384, the pipe-scaling check of `intel_atomic_setup_scaler` :476-506, `skl_scaler_get_filter_select`
//! :728-738, `skl_pfit_enable` :801-858), intel_pfit.c (`pch_panel_fitting` :185-254: the window for DRM_MODE_SCALE_ASPECT, eDP's
//! default scaling mode) and drm_rect.c (`drm_calc_scale` :133-149, `drm_rect_calc_hscale/vscale` :168-218). Copyright (c) Intel
//! Corporation.
//!
//! Every limit is a function of the DISPLAY VERSION (the platform table's `DisplayCaps::display_ver`), never a Kaby Lake constant,
//! so a later generation's row gets its own numbers from the same code.
use crate::regs::{scaler_reg, PS_1A_CTRL, PS_BINDING_PIPE, PS_SCALER_EN, PS_WIN_POS_1A, PS_WIN_SZ_1A};
use crate::regs_write::*;
use crate::types::Pipe;

/// The decision for one pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelFit {
    /// `pch_pfit.enabled == false`: no scaler register is written (`skl_pfit_enable`: "if (!crtc_state->pch_pfit.enabled) return").
    NotNeeded,
}

/// Named refusal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PfitError {
    /// The pipe source differs from the panel's native mode: a panel fit would be required (the native modeset does not program one).
    NotPorted { src_w: u32, src_h: u32, native_w: u32, native_h: u32 },
    /// `skl_update_scaler`'s range check: the source, the window, or the pipe source is outside the scaler's limits.
    OutOfRange { src_w: u32, src_h: u32, dst_w: u32, dst_h: u32 },
    /// `intel_atomic_setup_scaler`: `drm_rect_calc_h/vscale` outside `[0, max]` for this display version.
    ScaleOutOfRange { hscale: u32, vscale: u32, max_hscale: u32, max_vscale: u32 },
    /// The display version has no row here (no scaler limits are known for it).
    UnknownDisplayVersion { display_ver: u8 },
}

impl PfitError {
    pub const fn name(&self) -> &'static str {
        match self {
            PfitError::NotPorted { .. } => "PFIT-SCALER-NOT-PORTED",
            PfitError::OutOfRange { .. } => "PFIT-SCALER-OUT-OF-RANGE",
            PfitError::ScaleOutOfRange { .. } => "PFIT-SCALE-OUT-OF-RANGE",
            PfitError::UnknownDisplayVersion { .. } => "PFIT-UNKNOWN-DISPLAY-VERSION",
        }
    }
}

/// `intel_panel_fitting()`'s enable test for display ver 9: any difference between the pipe source and the mode.
pub const fn check_panel_fit(src_w: u32, src_h: u32, native_w: u32, native_h: u32) -> Result<PanelFit, PfitError> {
    if src_w == native_w && src_h == native_h {
        Ok(PanelFit::NotNeeded)
    } else {
        Err(PfitError::NotPorted { src_w, src_h, native_w, native_h })
    }
}

/// The scaler limits of a display version, for an RGB (not YUV semi-planar) source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalerLimits {
    pub min_src: (u32, u32),
    pub max_src: (u32, u32),
    pub min_dst: (u32, u32),
    pub max_dst: (u32, u32),
    /// 16.16 fixed point (`calculate_max_scale`; the pipe scaler's own scaler id does not matter below display ver 14).
    pub max_hscale: u32,
    pub max_vscale: u32,
}

/// `skl_scaler_min_src_size` (RGB: 8x8), `skl_scaler_max_src_size`, `skl_scaler_min_dst_size` (8x8), `skl_scaler_max_dst_size`,
/// `calculate_max_scale` (RGB: 3.0 - 1 ulp for display ver 9..13). Display ver 14+ and below 9 are not rows of this port.
pub const fn scaler_limits(display_ver: u8) -> Result<ScalerLimits, PfitError> {
    let (max_src, max_dst) = match display_ver {
        9 | 10 => ((4096, 4096), (4096, 4096)),
        11 => ((5120, 4096), (5120, 4096)),
        12 | 13 => ((5120, 8192), (8192, 8192)),
        _ => return Err(PfitError::UnknownDisplayVersion { display_ver }),
    };
    Ok(ScalerLimits { min_src: (8, 8), max_src, min_dst: (8, 8), max_dst, max_hscale: 0x30000 - 1, max_vscale: 0x30000 - 1 })
}

/// `drm_calc_scale(src << 16, dst)`: the 16.16 scale factor source/destination, rounded down when upscaling and up when downscaling.
pub const fn calc_scale(src: u32, dst: u32) -> u32 {
    if dst == 0 {
        return 0;
    }
    let s = (src as u64) << 16;
    if s > (dst as u64) << 16 {
        s.div_ceil(dst as u64) as u32
    } else {
        (s / dst as u64) as u32
    }
}

/// `skl_scaler_calc_phase(sub, scale, chroma_cosited)`: the initial phase, u2.13 with the trip bit (PS_PHASE_MASK | PS_PHASE_TRIP).
pub const fn calc_phase(sub: i32, scale: u32, chroma_cosited: bool) -> u16 {
    let mut phase: i32 = -0x8000;
    let mut trip = 0u32;
    if chroma_cosited {
        phase += (sub - 1) * 0x8000 / sub;
    }
    phase += scale as i32 / (2 * sub);
    if phase < 0 {
        phase += 0x10000;
    } else {
        trip = PS_PHASE_TRIP;
    }
    (((phase as u32 >> 2) & PS_PHASE_MASK) | trip) as u16
}

/// The panel-fitter window (`pch_pfit.dst`), in panel pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// `pch_panel_fitting()` for DRM_MODE_SCALE_ASPECT (intel_dp.c attaches it as eDP's default): the source scaled to the panel with
/// its aspect kept - full panel when the aspects agree, else pillar- or letter-boxed and centred (even size, as Linux rounds).
pub const fn aspect_window(src_w: u32, src_h: u32, panel_w: u32, panel_h: u32) -> Window {
    let scaled_width = panel_w as u64 * src_h as u64;
    let scaled_height = src_w as u64 * panel_h as u64;
    if scaled_width > scaled_height {
        let mut w = (scaled_height / src_h as u64) as u32;
        if w & 1 != 0 {
            w += 1;
        }
        Window { x: (panel_w - w).div_ceil(2), y: 0, w, h: panel_h }
    } else if scaled_width < scaled_height {
        let mut h = (scaled_width / src_w as u64) as u32;
        if h & 1 != 0 {
            h += 1;
        }
        Window { x: 0, y: (panel_h - h).div_ceil(2), w: panel_w, h }
    } else {
        Window { x: 0, y: 0, w: panel_w, h: panel_h }
    }
}

/// What `skl_pfit_enable` writes for one pipe scaler, in its order: PS_CTRL, PS_VPHASE, PS_HPHASE, PS_WIN_POS, PS_WIN_SZ (the
/// last arms the scaler). `hscale` / `vscale` are kept for the caller's marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PfitWrites {
    pub writes: [(u32, u32); 5],
    pub hscale: u32,
    pub vscale: u32,
}

/// `skl_update_scaler`'s range checks, `intel_atomic_setup_scaler`'s scale check and `skl_pfit_enable`'s values for scaling a
/// `src_w x src_h` pipe source into `dst` on `pipe`'s scaler `id`. `mode` is the scaler mode the caller keeps
/// (`SKL_PS_SCALER_MODE_*`, the bits `scaler_state->scalers[id].mode` holds); the filter is `skl_scaler_get_filter_select` for
/// DRM_SCALING_FILTER_DEFAULT (PS_FILTER_MEDIUM, and `skl_scaler_setup_filter` programs nothing for it).
pub fn pfit_enable_writes(display_ver: u8, pipe: Pipe, id: u32, mode: u32, src_w: u32, src_h: u32, dst: Window) -> Result<PfitWrites, PfitError> {
    let l = scaler_limits(display_ver)?;
    let out = PfitError::OutOfRange { src_w, src_h, dst_w: dst.w, dst_h: dst.h };
    if src_w < l.min_src.0 || src_h < l.min_src.1 || dst.w < l.min_dst.0 || dst.h < l.min_dst.1 {
        return Err(out);
    }
    if src_w > l.max_src.0 || src_h > l.max_src.1 || dst.w > l.max_dst.0 || dst.h > l.max_dst.1 {
        return Err(out);
    }
    // "the *pipe* source size must not be too large ... we assume the limits match the scaler destination size limits"
    if src_w > l.max_dst.0 || src_h > l.max_dst.1 {
        return Err(out);
    }
    let hscale = calc_scale(src_w, dst.w);
    let vscale = calc_scale(src_h, dst.h);
    if hscale > l.max_hscale || vscale > l.max_vscale {
        return Err(PfitError::ScaleOutOfRange { hscale, vscale, max_hscale: l.max_hscale, max_vscale: l.max_vscale });
    }
    let uv_rgb_hphase = calc_phase(1, hscale, false) as u32;
    let uv_rgb_vphase = calc_phase(1, vscale, false) as u32;
    let ctrl = PS_SCALER_EN | PS_BINDING_PIPE | (mode & SKL_PS_SCALER_MODE_MASK) | PS_FILTER_MEDIUM;
    let r = |base| scaler_reg(base, pipe, id);
    Ok(PfitWrites {
        writes: [
            (r(PS_1A_CTRL), ctrl),
            (r(PS_VPHASE_1A), uv_rgb_vphase & PS_UV_RGB_PHASE_MASK), // PS_Y_PHASE(0) | PS_UV_RGB_PHASE(vphase)
            (r(PS_HPHASE_1A), uv_rgb_hphase & PS_UV_RGB_PHASE_MASK),
            (r(PS_WIN_POS_1A), (dst.x << 16) | (dst.y & 0xffff)),
            (r(PS_WIN_SZ_1A), (dst.w << 16) | (dst.h & 0xffff)),
        ],
        hscale,
        vscale,
    })
}
