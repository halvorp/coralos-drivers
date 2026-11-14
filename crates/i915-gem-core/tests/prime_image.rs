// SPDX-License-Identifier: GPL-2.0-only
//! The image a PRIME grant carries: Linux's framebuffer checks (drm_framebuffer.c `framebuffer_check`, drm_gem_framebuffer_helper.c
//! min_size, i915 intel_fb.c / intel_fb_bo.c) made at grant, and the exact-match rule at redeem. Expected values are the Linux
//! literals: the format rows of drm_fourcc.c (XRGB8888 cpp 4; NV12 cpp {1, 2}, hsub = vsub = 2), the 64-byte linear stride alignment,
//! X tiles 512 bytes wide, Y tiles 128.
mod common;
use common::*;
use i915_gem_core::prime_image::*;
use i915_uapi_core::consts::misc as m;

/// 1280x720 XRGB8888, linear, pitch 1280 * 4.
fn rgbx() -> ImageLayout {
    ImageLayout { fourcc: DRM_FORMAT_XRGB8888, width: 1280, height: 720, planes: 1, pitches: [5120, 0, 0, 0], offsets: [0; 4], modifier: DRM_FORMAT_MOD_LINEAR }
}

/// 1280x720 NV12, linear: Y at 0, CbCr (640x360 pairs of bytes) right after it.
fn nv12() -> ImageLayout {
    ImageLayout { fourcc: DRM_FORMAT_NV12, width: 1280, height: 720, planes: 2, pitches: [1280, 1280, 0, 0], offsets: [0, 1280 * 720, 0, 0], modifier: DRM_FORMAT_MOD_LINEAR }
}

const NONE: u32 = m::I915_TILING_NONE;

#[test]
fn a_well_formed_image_fits_its_object_exactly_and_one_byte_less_is_refused() {
    // min_size = (height - 1) * pitch + min_pitch + offset (drm_gem_framebuffer_helper.c:186-188)
    assert_eq!(check_image(&rgbx(), 719 * 5120 + 5120, NONE, 0), Ok(()));
    assert_eq!(check_image(&rgbx(), 719 * 5120 + 5119, NONE, 0), Err(ImageRefusal::ObjectTooSmall));
    // NV12: the CbCr plane is 360 rows of 1280 bytes (DIV_ROUND_UP(720, 2) rows; 640 * cpp 2 bytes a row) after the Y plane
    let end = 1280 * 720 + 359 * 1280 + 1280;
    assert_eq!(check_image(&nv12(), end, NONE, 0), Ok(()));
    assert_eq!(check_image(&nv12(), end - 1, NONE, 0), Err(ImageRefusal::ObjectTooSmall));
    // odd sizes round the chroma plane UP (drm_format_info_plane_height): 3x3 NV12 has 2 chroma rows of 2 * 2 bytes
    let odd = ImageLayout { width: 3, height: 3, pitches: [64, 64, 0, 0], offsets: [0, 192, 0, 0], ..nv12() };
    assert_eq!(check_image(&odd, 192 + 64 + 4, NONE, 0), Ok(()));
    assert_eq!(check_image(&odd, 192 + 64 + 3, NONE, 0), Err(ImageRefusal::ObjectTooSmall));
}

#[test]
fn each_malformed_image_is_refused_by_its_own_name() {
    let big = 64 << 20;
    let with = |f: &dyn Fn(&mut ImageLayout)| {
        let mut i = rgbx();
        f(&mut i);
        check_image(&i, big, NONE, 0)
    };
    assert_eq!(with(&|i| i.fourcc = u32::from_le_bytes(*b"YUYV")), Err(ImageRefusal::Format));
    assert_eq!(with(&|i| i.planes = 2), Err(ImageRefusal::Planes));
    assert_eq!(with(&|i| i.width = 0), Err(ImageRefusal::Size));
    assert_eq!(with(&|i| i.height = 0), Err(ImageRefusal::Size));
    assert_eq!(with(&|i| i.modifier = 1 << 56 | 4), Err(ImageRefusal::Modifier), "Y_TILED_CCS is not a modifier a grant may name");
    assert_eq!(with(&|i| i.pitches[0] = 5056), Err(ImageRefusal::Pitch), "a row of 1280 XRGB pixels is 5120 bytes");
    assert_eq!(with(&|i| i.pitches[0] = 5120 + 32), Err(ImageRefusal::PitchAlign), "linear pitches are 64-byte aligned");
    assert_eq!(with(&|i| i.pitches[0] = 5120 + 64), Ok(()));
    assert_eq!(with(&|i| i.offsets[0] = 4096), Err(ImageRefusal::Offset), "plane 0 offset must be 0 (intel_fb.c:2258-2264)");
    assert_eq!(with(&|i| i.pitches[1] = 64), Err(ImageRefusal::UnusedPlane));
    assert_eq!(with(&|i| i.offsets[3] = 64), Err(ImageRefusal::UnusedPlane));
    assert_eq!(with(&|i| i.height = 0x10_0000), Err(ImageRefusal::Range), "height * pitch must fit 32 bits");
    // the names are what the device prints
    assert_eq!(ImageRefusal::PitchAlign.name(), "PRIME-IMAGE-PITCH-ALIGN");
    assert_eq!(ImageRefusal::ObjectTooSmall.name(), "PRIME-IMAGE-OBJECT-TOO-SMALL");
}

#[test]
fn a_tiled_object_must_be_named_with_its_own_modifier_and_fence_stride() {
    let big = 64 << 20;
    let x = ImageLayout { modifier: I915_FORMAT_MOD_X_TILED, ..rgbx() };
    let y = ImageLayout { modifier: I915_FORMAT_MOD_Y_TILED, ..rgbx() };
    // 5120 is a multiple of the X tile (512) and the Y tile (128)
    assert_eq!(check_image(&x, big, m::I915_TILING_X, 5120), Ok(()));
    assert_eq!(check_image(&y, big, m::I915_TILING_Y, 5120), Ok(()));
    assert_eq!(check_image(&x, big, m::I915_TILING_Y, 5120), Err(ImageRefusal::Tiling));
    assert_eq!(check_image(&rgbx(), big, m::I915_TILING_X, 5120), Err(ImageRefusal::Tiling), "a fenced X object is not linear");
    assert_eq!(check_image(&x, big, m::I915_TILING_X, 10240), Err(ImageRefusal::TilingStride));
    // tile widths: X 512, Y 128
    let x_pitch = ImageLayout { pitches: [5120 + 128, 0, 0, 0], ..x };
    assert_eq!(check_image(&x_pitch, big, NONE, 0), Err(ImageRefusal::PitchAlign));
    let y_pitch = ImageLayout { pitches: [5120 + 128, 0, 0, 0], ..y };
    assert_eq!(check_image(&y_pitch, big, NONE, 0), Ok(()));
    // an unfenced object may carry a tiled modifier (intel_fb_bo.c enforces the match only when there is a fence)
    assert_eq!(check_image(&x, big, NONE, 0), Ok(()));
}

#[test]
fn the_recipient_must_expect_exactly_the_granted_image() {
    let g = nv12();
    assert_eq!(image_mismatch(&g, &g), None);
    assert_eq!(image_mismatch(&g, &rgbx()), Some("PRIME-FORMAT-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { planes: 1, ..g }), Some("PRIME-FORMAT-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { width: 1920, ..g }), Some("PRIME-SIZE-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { height: 1080, ..g }), Some("PRIME-SIZE-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { pitches: [1344, 1280, 0, 0], ..g }), Some("PRIME-PITCH-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { offsets: [0, 1280 * 736, 0, 0], ..g }), Some("PRIME-OFFSET-MISMATCH"));
    assert_eq!(image_mismatch(&g, &ImageLayout { modifier: I915_FORMAT_MOD_Y_TILED, ..g }), Some("PRIME-MODIFIER-MISMATCH"));
}

#[test]
fn the_check_reads_the_object_as_it_is_now() {
    let mut r = Rig::new();
    let h = r.create(4 << 20);
    let obj = r.gem.lookup_object(r.s, h).unwrap();
    let x = ImageLayout { modifier: I915_FORMAT_MOD_X_TILED, ..rgbx() };
    assert_eq!(r.gem.prime_image_check(obj, &rgbx()), Ok(()));
    r.gem.set_tiling(r.s, h, m::I915_TILING_X, 5120, &mut r.sink).unwrap();
    assert_eq!(r.gem.prime_image_check(obj, &rgbx()), Err(ImageRefusal::Tiling), "the fence went on after the image was stated");
    assert_eq!(r.gem.prime_image_check(obj, &x), Ok(()));
    let small = r.create(4096);
    let so = r.gem.lookup_object(r.s, small).unwrap();
    assert_eq!(r.gem.prime_image_check(so, &rgbx()), Err(ImageRefusal::ObjectTooSmall));
    r.gem.close(r.s, small, &mut r.sink).unwrap();
    assert_eq!(r.gem.prime_image_check(so, &rgbx()), Err(ImageRefusal::NoObject));
}
