// SPDX-License-Identifier: GPL-2.0-only
//! Plane register values for a 1920x1080 XRGB8888 linear framebuffer (and the neighbouring formats / tilings) against the numbers
//! GCC computes from Linux's own `skl_universal_plane_regs.h` macros combined exactly as `skl_plane_ctl()`,
//! `skl_plane_update_noarm()` and `skl_plane_update_arm()` combine them (display version 9). Expected values are Linux's, written
//! out literally; the actual side travels through the crate.
use i915_kbl_display_core::skl_plane::*;
use i915_kbl_display_core::types::Pipe;

fn cfg(format: WriteFormat, tiling: WriteTiling) -> PlaneConfig {
    PlaneConfig {
        pipe: Pipe::A,
        plane_id: 0,
        fb: PlaneFb {
            format,
            tiling,
            stride_bytes: 7680,
            surf: 0x0040_0000,
            x: 0,
            y: 0,
            uv_stride_bytes: 0,
            uv_surf: 0,
            uv_x: 0,
            uv_y: 0,
        },
        src_w: 1920,
        src_h: 1080,
        dst_x: 0,
        dst_y: 0,
        dst_w: 1920,
        dst_h: 1080,
        rotation: WriteRotation::R0,
        blend: PixelBlend::None,
        yuv: YuvMatrix { bt709: false, full_range: false },
        plane_alpha: 0xff,
        pipe_gamma_enable: false,
        pipe_csc_enable: false,
    }
}

#[test]
fn xrgb8888_linear_1080p_pipe_a_matches_linux() {
    let r = encode_plane(&cfg(WriteFormat::Xrgb8888, WriteTiling::Linear)).unwrap();
    assert_eq!(r.ctl, 0x8400_2000); // ENABLE | PLANE_GAMMA_DISABLE | FORMAT_XRGB_8888 | linear | ROTATE_0
    assert_eq!(r.stride, 0x78); // PLANE_STRIDE_(7680 / 64)
    assert_eq!(r.pos, 0);
    assert_eq!(r.size, 0x0437_077f); // PLANE_HEIGHT(1079) | PLANE_WIDTH(1919)
    assert_eq!(r.offset, 0);
    assert_eq!(r.keyval, 0);
    assert_eq!(r.keymsk, 0);
    assert_eq!(r.keymax, 0xff00_0000); // PLANE_KEYMAX_ALPHA(0xff)
    assert_eq!(r.aux_dist, 0);
    assert_eq!(r.aux_offset, 0);
    assert_eq!(r.surf, 0x0040_0000);
    // register addresses (PLANE_*(PIPE_A, PLANE_1))
    assert_eq!(r.ctl_reg(), 0x70180);
    assert_eq!(r.stride_reg(), 0x70188);
    assert_eq!(r.pos_reg(), 0x7018c);
    assert_eq!(r.size_reg(), 0x70190);
    assert_eq!(r.offset_reg(), 0x701a4);
    assert_eq!(r.surf_reg(), 0x7019c);
    assert_eq!(r.surflive_reg(), 0x701ac);
}

#[test]
fn write_order_is_noarm_then_arm_with_surf_last() {
    let r = encode_plane(&cfg(WriteFormat::Xrgb8888, WriteTiling::Linear)).unwrap();
    // skl_plane_update_noarm: STRIDE, POS, SIZE
    let noarm: Vec<u32> = r.noarm_writes().as_slice().iter().map(|w| w.0).collect();
    assert_eq!(noarm, [0x70188, 0x7018c, 0x70190]);
    // skl_plane_update_arm: KEYVAL, KEYMSK, KEYMAX, OFFSET, AUX_DIST, AUX_OFFSET, CTL, SURF - surf is the latch, written LAST.
    let arm: Vec<u32> = r.arm_writes().as_slice().iter().map(|w| w.0).collect();
    assert_eq!(arm, [0x70194, 0x70198, 0x701a0, 0x701a4, 0x701c0, 0x701c4, 0x70180, 0x7019c]);
    assert_eq!(*arm.last().unwrap(), r.surf_reg());
    assert_eq!(arm[arm.len() - 2], r.ctl_reg(), "PLANE_CTL is written just before PLANE_SURF");
}

#[test]
fn alpha_blend_and_order_variants() {
    let mut c = cfg(WriteFormat::Argb8888, WriteTiling::Linear);
    assert_eq!(encode_plane(&c).unwrap().ctl, 0x8400_2000, "ARGB with BLEND_PIXEL_NONE: PLANE_CTL_ALPHA_DISABLE");
    c.blend = PixelBlend::Premulti;
    assert_eq!(encode_plane(&c).unwrap().ctl, 0x8400_2020, "SW premultiply");
    c.blend = PixelBlend::Coverage;
    assert_eq!(encode_plane(&c).unwrap().ctl, 0x8400_2030, "HW premultiply");
    // XRGB ignores the blend mode (has_alpha is false).
    let mut x = cfg(WriteFormat::Xrgb8888, WriteTiling::Linear);
    x.blend = PixelBlend::Coverage;
    assert_eq!(encode_plane(&x).unwrap().ctl, 0x8400_2000);
    // XBGR: PLANE_CTL_ORDER_RGBX (bit 20)
    assert_eq!(encode_plane(&cfg(WriteFormat::Xbgr8888, WriteTiling::Linear)).unwrap().ctl, 0x8410_2000);
}

#[test]
fn tiling_and_crtc_bits() {
    let x = encode_plane(&cfg(WriteFormat::Xrgb8888, WriteTiling::X)).unwrap();
    assert_eq!(x.ctl, 0x8400_2400);
    assert_eq!(x.stride, 15, "7680 / 512 (X tile width in bytes)");
    let mut y = cfg(WriteFormat::Xrgb8888, WriteTiling::Y);
    y.fb.surf = 0x0010_0000; // Y tiling needs 1 MiB alignment
    let y = encode_plane(&y).unwrap();
    assert_eq!(y.ctl, 0x8400_3000);
    assert_eq!(y.stride, 60, "7680 / 128 (Y tile width in bytes)");
    let mut c = cfg(WriteFormat::Xrgb8888, WriteTiling::Linear);
    c.pipe_gamma_enable = true;
    assert_eq!(encode_plane(&c).unwrap().ctl, 0xc400_2000);
    c.pipe_gamma_enable = false;
    c.pipe_csc_enable = true;
    assert_eq!(encode_plane(&c).unwrap().ctl, 0x8480_2000);
}

#[test]
fn position_offset_and_pipe_b_plane_2_registers() {
    let mut c = cfg(WriteFormat::Xrgb8888, WriteTiling::Linear);
    c.src_w = 800;
    c.src_h = 600;
    c.dst_w = 800;
    c.dst_h = 600;
    c.dst_x = 9;
    c.dst_y = 7;
    c.fb.x = 5;
    c.fb.y = 3;
    c.fb.stride_bytes = 3200;
    let r = encode_plane(&c).unwrap();
    assert_eq!(r.pos, 0x0007_0009); // PLANE_POS_Y(7) | PLANE_POS_X(9)
    assert_eq!(r.offset, 0x0003_0005); // PLANE_OFFSET_Y(3) | PLANE_OFFSET_X(5)
    c.pipe = Pipe::B;
    c.plane_id = 1;
    let r = encode_plane(&c).unwrap();
    assert_eq!(r.ctl_reg(), 0x71280);
    assert_eq!(r.surf_reg(), 0x7129c);
}

#[test]
fn nv12_aux_registers() {
    let mut c = cfg(WriteFormat::Nv12, WriteTiling::Linear);
    c.fb.stride_bytes = 1920;
    c.fb.uv_stride_bytes = 1920;
    c.fb.uv_surf = 0x0040_0000 + 0x1f_b000; // the 1920 * 1080 luma plane (0x1fa400 bytes) rounded up to 4 KiB
    c.fb.uv_x = 4;
    c.fb.uv_y = 2;
    let r = encode_plane(&c).unwrap();
    assert_eq!(r.ctl, 0x8100_2000, "ENABLE | PLANE_GAMMA_DISABLE | NV12 (1 << 24), no BT.709 / range bits");
    assert_eq!(r.aux_dist, 0x001f_b000 | (1920 / 64), "PLANE_AUX_DIST = uv - y | PLANE_AUX_STRIDE(uv stride / 64)");
    assert_eq!(r.aux_offset, 0x0002_0004, "PLANE_OFFSET_Y(2) | PLANE_OFFSET_X(4)");
    c.yuv = YuvMatrix { bt709: true, full_range: true };
    assert_eq!(encode_plane(&c).unwrap().ctl, 0x9104_2000, "ENABLE | PLANE_GAMMA_DISABLE | NV12 | BT709 | RANGE_CORRECTION_DISABLE");
}

#[test]
fn named_refusals() {
    let base = cfg(WriteFormat::Xrgb8888, WriteTiling::Linear);
    let mut c = base;
    c.rotation = WriteRotation::R90;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-ROTATION-NOT-PORTED");
    for rot in [WriteRotation::R180, WriteRotation::R270] {
        let mut c = base;
        c.rotation = rot;
        assert_eq!(encode_plane(&c).unwrap_err(), PlaneError::RotationNotPorted);
    }
    let mut c = base;
    c.dst_w = 1280;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-SCALER-NOT-PORTED");
    let mut c = base;
    c.plane_id = 3;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-NO-SUCH-PLANE");
    let mut c = base;
    c.fb.surf = 0x0040_1000;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-SURF-MISALIGNED");
    let mut c = base;
    c.fb.stride_bytes = 7690;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-BAD-STRIDE");
    let mut c = base;
    c.fb.stride_bytes = 7040; // 1760 px: below src_w * cpp
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-BAD-STRIDE");
    let mut c = base;
    c.src_w = 5121;
    c.dst_w = 5121;
    c.fb.stride_bytes = 20736;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-TOO-LARGE");
    let mut c = cfg(WriteFormat::Nv12, WriteTiling::Linear);
    c.pipe = Pipe::C;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-PLANAR-NOT-AVAILABLE");
    let mut c = cfg(WriteFormat::Nv12, WriteTiling::Linear);
    c.plane_id = 2;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-PLANAR-NOT-AVAILABLE");
    let mut c = cfg(WriteFormat::Nv12, WriteTiling::Linear);
    c.src_w = 1919;
    c.dst_w = 1919;
    assert_eq!(encode_plane(&c).unwrap_err().name(), "PLANE-PLANAR-GEOMETRY");
}

#[test]
fn disable_writes_clear_ctl_then_surf() {
    let w = disable_writes(Pipe::A, 0);
    assert_eq!(w.as_slice(), &[(0x70180, 0), (0x7019c, 0)]);
}
