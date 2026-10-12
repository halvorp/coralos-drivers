// SPDX-License-Identifier: GPL-2.0-only
//! Universal planes (primary + sprites) and the pipe scalers / panel fitter, as the firmware left them.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_universal_plane.c (`skl_plane_get_hw_state`,
//! `skl_get_initial_plane_config`, `skl_format_to_fourcc`, `skl_plane_stride_mult`), intel_fb.c
//! (`intel_tile_width_bytes`), skl_scaler.c (`skl_scaler_get_config`), skl_universal_plane_regs.h and
//! intel_display_regs.h. Copyright (c) Intel Corporation.

use crate::error::ReadoutError;
use crate::regs::*;
use crate::timing::field_get;

/// `PLANE_CTL_FORMAT` (bits 27:24) of display ver 9 (`PLANE_CTL_FORMAT_MASK_SKL`, skl_universal_plane_regs.h:48-60).
/// Linux's `skl_format_to_fourcc` sends every value it does not list to XRGB8888 (its `default:` shares that case,
/// skl_universal_plane.c:200-201); here an undefined value is a named error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneFormat {
    Yuv422,
    Nv12,
    Xrgb2101010,
    P010,
    Xrgb8888,
    P012,
    Xrgb16161616f,
    P016,
    Xyuv,
    Indexed,
    Rgb565,
}

impl PlaneFormat {
    /// Decode the format field of a PLANE_CTL word.
    pub fn decode(ctl: u32) -> Option<PlaneFormat> {
        match ctl & PLANE_CTL_FORMAT_MASK_SKL {
            PLANE_CTL_FORMAT_YUV422 => Some(PlaneFormat::Yuv422),
            PLANE_CTL_FORMAT_NV12 => Some(PlaneFormat::Nv12),
            PLANE_CTL_FORMAT_XRGB_2101010 => Some(PlaneFormat::Xrgb2101010),
            PLANE_CTL_FORMAT_P010 => Some(PlaneFormat::P010),
            PLANE_CTL_FORMAT_XRGB_8888 => Some(PlaneFormat::Xrgb8888),
            PLANE_CTL_FORMAT_P012 => Some(PlaneFormat::P012),
            PLANE_CTL_FORMAT_XRGB_16161616F => Some(PlaneFormat::Xrgb16161616f),
            PLANE_CTL_FORMAT_P016 => Some(PlaneFormat::P016),
            PLANE_CTL_FORMAT_XYUV => Some(PlaneFormat::Xyuv),
            PLANE_CTL_FORMAT_INDEXED => Some(PlaneFormat::Indexed),
            PLANE_CTL_FORMAT_RGB_565 => Some(PlaneFormat::Rgb565),
            _ => None,
        }
    }
    /// Bytes per pixel of plane 0 for the RGB formats (`fb->format->cpp[0]`); `None` for the planar / packed YUV ones.
    pub const fn cpp(self) -> Option<u32> {
        match self {
            PlaneFormat::Xrgb8888 | PlaneFormat::Xrgb2101010 => Some(4),
            PlaneFormat::Rgb565 => Some(2),
            PlaneFormat::Xrgb16161616f => Some(8),
            PlaneFormat::Indexed => Some(1),
            _ => None,
        }
    }
}

/// The DRM fourcc name `skl_format_to_fourcc(format, rgb_order, alpha)` picks (skl_universal_plane.c:173-231).
pub fn fourcc_name(f: PlaneFormat, rgb_order: bool, alpha: bool) -> &'static str {
    match (f, rgb_order, alpha) {
        (PlaneFormat::Rgb565, _, _) => "RG16",
        (PlaneFormat::Nv12, _, _) => "NV12",
        (PlaneFormat::Xyuv, _, _) => "XYUV8888",
        (PlaneFormat::P010, _, _) => "P010",
        (PlaneFormat::P012, _, _) => "P012",
        (PlaneFormat::P016, _, _) => "P016",
        (PlaneFormat::Yuv422, _, _) => "YUV422",
        (PlaneFormat::Indexed, _, _) => "C8",
        (PlaneFormat::Xrgb8888, false, false) => "XR24",
        (PlaneFormat::Xrgb8888, false, true) => "AR24",
        (PlaneFormat::Xrgb8888, true, false) => "XB24",
        (PlaneFormat::Xrgb8888, true, true) => "AB24",
        (PlaneFormat::Xrgb2101010, false, false) => "XR30",
        (PlaneFormat::Xrgb2101010, false, true) => "AR30",
        (PlaneFormat::Xrgb2101010, true, false) => "XB30",
        (PlaneFormat::Xrgb2101010, true, true) => "AB30",
        (PlaneFormat::Xrgb16161616f, false, false) => "XR4H",
        (PlaneFormat::Xrgb16161616f, false, true) => "AR4H",
        (PlaneFormat::Xrgb16161616f, true, false) => "XB4H",
        (PlaneFormat::Xrgb16161616f, true, true) => "AB4H",
    }
}

/// `PLANE_CTL_TILED` (skl_universal_plane_regs.h:83-88). `Yf` is also the 4-tile encoding on newer parts; Kaby Lake
/// has no 4-tile (`HAS_4TILE` is DG2 and display ver 14+, intel_display_device.h:149).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tiling {
    Linear,
    X,
    Y,
    Yf,
}

impl Tiling {
    pub fn name(self) -> &'static str {
        match self {
            Tiling::Linear => "linear",
            Tiling::X => "X",
            Tiling::Y => "Y",
            Tiling::Yf => "Yf",
        }
    }
    pub fn decode(ctl: u32) -> Option<Tiling> {
        match ctl & PLANE_CTL_TILED_MASK {
            PLANE_CTL_TILED_LINEAR => Some(Tiling::Linear), // skl_universal_plane.c:3103
            PLANE_CTL_TILED_X => Some(Tiling::X),           // skl_universal_plane.c:3106
            PLANE_CTL_TILED_Y => Some(Tiling::Y),           // skl_universal_plane.c:3109
            PLANE_CTL_TILED_YF => Some(Tiling::Yf),         // skl_universal_plane.c:3125
            _ => None,
        }
    }
}

/// `drm_rotation` as the hardware field reads (clockwise degrees; DRM's own constants count the other way,
/// skl_universal_plane.c:3159-3175).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rotation {
    R0,
    R90,
    R180,
    R270,
}

/// `skl_plane_stride_mult` (skl_universal_plane.c:751-765): the unit the 12-bit stride field counts in. 64 bytes for
/// linear; the tile width in bytes for tiled (X: 512, intel_fb.c:783-787; Y: 128 where `HAS_128B_Y_TILING`, true for
/// every non-i915g/gm part, intel_fb.c:813-817, intel_display_device.h:148; Yf: 64/128/256 by bytes per pixel,
/// intel_fb.c:822-835). `None` if the Yf case needs a bytes-per-pixel this format cannot give.
pub fn stride_unit_bytes(t: Tiling, cpp: Option<u32>) -> Option<u32> {
    match t {
        Tiling::Linear => Some(64),
        Tiling::X => Some(512),
        Tiling::Y => Some(128),
        Tiling::Yf => match cpp? {
            1 => Some(64),
            2 | 4 => Some(128),
            8 | 16 => Some(256),
            _ => None,
        },
    }
}

/// One universal plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneState {
    /// 0 = PLANE_1 (the primary), 1 = PLANE_2, 2 = PLANE_3 (`enum plane_id`, intel_display_limits.h:74-76).
    pub plane_id: u8,
    pub ctl: u32,
    /// `PLANE_CTL_ENABLE` (`skl_plane_get_hw_state`, skl_universal_plane.c:949).
    pub enabled: bool,
    pub format: Option<PlaneFormat>,
    pub rgb_order: bool,
    pub alpha: bool,
    pub tiling: Option<Tiling>,
    pub render_decompression: bool,
    pub rotation: Rotation,
    /// PLANE_STRIDE field, in `stride_unit_bytes` units.
    pub stride_field: u32,
    /// The stride in bytes (`fb->pitches[0]`, skl_universal_plane.c:3195-3197); `None` if undeterminable.
    pub stride_bytes: Option<u32>,
    pub width: u32,
    pub height: u32,
    pub pos_x: u32,
    pub pos_y: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    /// PLANE_SURF with the low 12 bits masked: the GGTT address of the framebuffer (`plane_config->base`).
    pub surf: u32,
}

/// Decode a plane from its six registers. A disabled plane is still decoded (its registers keep the last
/// programming), but `enabled` is false and format/tiling errors are not raised for it.
pub fn decode_plane(
    plane_id: u8,
    ctl: u32,
    stride: u32,
    size: u32,
    pos: u32,
    offset: u32,
    surf: u32,
) -> Result<PlaneState, ReadoutError> {
    let enabled = ctl & PLANE_CTL_ENABLE != 0;
    let format = PlaneFormat::decode(ctl);
    let tiling = Tiling::decode(ctl);
    if enabled {
        if format.is_none() {
            return Err(ReadoutError::FormatReserved { plane: plane_id + 1, field: (ctl & PLANE_CTL_FORMAT_MASK_SKL) >> 24 });
        }
        if tiling.is_none() {
            return Err(ReadoutError::TilingReserved { plane: plane_id + 1, field: (ctl & PLANE_CTL_TILED_MASK) >> 10 });
        }
    }
    let stride_field = stride & PLANE_STRIDE_MASK;
    let stride_bytes = match (tiling, format) {
        (Some(t), Some(f)) => stride_unit_bytes(t, f.cpp()).map(|u| u * stride_field),
        (Some(t), None) => stride_unit_bytes(t, None).map(|u| u * stride_field),
        _ => None,
    };
    Ok(PlaneState {
        plane_id,
        ctl,
        enabled,
        format,
        rgb_order: ctl & PLANE_CTL_ORDER_RGBX != 0,
        alpha: ctl & PLANE_CTL_ALPHA_MASK != 0,
        tiling,
        render_decompression: ctl & PLANE_CTL_RENDER_DECOMPRESSION_ENABLE != 0,
        rotation: match ctl & PLANE_CTL_ROTATE_MASK {
            PLANE_CTL_ROTATE_0 => Rotation::R0,
            PLANE_CTL_ROTATE_90 => Rotation::R90,
            PLANE_CTL_ROTATE_180 => Rotation::R180,
            _ => Rotation::R270,
        },
        stride_field,
        stride_bytes,
        width: field_get(PLANE_WIDTH_MASK, size) + 1,   // skl_universal_plane.c:3192
        height: field_get(PLANE_HEIGHT_MASK, size) + 1, // skl_universal_plane.c:3191
        pos_x: field_get(PLANE_POS_X_MASK, pos),
        pos_y: field_get(PLANE_POS_Y_MASK, pos),
        offset_x: field_get(PLANE_OFFSET_X_MASK, offset),
        offset_y: field_get(PLANE_OFFSET_Y_MASK, offset),
        surf: surf & PLANE_SURF_ADDR_MASK, // skl_universal_plane.c:3184
    })
}

/// What a scaler is bound to (`PS_BINDING_*`, intel_display_regs.h:1079-1081).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalerBinding {
    Pipe,
    /// `PS_BINDING_PLANE(plane_id)` = `plane_id + 1` in the field.
    Plane(u8),
}

/// One SKL pipe scaler (`SKL_PS_CTRL` / `SKL_PS_WIN_POS` / `SKL_PS_WIN_SZ`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScalerState {
    pub id: u8,
    pub enabled: bool,
    pub binding: ScalerBinding,
    pub win_x: u32,
    pub win_y: u32,
    pub win_w: u32,
    pub win_h: u32,
}

pub fn decode_scaler(id: u8, ctrl: u32, win_pos: u32, win_sz: u32) -> ScalerState {
    let b = (ctrl & PS_BINDING_MASK) >> 25;
    ScalerState {
        id,
        enabled: ctrl & PS_SCALER_EN != 0,
        binding: if b == 0 { ScalerBinding::Pipe } else { ScalerBinding::Plane((b - 1) as u8) },
        win_x: field_get(PS_WIN_XPOS_MASK, win_pos),
        win_y: field_get(PS_WIN_YPOS_MASK, win_pos),
        win_w: field_get(PS_WIN_XSIZE_MASK, win_sz),
        win_h: field_get(PS_WIN_YSIZE_MASK, win_sz),
    }
}

impl ScalerState {
    /// A scaler is the pipe's panel fitter when enabled and bound to the pipe (`skl_scaler_get_config`,
    /// skl_scaler.c:980): `(ctl & (PS_SCALER_EN | PS_BINDING_MASK)) == (PS_SCALER_EN | PS_BINDING_PIPE)`.
    pub fn is_pipe_fitter(&self) -> bool {
        self.enabled && self.binding == ScalerBinding::Pipe
    }
}
