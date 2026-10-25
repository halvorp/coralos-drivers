// SPDX-License-Identifier: GPL-2.0-only
//! The adoption predicate and the initial plane config: the Pixelbook Go's real firmware state (a 1920x1080 linear XRGB8888
//! framebuffer at GGTT 0 scaled by the pipe's panel fitter onto the 3840x2160 eDP panel - the 2026-10-16 Atlas ring:
//! `[I915-GEN9-READOUT w=3840 h=2160 stride=7680 trans=EDP]`, `src 1920x1080 surf=0 ctl=0x84002000`), the readout test's vector A,
//! every refusal by name, the one mode offered, and the Linux tables behind the extent.
use i915_display_modeset_core::initial::*;
use i915_kbl_display_core::plane::{PlaneFormat, Tiling};
use i915_kbl_display_core::readout::{read_display_state, ReadoutState};
use i915_kbl_display_core::types::{Pipe, Transcoder};
use i915_kbl_display_core::KBL_DISPLAY;
use std::collections::BTreeMap;

/// Vector A of the readout tests (eDP 1080p60 on pipe A, the primary XRGB8888 X-tiled at GGTT 0x100000, stride 15 tiles).
fn vector_a() -> BTreeMap<u32, u32> {
    [
        (0x45400u32, 0x3000_0003u32),
        (0x45404, 0xc000_000c),
        (0x42000, 0x8e00_0000),
        (0x45504, 0),
        (0x46010, 0xc000_0000),
        (0x6c058, 3),
        (0x6c05c, 1),
        (0x6c060, 1),
        (0x46000, 0x382),
        (0x6f400, 0x8203_0002),
        (0x7f008, 0x8000_0000),
        (0x64000, 0x8000_0002),
        (0x6f000, 0x0897_077f),
        (0x6f004, 0x0897_077f),
        (0x6f008, 0x0803_07d7),
        (0x6f00c, 0x0464_0437),
        (0x6f010, 0x0464_0437),
        (0x6f014, 0x0440_043b),
        (0x6001c, 0x077f_0437),
        (0x6f030, 0x7e69_9999),
        (0x6f034, 0x0080_0000),
        (0x6f040, 288_358),
        (0x6f044, 0x0008_0000),
        (0x70180, 0x8400_0400),
        (0x70188, 15),
        (0x70190, 0x0437_077f),
        (0x7019c, 0x0010_0005),
    ]
    .into_iter()
    .collect()
}

/// The Pixelbook Go as its firmware left it: vector A's link and power, a 3840x2160 eDP raster (4000x2222 total), the pipe source
/// 1920x1080, PLANE_CTL 0x84002000 (enabled, XRGB8888, linear, plane gamma disabled), stride 120 x 64 = 7680, PLANE_SURF 0, and
/// scaler 1 of pipe A enabled and bound to the PIPE over the whole panel.
fn atlas() -> BTreeMap<u32, u32> {
    let mut r = vector_a();
    for (k, v) in [
        (0x6f000u32, 0x0f9f_0effu32), // HTOTAL: 4000 total, 3840 active
        (0x6f004, 0x0f9f_0eff),
        (0x6f008, 0x0f4f_0f2f),
        (0x6f00c, 0x08ad_086f), // VTOTAL: 2222 total, 2160 active
        (0x6f010, 0x08ad_086f),
        (0x6f014, 0x0877_0872),
        (0x6001c, 0x077f_0437), // PIPESRC 1920x1080
        (0x70180, 0x8400_2000),
        (0x70188, 120),
        (0x70190, 0x0437_077f),
        (0x7019c, 0),
        (0x68180, 0x8000_0000), // PS_1A_CTRL: enabled, bound to the pipe
        (0x68170, 0),
        (0x68174, 0x0f00_0870), // PS_WIN_SZ 3840x2160
    ] {
        r.insert(k, v);
    }
    r
}

fn state(regs: &BTreeMap<u32, u32>) -> ReadoutState {
    let mut rd = |a: u32| regs.get(&a).copied().unwrap_or(0);
    read_display_state(&mut rd, &KBL_DISPLAY)
}

fn with(mut r: BTreeMap<u32, u32>, k: u32, v: u32) -> BTreeMap<u32, u32> {
    r.insert(k, v);
    r
}

const PAGE: u64 = 4096;

#[test]
fn the_pixelbook_go_firmware_scanout_is_adopted_as_it_is() {
    let a = adopt(&state(&atlas()), PAGE).unwrap();
    assert_eq!((a.pipe, a.transcoder), (Pipe::A, Transcoder::Edp));
    assert_eq!((a.fb.width, a.fb.height, a.fb.stride, a.fb.cpp), (1920, 1080, 7680, 4));
    assert_eq!((a.fb.format, a.fb.tiling, a.fb.base), (PlaneFormat::Xrgb8888, Tiling::Linear, 0));
    assert_eq!((a.fb.modifier, a.fb.fourcc), (0, 0x3432_5258), "DRM_FORMAT_MOD_LINEAR, 'XR24'");
    assert_eq!(a.fb.size, 7680 * 1080, "linear: intel_fb_align_height is the height itself");
    assert_eq!(a.extent, GgttExtent { start: 0, size: 0x7e_9000 });
    assert_eq!((a.active_w, a.active_h), (3840, 2160));
    assert!(a.scaled(), "the panel fitter carries 1920x1080 onto the 3840x2160 panel");
    let f = a.fitter.unwrap();
    assert_eq!((f.id, f.win_w, f.win_h), (0, 3840, 2160));
    assert!(a.refresh_millihz.is_some());
    assert_ne!(a.wells_on & 1, 0, "PW1 is on");
}

#[test]
fn vector_a_x_tiled_at_one_megabyte_is_adopted_with_its_extent() {
    let a = adopt(&state(&vector_a()), PAGE).unwrap();
    assert_eq!((a.fb.width, a.fb.height, a.fb.stride, a.fb.tiling), (1920, 1080, 15 * 512, Tiling::X));
    assert_eq!(a.fb.base, 0x10_0000, "PLANE_SURF 0x00100005: the low 12 bits are not the address");
    assert_eq!(a.fb.modifier, 0x0100_0000_0000_0001, "I915_FORMAT_MOD_X_TILED");
    assert_eq!(a.extent, GgttExtent { start: 0x10_0000, size: 7680 * 1080 });
    assert!(!a.scaled());
    assert_eq!((a.active_w, a.active_h), (1920, 1080));
}

#[test]
fn every_refusal_is_named() {
    let refused = |r: BTreeMap<u32, u32>| adopt(&state(&r), PAGE).unwrap_err();
    assert_eq!(refused(with(atlas(), 0x7f008, 0)), AdoptRefusal::NoActivePipe, "TRANSCONF off");
    assert_eq!(refused(with(atlas(), 0x70180, 0x0400_2000)), AdoptRefusal::PlaneDisabled);
    assert_eq!(refused(with(atlas(), 0x70180, 0x8100_2000)), AdoptRefusal::FormatNotRgb, "NV12");
    assert_eq!(refused(with(atlas(), 0x70180, 0x8400_2000 | 1 << 15)), AdoptRefusal::Compressed);
    assert_eq!(refused(with(atlas(), 0x70180, 0x8400_2002)), AdoptRefusal::Rotated, "180");
    assert_eq!(refused(with(atlas(), 0x701a4, 0x0000_0010)), AdoptRefusal::OffsetNonZero { x: 16, y: 0 });
    assert_eq!(refused(with(atlas(), 0x70188, 0)), AdoptRefusal::StrideUnknown);
    assert_eq!(
        refused(with(atlas(), 0x70190, 0x0437_063f)),
        AdoptRefusal::PlaneNotPipeSource { plane_w: 1600, plane_h: 1080, pos_x: 0, pos_y: 0, src_w: 1920, src_h: 1080 }
    );
    assert_eq!(
        refused(with(atlas(), 0x7018c, 0x0000_0008)),
        AdoptRefusal::PlaneNotPipeSource { plane_w: 1920, plane_h: 1080, pos_x: 8, pos_y: 0, src_w: 1920, src_h: 1080 },
        "a positioned plane"
    );
    assert_eq!(refused(with(atlas(), 0x68180, 0x8000_0000 | 1 << 25)), AdoptRefusal::PlaneScaler { scaler: 0 }, "PS_BINDING_PLANE(0)");
    assert_eq!(refused(with(atlas(), 0x68180, 0)), AdoptRefusal::UnscaledSizeMismatch { src_w: 1920, src_h: 1080, active_w: 3840, active_h: 2160 });
    assert_eq!(refused(with(atlas(), 0x68174, 0x0780_0438)), AdoptRefusal::UnscaledSizeMismatch { src_w: 1920, src_h: 1080, active_w: 3840, active_h: 2160 }, "the fitter's window must be the panel");
    // the names are the ABI of the log and of DISPLAY-QUERY
    let names = [
        (AdoptRefusal::NoActivePipe, "ADOPT-NO-ACTIVE-PIPE"),
        (AdoptRefusal::MultipleActivePipes { mask: 3 }, "ADOPT-MULTIPLE-ACTIVE-PIPES"),
        (AdoptRefusal::PlaneUnreadable, "ADOPT-PLANE-UNREADABLE"),
        (AdoptRefusal::PlaneDisabled, "ADOPT-PLANE-DISABLED"),
        (AdoptRefusal::FormatNotRgb, "ADOPT-FORMAT-NOT-RGB"),
        (AdoptRefusal::Compressed, "ADOPT-COMPRESSED"),
        (AdoptRefusal::Rotated, "ADOPT-ROTATED"),
        (AdoptRefusal::OffsetNonZero { x: 0, y: 0 }, "ADOPT-OFFSET-NONZERO"),
        (AdoptRefusal::StrideUnknown, "ADOPT-STRIDE-UNKNOWN"),
        (AdoptRefusal::PlaneNotPipeSource { plane_w: 0, plane_h: 0, pos_x: 0, pos_y: 0, src_w: 0, src_h: 0 }, "ADOPT-PLANE-NOT-PIPE-SOURCE"),
        (AdoptRefusal::PlaneScaler { scaler: 0 }, "ADOPT-PLANE-SCALER"),
        (AdoptRefusal::UnscaledSizeMismatch { src_w: 0, src_h: 0, active_w: 0, active_h: 0 }, "ADOPT-UNSCALED-SIZE-MISMATCH"),
        (AdoptRefusal::Interlaced, "ADOPT-INTERLACED"),
        (AdoptRefusal::ExtentOverflow, "ADOPT-EXTENT-OVERFLOW"),
    ];
    for (r, n) in names {
        assert_eq!(r.name(), n);
    }
}

#[test]
fn two_active_pipes_are_refused_with_their_mask() {
    // pipe B's transcoder (TRANSCODER_B, TRANSCONF 0x71008) enabled through its DDI function: the readout calls it active only with
    // PW2 on and a function enabled; a second active pipe is the refusal, whatever it scans out.
    let a = adopt(&state(&atlas()), PAGE).unwrap();
    let mut two = state(&atlas());
    two.pipes[1].state = two.pipes[0].state;
    assert_eq!(adopt(&two, PAGE).unwrap_err(), AdoptRefusal::MultipleActivePipes { mask: 0b011 });
    assert_eq!(a.pipe, Pipe::A, "the control: one active pipe is adopted");
}

#[test]
fn only_the_adopted_mode_is_offered_and_any_other_is_refused_by_name() {
    let a = adopt(&state(&atlas()), PAGE).unwrap();
    let m = a.mode();
    assert_eq!((m.width, m.height), (1920, 1080), "the source the clients draw, not the panel");
    assert_eq!(a.check_mode(m), Ok(()));
    let panel = OfferedMode { width: 3840, height: 2160, refresh_millihz: m.refresh_millihz };
    let e = a.check_mode(panel).unwrap_err();
    assert_eq!((e.name(), e.asked, e.adopted), ("MODE-NOT-ADOPTED", panel, m));
    let other_rate = OfferedMode { refresh_millihz: m.refresh_millihz + 1, ..m };
    assert!(a.check_mode(other_rate).is_err(), "the refresh is part of the mode");
}

#[test]
fn the_tile_tables_and_the_modifiers_are_linuxs() {
    // intel_fb.c:780-841 (display ver 9, HAS_128B_Y_TILING)
    assert_eq!(tile_width_bytes(Tiling::Linear, 4), Some(4096));
    assert_eq!(tile_width_bytes(Tiling::X, 4), Some(512));
    assert_eq!(tile_width_bytes(Tiling::Y, 4), Some(128));
    assert_eq!([1, 2, 4, 8, 16, 3].map(|c| tile_width_bytes(Tiling::Yf, c)), [Some(64), Some(128), Some(128), Some(256), Some(256), None]);
    assert_eq!(TILE_SIZE, 4096);
    // intel_fb_align_height = ALIGN(h, 4096 / tile width)
    assert_eq!(align_height(Tiling::Linear, 4, 1081), Some(1081));
    assert_eq!(align_height(Tiling::X, 4, 1081), Some(1088));
    assert_eq!(align_height(Tiling::Y, 4, 1080), Some(1088));
    assert_eq!(align_height(Tiling::Yf, 8, 1080), Some(1088));
    assert_eq!(align_height(Tiling::Yf, 1, 1080), Some(1088));
    // drm_fourcc.h:544, :573, :591, :606 - vendor INTEL is 0x01 in bits 63:56
    assert_eq!(DRM_FORMAT_MOD_LINEAR, 0);
    assert_eq!(I915_FORMAT_MOD_X_TILED, 0x0100_0000_0000_0001);
    assert_eq!(I915_FORMAT_MOD_Y_TILED, 0x0100_0000_0000_0002);
    assert_eq!(I915_FORMAT_MOD_YF_TILED, 0x0100_0000_0000_0003);
    assert_eq!([Tiling::Linear, Tiling::X, Tiling::Y, Tiling::Yf].map(modifier), [0, 0x0100_0000_0000_0001, 0x0100_0000_0000_0002, 0x0100_0000_0000_0003]);
    // fourcc_code('X','R','2','4') = 0x34325258 (DRM_FORMAT_XRGB8888, drm_fourcc.h:193); 'XB24', 'AR30', 'RG16'
    assert_eq!(fourcc(PlaneFormat::Xrgb8888, false, false), Some(0x3432_5258));
    assert_eq!(fourcc(PlaneFormat::Xrgb8888, true, false), Some(u32::from_le_bytes(*b"XB24")));
    assert_eq!(fourcc(PlaneFormat::Xrgb2101010, false, true), Some(u32::from_le_bytes(*b"AR30")));
    assert_eq!(fourcc(PlaneFormat::Rgb565, false, false), Some(u32::from_le_bytes(*b"RG16")));
    for f in [PlaneFormat::Nv12, PlaneFormat::Yuv422, PlaneFormat::P010, PlaneFormat::Xyuv, PlaneFormat::Indexed] {
        assert_eq!(fourcc(f, false, false), None, "{f:?} is not an RGB scanout format");
    }
}

#[test]
fn the_extent_rounds_like_initial_plane_vma() {
    let cfg = |base: u32, size: u64| InitialPlaneConfig { base, size, ..adopt(&state(&atlas()), PAGE).unwrap().fb };
    assert_eq!(cfg(0x1000, 0x1800).ggtt_extent(PAGE), Ok(GgttExtent { start: 0x1000, size: 0x2000 }));
    assert_eq!(cfg(0x1000, 0x1800).ggtt_extent(1 << 16), Ok(GgttExtent { start: 0x1000, size: 0xf000 }), "a 64 KiB region rounds the END up");
    assert_eq!(cfg(0xffff_f000, 0x2000).ggtt_extent(PAGE), Err(AdoptRefusal::ExtentOverflow), "past 4 GiB");
    assert_eq!(cfg(0, 0x1000).ggtt_extent(3000), Err(AdoptRefusal::ExtentOverflow), "not a power of two");
    assert_eq!(GTT_MIN_ALIGNMENT, 4096);
}
