// SPDX-License-Identifier: GPL-2.0-only
//! The image a PRIME grant carries (wire ABI 1.6 `PRIME-GRANT-IMAGE` / `PRIME-REDEEM-IMAGE`): is the stated image inside the object it
//! names, and is it the image the recipient expects. Linux makes these checks when a dma-buf is turned into a framebuffer, not when it
//! is shared: `drm_internal_framebuffer_create` -> `framebuffer_check` (drm_framebuffer.c:155-251), the GEM size floor
//! (`drm_gem_fb_init_with_funcs`, drm_gem_framebuffer_helper.c:177-195), and i915's `intel_framebuffer_init` (display/intel_fb.c:2208-
//! 2290) with `intel_fb_bo_framebuffer_init` (display/intel_fb_bo.c). Here the consumer (the compositor) is another process, so the
//! device makes them for it, once at grant and again at redeem (a tiling change in between is caught), and then compares the granted
//! image with the one the recipient states: a mismatch is refused by name, never coerced.
//!
//! Formats are the `drm_format_info` rows of the formats a grant may name (drm_fourcc.c:227-278); modifiers are linear, X and Y
//! (drm_fourcc.h:544, :573, :591). Display-version facts are Gen9's (KBL) and Gen8's (CHV), which agree on every value used here.
use crate::gem::Gem;
use crate::ids::ObjId;
use i915_uapi_core::consts::misc as m;

/// An image in `drm_mode_fb_cmd2` terms (include/uapi/drm/drm_mode.h), all planes in one object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageLayout {
    pub fourcc: u32,
    pub width: u32,
    pub height: u32,
    pub planes: u32,
    pub pitches: [u32; 4],
    pub offsets: [u32; 4],
    pub modifier: u64,
}

/// Why an image was refused. Every one is `-EINVAL` (Linux's answer to each of these checks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageRefusal {
    /// Not a format a grant may name (`drm_get_format_info` -> NULL in `drm_internal_framebuffer_create`).
    Format,
    /// `planes` is not the format's plane count.
    Planes,
    /// Zero width or height (drm_framebuffer.c:161-169).
    Size,
    /// Not linear, X or Y tiled (`drm_any_plane_has_format`, intel_fb.c:2236-2244).
    Modifier,
    /// `height * pitch + offset` does not fit 32 bits (drm_framebuffer.c:187-191).
    Range,
    /// A pitch smaller than a row of the plane (drm_framebuffer.c:193-196).
    Pitch,
    /// A pitch that is not a multiple of the stride alignment (`intel_fb_stride_alignment`, intel_fb.c:1994-2030, :2278-2284).
    PitchAlign,
    /// Plane 0 does not start at the object's first byte (intel_fb.c:2258-2264).
    Offset,
    /// A plane past `planes` carries a pitch or offset (drm_framebuffer.c:229-248).
    UnusedPlane,
    /// The plane does not fit the object (drm_gem_framebuffer_helper.c:186-195).
    ObjectTooSmall,
    /// The object is tiled, and not as the modifier says (intel_fb_bo.c `intel_fb_bo_framebuffer_init`).
    Tiling,
    /// The object is tiled and the pitch is not its fence stride (intel_fb_bo.c `intel_fb_bo_framebuffer_init`).
    TilingStride,
    /// The object named is gone.
    NoObject,
}

impl ImageRefusal {
    pub const fn name(self) -> &'static str {
        match self {
            ImageRefusal::Format => "PRIME-IMAGE-FORMAT",
            ImageRefusal::Planes => "PRIME-IMAGE-PLANES",
            ImageRefusal::Size => "PRIME-IMAGE-SIZE",
            ImageRefusal::Modifier => "PRIME-IMAGE-MODIFIER",
            ImageRefusal::Range => "PRIME-IMAGE-RANGE",
            ImageRefusal::Pitch => "PRIME-IMAGE-PITCH",
            ImageRefusal::PitchAlign => "PRIME-IMAGE-PITCH-ALIGN",
            ImageRefusal::Offset => "PRIME-IMAGE-OFFSET",
            ImageRefusal::UnusedPlane => "PRIME-IMAGE-UNUSED-PLANE",
            ImageRefusal::ObjectTooSmall => "PRIME-IMAGE-OBJECT-TOO-SMALL",
            ImageRefusal::Tiling => "PRIME-IMAGE-TILING",
            ImageRefusal::TilingStride => "PRIME-IMAGE-TILING-STRIDE",
            ImageRefusal::NoObject => "PRIME-IMAGE-NO-OBJECT",
        }
    }
}

// The codes are the wire's vocabulary (i915_wire::msg::prime, the values of drm_fourcc.h:105, :193-199, :332, :468, :544, :573, :591).
pub use i915_wire::msg::prime::fourcc::{ABGR8888 as DRM_FORMAT_ABGR8888, ARGB8888 as DRM_FORMAT_ARGB8888, NV12 as DRM_FORMAT_NV12};
pub use i915_wire::msg::prime::fourcc::{XBGR8888 as DRM_FORMAT_XBGR8888, XRGB8888 as DRM_FORMAT_XRGB8888};
pub use i915_wire::msg::prime::modifier::{I915_X_TILED as I915_FORMAT_MOD_X_TILED, I915_Y_TILED as I915_FORMAT_MOD_Y_TILED, LINEAR as DRM_FORMAT_MOD_LINEAR};

/// The `drm_format_info` fields the checks read.
#[derive(Clone, Copy)]
struct FormatInfo {
    num_planes: u32,
    cpp: [u32; 2],
    hsub: u32,
    vsub: u32,
}

/// `drm_format_info` (drm_fourcc.c:227, :228, :249, :250, :278).
fn format_info(fourcc: u32) -> Option<FormatInfo> {
    let rgb = FormatInfo { num_planes: 1, cpp: [4, 0], hsub: 1, vsub: 1 };
    match fourcc {
        DRM_FORMAT_XRGB8888 | DRM_FORMAT_XBGR8888 | DRM_FORMAT_ARGB8888 | DRM_FORMAT_ABGR8888 => Some(rgb),
        DRM_FORMAT_NV12 => Some(FormatInfo { num_planes: 2, cpp: [1, 2], hsub: 2, vsub: 2 }),
        _ => None,
    }
}

/// `intel_fb_modifier_to_tiling` (intel_fb.c:398-418) for the modifiers a grant may name; `None` for any other.
fn modifier_tiling(modifier: u64) -> Option<u32> {
    match modifier {
        DRM_FORMAT_MOD_LINEAR => Some(m::I915_TILING_NONE),
        I915_FORMAT_MOD_X_TILED => Some(m::I915_TILING_X),
        I915_FORMAT_MOD_Y_TILED => Some(m::I915_TILING_Y),
        _ => None,
    }
}

/// `intel_fb_stride_alignment` (intel_fb.c:1994-2030): 64 bytes linear (a pitch up to the plane's max stride), else the tile width
/// (`intel_tile_width_bytes`, intel_fb.c:775-815: X = 512 on display ver > 2, Y = 128 with `HAS_128B_Y_TILING`).
fn stride_alignment(modifier: u64) -> u32 {
    match modifier {
        I915_FORMAT_MOD_X_TILED => 512,
        I915_FORMAT_MOD_Y_TILED => 128,
        _ => 64,
    }
}

/// Is `img` an image the object (`size` bytes, fence `tiling`/`stride`) holds?
pub fn check_image(img: &ImageLayout, size: u64, tiling: u32, stride: u32) -> Result<(), ImageRefusal> {
    let info = format_info(img.fourcc).ok_or(ImageRefusal::Format)?;
    if img.planes != info.num_planes {
        return Err(ImageRefusal::Planes);
    }
    if img.width == 0 || img.height == 0 {
        return Err(ImageRefusal::Size);
    }
    let want_tiling = modifier_tiling(img.modifier).ok_or(ImageRefusal::Modifier)?;
    let align = stride_alignment(img.modifier);
    for i in 0..info.num_planes as usize {
        // drm_format_info_plane_width / _height (drm_fourcc.h:273-305): plane 0 whole, the others DIV_ROUND_UP by the subsampling
        let (w, h) = if i == 0 { (img.width, img.height) } else { (img.width.div_ceil(info.hsub), img.height.div_ceil(info.vsub)) };
        let (pitch, offset) = (img.pitches[i] as u64, img.offsets[i] as u64);
        let min_pitch = w as u64 * info.cpp[i] as u64; // drm_format_info_min_pitch (drm_fourcc.c:513-522), 1x1 blocks
        if h as u64 * pitch + offset > u32::MAX as u64 || min_pitch > u32::MAX as u64 {
            return Err(ImageRefusal::Range);
        }
        if pitch < min_pitch {
            return Err(ImageRefusal::Pitch);
        }
        if img.pitches[i] % align != 0 {
            return Err(ImageRefusal::PitchAlign);
        }
        if (h as u64 - 1) * pitch + min_pitch + offset > size {
            return Err(ImageRefusal::ObjectTooSmall);
        }
    }
    if img.offsets[0] != 0 {
        return Err(ImageRefusal::Offset);
    }
    for i in info.num_planes as usize..4 {
        if img.pitches[i] != 0 || img.offsets[i] != 0 {
            return Err(ImageRefusal::UnusedPlane);
        }
    }
    if tiling != m::I915_TILING_NONE {
        if tiling != want_tiling {
            return Err(ImageRefusal::Tiling);
        }
        if img.pitches[0] != stride {
            return Err(ImageRefusal::TilingStride);
        }
    }
    Ok(())
}

/// The first way `expected` differs from `granted`, by name: the recipient must expect exactly the granted image.
pub fn image_mismatch(granted: &ImageLayout, expected: &ImageLayout) -> Option<&'static str> {
    if granted.fourcc != expected.fourcc || granted.planes != expected.planes {
        Some("PRIME-FORMAT-MISMATCH")
    } else if (granted.width, granted.height) != (expected.width, expected.height) {
        Some("PRIME-SIZE-MISMATCH")
    } else if granted.pitches != expected.pitches {
        Some("PRIME-PITCH-MISMATCH")
    } else if granted.offsets != expected.offsets {
        Some("PRIME-OFFSET-MISMATCH")
    } else if granted.modifier != expected.modifier {
        Some("PRIME-MODIFIER-MISMATCH")
    } else {
        None
    }
}

impl Gem {
    /// [`check_image`] against a live object as it is NOW (its size, and the fence tiling and stride `GEM-SET-TILING` gave it).
    pub fn prime_image_check(&self, obj: ObjId, img: &ImageLayout) -> Result<(), ImageRefusal> {
        let o = self.obj(obj).filter(|o| o.handles > 0).ok_or(ImageRefusal::NoObject)?;
        check_image(img, o.size, o.tiling, o.stride)
    }
}
