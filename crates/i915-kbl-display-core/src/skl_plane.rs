// SPDX-License-Identifier: GPL-2.0-only
//! Universal-plane WRITE encoding for display version 9 (Skylake / Kaby Lake): the six-plus register words Linux programs for a
//! plane, from a plain description of the framebuffer, the source/destination rectangles and the blend state. Pure values: no
//! register is touched here (the sequencing into effects is `i915-display-modeset-core`).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_universal_plane.c at v7.0-rc6: `skl_plane_ctl`, `skl_plane_ctl_format`,
//! `skl_plane_ctl_alpha`, `skl_plane_ctl_tiling`, `skl_plane_ctl_rotate`, `skl_plane_ctl_crtc`, `skl_plane_stride`,
//! `skl_plane_stride_mult`, `skl_plane_update_noarm`, `skl_plane_update_arm`, `skl_plane_keyval/keymax/keymsk`,
//! `skl_plane_aux_dist`, `skl_plane_max_width/height/stride`, `skl_plane_min_alignment`, `skl_plane_check_fb`,
//! `skl_plane_has_planar`. Copyright (c) Intel Corporation.
//!
//! WHAT IS DELIBERATELY NOT PORTED (each is a NAMED refusal, never a silent default):
//!  * rotation other than 0 (`PlaneError::RotationNotPorted`) and any reflection - the 90/270 path needs Y-tiled views and the
//!    rotated GTT mapping (`intel_fb_view`), which is `i915-memory-core` work;
//!  * Yf tiling and every CCS / render-compression modifier (`TilingNotPorted`);
//!  * plane scaling (`ScalerNotPorted`): the destination rectangle must equal the source rectangle;
//!  * colour keying (the key words are written, but only as the "no key" values `skl_plane_keyval/keymsk/keymax` give for
//!    `ckey.flags == 0`) and the cursor plane (that is `skl_wm` + `plane_plan`'s cursor block).
use crate::regs::*;
use crate::regs_write::*;
use crate::types::Pipe;

/// The pixel formats the first write milestone programs. All four packed RGB formats are 4 bytes per pixel with
/// `skl_plane_formats[]` (skl_universal_plane.c:34-49); NV12 is in `skl_planar_formats[]` only where
/// `skl_plane_has_planar` (skl_universal_plane.c:2469).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteFormat {
    Xrgb8888,
    Argb8888,
    Xbgr8888,
    Abgr8888,
    Nv12,
}

impl WriteFormat {
    /// `fb->format->cpp[0]` (DRM format info): 4 for every packed format here, 1 for the NV12 luma plane.
    pub const fn cpp(self) -> u32 {
        match self {
            WriteFormat::Nv12 => 1,
            _ => 4,
        }
    }
    /// `fb->format->has_alpha` - the A formats only.
    pub const fn has_alpha(self) -> bool {
        matches!(self, WriteFormat::Argb8888 | WriteFormat::Abgr8888)
    }
    pub const fn is_planar(self) -> bool {
        matches!(self, WriteFormat::Nv12)
    }
    /// `skl_plane_ctl_format()` — skl_universal_plane.c:958-1018.
    pub const fn ctl_format(self) -> u32 {
        match self {
            WriteFormat::Xbgr8888 | WriteFormat::Abgr8888 => PLANE_CTL_FORMAT_XRGB_8888 | PLANE_CTL_ORDER_RGBX,
            WriteFormat::Xrgb8888 | WriteFormat::Argb8888 => PLANE_CTL_FORMAT_XRGB_8888,                       
            WriteFormat::Nv12 => PLANE_CTL_FORMAT_NV12,                                                         
        }
    }
}

/// Framebuffer modifier, restricted to what is ported (`skl_plane_ctl_tiling`, skl_universal_plane.c:1056-1106).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteTiling {
    Linear,
    X,
    Y,
}

impl WriteTiling {
    pub const fn ctl(self) -> u32 {
        match self {
            WriteTiling::Linear => 0, // DRM_FORMAT_MOD_LINEAR: `break`, return 0
            WriteTiling::X => PLANE_CTL_TILED_X,
            WriteTiling::Y => PLANE_CTL_TILED_Y,
        }
    }
    /// `skl_plane_stride_mult()` for rotation 0 (skl_universal_plane.c:751-764): 64 for linear, the tile width in bytes otherwise
    /// (`intel_tile_width_bytes`: X 512, Y 128 — intel_fb.c:783-817, the same numbers `plane::stride_unit_bytes` decodes with).
    pub const fn stride_mult(self) -> u32 {
        match self {
            WriteTiling::Linear => 64,
            WriteTiling::X => 512,
            WriteTiling::Y => 128,
        }
    }
    /// `skl_plane_min_alignment()` for colour plane 0 — skl_universal_plane.c:630-651.
    pub const fn min_alignment(self) -> u32 {
        match self {
            WriteTiling::Linear | WriteTiling::X => 256 * 1024,
            WriteTiling::Y => 1024 * 1024,
        }
    }
    /// `skl_plane_max_width()` — skl_universal_plane.c:328-366 (cpp 8 never occurs for the formats here, so the else arms).
    pub const fn max_width(self) -> u32 {
        match self {
            WriteTiling::Linear | WriteTiling::X => 5120,
            WriteTiling::Y => 4096,
        }
    }
}

/// `drm_rotation` (only 0 is accepted; the other values exist so a request for them is a named refusal rather than a type error).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteRotation {
    R0,
    R90,
    R180,
    R270,
}

/// `pixel_blend_mode` (`DRM_MODE_BLEND_*`) — consulted by `skl_plane_ctl_alpha`, skl_universal_plane.c:1020-1036.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelBlend {
    None,
    Premulti,
    Coverage,
}

/// `hw.color_encoding` / `hw.color_range` for NV12 (`DRM_COLOR_YCBCR_BT709`, `DRM_COLOR_YCBCR_FULL_RANGE`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct YuvMatrix {
    pub bt709: bool,
    pub full_range: bool,
}

/// The framebuffer as the plane sees it (the "view": offsets within a GGTT mapping already resolved to GGTT addresses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneFb {
    pub format: WriteFormat,
    pub tiling: WriteTiling,
    /// Colour plane 0 pitch in bytes (`view.color_plane[0].scanout_stride`).
    pub stride_bytes: u32,
    /// `plane_state->surf` = GGTT address of colour plane 0 (intel_fb_pin.c:322-325).
    pub surf: u32,
    /// `view.color_plane[0].x/y` - the source origin inside the mapped surface, in pixels (`skl_plane_update_arm`).
    pub x: u32,
    pub y: u32,
    /// NV12 only: colour plane 1 (UV) pitch, GGTT address and origin. Ignored for the packed formats.
    pub uv_stride_bytes: u32,
    pub uv_surf: u32,
    pub uv_x: u32,
    pub uv_y: u32,
}

/// Everything `skl_plane_ctl` + the update functions read, for one plane of one pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneConfig {
    pub pipe: Pipe,
    /// 0 = PLANE_1 (the primary) .. 2 = PLANE_3.
    pub plane_id: u8,
    pub fb: PlaneFb,
    /// `drm_rect_width/height(&uapi.src) >> 16`.
    pub src_w: u32,
    pub src_h: u32,
    /// `uapi.dst.x1/y1`; the destination size must equal the source size (no scaler).
    pub dst_x: u32,
    pub dst_y: u32,
    pub dst_w: u32,
    pub dst_h: u32,
    pub rotation: WriteRotation,
    pub blend: PixelBlend,
    pub yuv: YuvMatrix,
    /// `hw.alpha >> 8` (0xff = opaque, the default `DRM_BLEND_ALPHA_OPAQUE`).
    pub plane_alpha: u8,
    /// `crtc_state->gamma_enable` / `csc_enable` (`skl_plane_ctl_crtc`, skl_universal_plane.c:1168-1183).
    pub pipe_gamma_enable: bool,
    pub pipe_csc_enable: bool,
}

/// Named refusals. `name()` strings are stable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneError {
    NoSuchPlane { plane_id: u8 },
    RotationNotPorted,
    ScalerNotPorted { src_w: u32, src_h: u32, dst_w: u32, dst_h: u32 },
    EmptySource,
    /// `skl_plane_max_width/height`.
    TooLarge { w: u32, h: u32 },
    /// `skl_plane_has_planar` false for this pipe/plane, or an NV12 source with an odd dimension / origin.
    PlanarNotAvailable { pipe: Pipe, plane_id: u8 },
    PlanarGeometry,
    /// Stride not a multiple of the unit, below `src_w * cpp`, above `skl_plane_max_stride`, or too big for the 12-bit field.
    BadStride { stride_bytes: u32 },
    /// `PLANE_SURF` address below the alignment `skl_plane_min_alignment` requires.
    SurfMisaligned { surf: u32, need: u32 },
    /// NV12: UV surface below the luma surface (the distance field is unsigned) or misaligned.
    BadAux,
}

impl PlaneError {
    pub const fn name(&self) -> &'static str {
        match self {
            PlaneError::NoSuchPlane { .. } => "PLANE-NO-SUCH-PLANE",
            PlaneError::RotationNotPorted => "PLANE-ROTATION-NOT-PORTED",
            PlaneError::ScalerNotPorted { .. } => "PLANE-SCALER-NOT-PORTED",
            PlaneError::EmptySource => "PLANE-EMPTY-SOURCE",
            PlaneError::TooLarge { .. } => "PLANE-TOO-LARGE",
            PlaneError::PlanarNotAvailable { .. } => "PLANE-PLANAR-NOT-AVAILABLE",
            PlaneError::PlanarGeometry => "PLANE-PLANAR-GEOMETRY",
            PlaneError::BadStride { .. } => "PLANE-BAD-STRIDE",
            PlaneError::SurfMisaligned { .. } => "PLANE-SURF-MISALIGNED",
            PlaneError::BadAux => "PLANE-BAD-AUX",
        }
    }
}

/// `skl_plane_max_height()` — skl_universal_plane.c:436-441.
pub const PLANE_MAX_HEIGHT: u32 = 4096;
/// `skl_plane_max_stride()` limits — skl_universal_plane.c:496-503 (`max_pixels` 8192, `max_bytes` 32 KiB).
pub const PLANE_MAX_STRIDE_PIXELS: u32 = 8192;
pub const PLANE_MAX_STRIDE_BYTES: u32 = 32 * 1024;
/// `skl_plane_min_alignment()` for colour plane != 0 (the AUX_DIST plane) — skl_universal_plane.c:635-637.
pub const PLANE_AUX_ALIGNMENT: u32 = 4 * 1024;

/// A short fixed list of `(register offset, value)` writes, in the order Linux performs them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegWrites<const N: usize> {
    items: [(u32, u32); N],
    len: usize,
}

impl<const N: usize> RegWrites<N> {
    pub const fn new() -> Self {
        RegWrites { items: [(0, 0); N], len: 0 }
    }
    /// Append. `N` is sized to the longest sequence, so a full list is a bug, not a runtime case.
    pub fn push(&mut self, reg: u32, value: u32) {
        assert!(self.len < N, "RegWrites capacity");
        self.items[self.len] = (reg, value);
        self.len += 1;
    }
    pub fn as_slice(&self) -> &[(u32, u32)] {
        &self.items[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl<const N: usize> Default for RegWrites<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// The plane's register words (`skl_plane_update_noarm` + `skl_plane_update_arm`), already encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneRegs {
    pub pipe: Pipe,
    pub plane_id: u8,
    /// `plane_state->ctl | skl_plane_ctl_crtc(crtc_state)` — the value `skl_plane_update_arm` writes.
    pub ctl: u32,
    pub stride: u32,
    pub pos: u32,
    pub size: u32,
    pub keyval: u32,
    pub keymsk: u32,
    pub keymax: u32,
    pub offset: u32,
    pub aux_dist: u32,
    pub aux_offset: u32,
    pub surf: u32,
}

/// `skl_plane_ctl_alpha()` — skl_universal_plane.c:1020-1036.
pub const fn ctl_alpha(has_alpha: bool, blend: PixelBlend) -> u32 {
    if !has_alpha {
        return PLANE_CTL_ALPHA_DISABLE;
    }
    match blend {
        PixelBlend::None => PLANE_CTL_ALPHA_DISABLE,
        PixelBlend::Premulti => PLANE_CTL_ALPHA_SW_PREMULTIPLY,
        PixelBlend::Coverage => PLANE_CTL_ALPHA_HW_PREMULTIPLY,
    }
}

/// `skl_plane_has_planar()` for Kaby Lake (not Skylake / Broxton, so no WA #0870) — skl_universal_plane.c:2469-2480.
pub const fn plane_has_planar(pipe: Pipe, plane_id: u8) -> bool {
    !matches!(pipe, Pipe::C) && (plane_id == 0 || plane_id == 1)
}

/// `skl_plane_ctl()` for display ver < 10 plus `skl_plane_ctl_crtc()` — skl_universal_plane.c:1185-1224, :1168-1183.
fn plane_ctl(c: &PlaneConfig) -> u32 {
    let mut ctl = PLANE_CTL_ENABLE;
    // `DISPLAY_VER(display) < 10` block.
    ctl |= ctl_alpha(c.fb.format.has_alpha(), c.blend);
    ctl |= PLANE_CTL_PLANE_GAMMA_DISABLE;
    if c.yuv.bt709 {
        ctl |= PLANE_CTL_YUV_TO_RGB_CSC_FORMAT_BT709;
    }
    if c.yuv.full_range {
        ctl |= PLANE_CTL_YUV_RANGE_CORRECTION_DISABLE;
    }
    ctl |= c.fb.format.ctl_format();
    ctl |= c.fb.tiling.ctl();
    ctl |= PLANE_CTL_ROTATE_0; // `skl_plane_ctl_rotate(DRM_MODE_ROTATE_0)` = 0; only 0 is accepted
    // `ckey.flags == 0`: no key-enable bits. `DISPLAY_VER(display) >= 11` / `== 13` arms do not apply.
    if c.pipe_gamma_enable {
        ctl |= PLANE_CTL_PIPE_GAMMA_ENABLE;
    }
    if c.pipe_csc_enable {
        ctl |= PLANE_CTL_PIPE_CSC_ENABLE;
    }
    ctl
}

/// `field_prep` over a 16-bit-pair register: `hi << 16 | lo`, each half checked to fit.
const fn pair(hi: u32, lo: u32) -> Option<u32> {
    if hi > 0xffff || lo > 0xffff {
        None
    } else {
        Some((hi << 16) | lo)
    }
}

/// Validate a configuration against the checks Linux makes before it programs the plane (`skl_plane_check`, `skl_plane_check_fb`,
/// `skl_plane_max_*`, `skl_plane_min_alignment`, `intel_plane_check_src_coordinates` for the YUV hsub/vsub rule) and encode it.
pub fn encode_plane(c: &PlaneConfig) -> Result<PlaneRegs, PlaneError> {
    if c.plane_id > 2 {
        return Err(PlaneError::NoSuchPlane { plane_id: c.plane_id });
    }
    if c.rotation != WriteRotation::R0 {
        return Err(PlaneError::RotationNotPorted);
    }
    if c.src_w == 0 || c.src_h == 0 {
        return Err(PlaneError::EmptySource);
    }
    if c.dst_w != c.src_w || c.dst_h != c.src_h {
        return Err(PlaneError::ScalerNotPorted { src_w: c.src_w, src_h: c.src_h, dst_w: c.dst_w, dst_h: c.dst_h });
    }
    if c.src_w > c.fb.tiling.max_width() || c.src_h > PLANE_MAX_HEIGHT {
        return Err(PlaneError::TooLarge { w: c.src_w, h: c.src_h });
    }
    let planar = c.fb.format.is_planar();
    if planar {
        if !plane_has_planar(c.pipe, c.plane_id) {
            return Err(PlaneError::PlanarNotAvailable { pipe: c.pipe, plane_id: c.plane_id });
        }
        // NV12 subsamples 2x2: source size and origin must be even (DRM `drm_format_info` hsub/vsub = 2).
        if (c.src_w | c.src_h | c.fb.x | c.fb.y) & 1 != 0 {
            return Err(PlaneError::PlanarGeometry);
        }
    }

    // Stride (`skl_plane_stride`, :766-777): bytes / mult, in a 12-bit field; `skl_plane_max_stride` bounds the bytes.
    let mult = c.fb.tiling.stride_mult();
    let cpp = c.fb.format.cpp();
    let stride_ok = |bytes: u32, width_px: u32| -> bool {
        bytes != 0
            && bytes % mult == 0
            && bytes >= width_px * cpp
            && bytes <= core::cmp::min(PLANE_MAX_STRIDE_PIXELS * cpp, PLANE_MAX_STRIDE_BYTES)
            && bytes / mult <= PLANE_STRIDE_MASK
    };
    if !stride_ok(c.fb.stride_bytes, c.src_w) {
        return Err(PlaneError::BadStride { stride_bytes: c.fb.stride_bytes });
    }
    let stride = (c.fb.stride_bytes / mult) & PLANE_STRIDE_MASK;

    let need = c.fb.tiling.min_alignment();
    if c.fb.surf % need != 0 {
        return Err(PlaneError::SurfMisaligned { surf: c.fb.surf, need });
    }

    // `skl_plane_aux_dist`: NV12 on ver < 11 has aux plane 1; the distance is `uv - y`, plus the UV stride field.
    let (aux_dist, aux_offset) = if planar {
        if c.fb.uv_surf < c.fb.surf || c.fb.uv_surf % PLANE_AUX_ALIGNMENT != 0 {
            return Err(PlaneError::BadAux);
        }
        let uv_ok = c.fb.uv_stride_bytes != 0
            && c.fb.uv_stride_bytes % mult == 0
            && c.fb.uv_stride_bytes >= c.src_w
            && c.fb.uv_stride_bytes / mult <= PLANE_AUX_STRIDE_MASK;
        if !uv_ok {
            return Err(PlaneError::BadStride { stride_bytes: c.fb.uv_stride_bytes });
        }
        let dist = c.fb.uv_surf - c.fb.surf;
        if dist & !PLANE_AUX_DISTANCE_MASK_BITS != 0 {
            return Err(PlaneError::BadAux);
        }
        (dist | ((c.fb.uv_stride_bytes / mult) & PLANE_AUX_STRIDE_MASK), pair(c.fb.uv_y, c.fb.uv_x).ok_or(PlaneError::BadAux)?)
    } else {
        // `skl_main_to_aux_plane` = 0 for a non-planar format: `skl_plane_aux_dist` returns 0, and colour plane 1
        // is empty so PLANE_AUX_OFFSET is `PLANE_OFFSET_Y(0) | PLANE_OFFSET_X(0)` (`skl_plane_update_arm`).
        (0, 0)
    };

    let pos = pair(c.dst_y, c.dst_x).ok_or(PlaneError::TooLarge { w: c.dst_x, h: c.dst_y })?;
    let size = pair(c.src_h - 1, c.src_w - 1).ok_or(PlaneError::TooLarge { w: c.src_w, h: c.src_h })?;
    let offset = pair(c.fb.y, c.fb.x).ok_or(PlaneError::TooLarge { w: c.fb.x, h: c.fb.y })?;

    // skl_plane_keyval/keymax/keymsk with `ckey` all zero: `alpha = hw.alpha >> 8`.
    let keyval = 0;
    let keymax = (c.plane_alpha as u32) << PLANE_KEYMAX_ALPHA_SHIFT; // (key.max_value & 0xffffff) is 0 with no key
    let keymsk = if c.plane_alpha < 0xff { PLANE_KEYMSK_ALPHA_ENABLE } else { 0 };

    Ok(PlaneRegs {
        pipe: c.pipe,
        plane_id: c.plane_id,
        ctl: plane_ctl(c),
        stride,
        pos,
        size,
        keyval,
        keymsk,
        keymax,
        offset,
        aux_dist,
        aux_offset,
        surf: c.fb.surf,
    })
}

/// `PLANE_AUX_DISTANCE_MASK` = `REG_GENMASK(31, 12)` — skl_universal_plane_regs.h:209.
const PLANE_AUX_DISTANCE_MASK_BITS: u32 = 0xffff_f000; // skl_universal_plane_regs.h:209

impl PlaneRegs {
    fn reg(&self, base_1a: u32) -> u32 {
        plane_reg(base_1a, self.pipe, self.plane_id as u32)
    }
    pub fn ctl_reg(&self) -> u32 {
        self.reg(PLANE_CTL_1_A)
    }
    pub fn stride_reg(&self) -> u32 {
        self.reg(PLANE_STRIDE_1_A)
    }
    pub fn pos_reg(&self) -> u32 {
        self.reg(PLANE_POS_1_A)
    }
    pub fn size_reg(&self) -> u32 {
        self.reg(PLANE_SIZE_1_A)
    }
    pub fn offset_reg(&self) -> u32 {
        self.reg(PLANE_OFFSET_1_A)
    }
    pub fn surf_reg(&self) -> u32 {
        self.reg(PLANE_SURF_1_A)
    }
    pub fn surflive_reg(&self) -> u32 {
        self.reg(PLANE_SURFLIVE_1_A)
    }

    /// `skl_plane_update_noarm()` WITHOUT its trailing `skl_write_plane_wm()` (the watermark block is `skl_wm`'s): stride, pos, size
    /// — skl_universal_plane.c:1404-1432. These registers do not take effect until the arm write.
    pub fn noarm_writes(&self) -> RegWrites<3> {
        let mut w = RegWrites::new();
        w.push(self.stride_reg(), self.stride);
        w.push(self.pos_reg(), self.pos);
        w.push(self.size_reg(), self.size);
        w
    }

    /// `skl_plane_update_arm()` for display ver 9 (no `PLANE_COLOR_CTL`, no scaler) — skl_universal_plane.c:1435-1498. The order is
    /// the protocol: keys, offset, aux, then `PLANE_CTL`, then `PLANE_SURF` LAST ("The control register self-arms if the plane
    /// was previously disabled. Try to make the plane enable atomic by writing the control register just before the surface
    /// register", :1487-1491). The surface write is the latch.
    pub fn arm_writes(&self) -> RegWrites<8> {
        let mut w = RegWrites::new();
        w.push(self.reg(PLANE_KEYVAL_1_A), self.keyval);
        w.push(self.reg(PLANE_KEYMSK_1_A), self.keymsk);
        w.push(self.reg(PLANE_KEYMAX_1_A), self.keymax);
        w.push(self.offset_reg(), self.offset);
        w.push(self.reg(PLANE_AUX_DIST_1_A), self.aux_dist);
        w.push(self.reg(PLANE_AUX_OFFSET_1_A), self.aux_offset);
        w.push(self.ctl_reg(), self.ctl);
        w.push(self.surf_reg(), self.surf); // - the latch, last
        w
    }
}

/// `skl_plane_disable_arm()`'s plane half: `PLANE_CTL = 0`, `PLANE_SURF = 0` (skl_universal_plane.c:867-879) — the watermark write
/// that precedes them is `skl_wm`'s.
pub fn disable_writes(pipe: Pipe, plane_id: u8) -> RegWrites<2> {
    let mut w = RegWrites::new();
    w.push(plane_reg(PLANE_CTL_1_A, pipe, plane_id as u32), 0);
    w.push(plane_reg(PLANE_SURF_1_A, pipe, plane_id as u32), 0);
    w
}
