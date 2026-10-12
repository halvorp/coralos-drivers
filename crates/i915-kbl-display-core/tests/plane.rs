// SPDX-License-Identifier: GPL-2.0-only
//! Universal planes and scalers: every format / tiling / rotation member through the decode, strides in bytes.
use i915_kbl_display_core::error::ReadoutError;
use i915_kbl_display_core::plane::*;

const EN: u32 = 0x8000_0000;

fn plane(ctl: u32) -> Result<PlaneState, ReadoutError> {
    decode_plane(0, ctl, 0, 0, 0, 0, 0)
}

#[test]
fn format_family_all_members() {
    // PLANE_CTL bits 27:24, skl_universal_plane_regs.h:50-60; 9, 10, 11, 13, 15 are not SKL formats
    let t: [(u32, Option<PlaneFormat>); 16] = [
        (0, Some(PlaneFormat::Yuv422)),
        (1, Some(PlaneFormat::Nv12)),
        (2, Some(PlaneFormat::Xrgb2101010)),
        (3, Some(PlaneFormat::P010)),
        (4, Some(PlaneFormat::Xrgb8888)),
        (5, Some(PlaneFormat::P012)),
        (6, Some(PlaneFormat::Xrgb16161616f)),
        (7, Some(PlaneFormat::P016)),
        (8, Some(PlaneFormat::Xyuv)),
        (9, None),
        (10, None),
        (11, None),
        (12, Some(PlaneFormat::Indexed)),
        (13, None),
        (14, Some(PlaneFormat::Rgb565)),
        (15, None),
    ];
    for (field, want) in t {
        let ctl = EN | (field << 24);
        assert_eq!(PlaneFormat::decode(ctl), want, "format {field}");
        match want {
            Some(f) => assert_eq!(plane(ctl).unwrap().format, Some(f)),
            None => assert_eq!(plane(ctl), Err(ReadoutError::FormatReserved { plane: 1, field })),
        }
        // a DISABLED plane with an undefined format is not an error
        assert!(plane(field << 24).is_ok());
    }
}

#[test]
fn cpp_of_each_rgb_format() {
    assert_eq!(PlaneFormat::Xrgb8888.cpp(), Some(4));
    assert_eq!(PlaneFormat::Xrgb2101010.cpp(), Some(4));
    assert_eq!(PlaneFormat::Rgb565.cpp(), Some(2));
    assert_eq!(PlaneFormat::Xrgb16161616f.cpp(), Some(8));
    assert_eq!(PlaneFormat::Indexed.cpp(), Some(1));
    for f in [PlaneFormat::Yuv422, PlaneFormat::Nv12, PlaneFormat::P010, PlaneFormat::P012, PlaneFormat::P016, PlaneFormat::Xyuv] {
        assert_eq!(f.cpp(), None);
    }
}

#[test]
fn fourcc_matrix() {
    // skl_format_to_fourcc (skl_universal_plane.c:173-231): XRGB/XBGR x with/without alpha for 8888, 2101010, 16161616F
    let m = [
        (PlaneFormat::Xrgb8888, [("XR24", false, false), ("AR24", false, true), ("XB24", true, false), ("AB24", true, true)]),
        (PlaneFormat::Xrgb2101010, [("XR30", false, false), ("AR30", false, true), ("XB30", true, false), ("AB30", true, true)]),
        (PlaneFormat::Xrgb16161616f, [("XR4H", false, false), ("AR4H", false, true), ("XB4H", true, false), ("AB4H", true, true)]),
    ];
    for (f, cells) in m {
        for (name, rgb, alpha) in cells {
            assert_eq!(fourcc_name(f, rgb, alpha), name, "{f:?} rgb={rgb} alpha={alpha}");
        }
    }
    assert_eq!(fourcc_name(PlaneFormat::Rgb565, true, true), "RG16");
    assert_eq!(fourcc_name(PlaneFormat::Nv12, false, false), "NV12");
    assert_eq!(fourcc_name(PlaneFormat::P010, false, false), "P010");
    assert_eq!(fourcc_name(PlaneFormat::P012, false, false), "P012");
    assert_eq!(fourcc_name(PlaneFormat::P016, false, false), "P016");
    assert_eq!(fourcc_name(PlaneFormat::Xyuv, false, false), "XYUV8888");
}

#[test]
fn tiling_family_all_members() {
    // bits 12:10: 0 linear, 1 X, 4 Y, 5 Yf; 2, 3, 6, 7 undefined (skl_universal_plane.c:3102-3146)
    let t: [(u32, Option<Tiling>); 8] = [
        (0, Some(Tiling::Linear)),
        (1, Some(Tiling::X)),
        (2, None),
        (3, None),
        (4, Some(Tiling::Y)),
        (5, Some(Tiling::Yf)),
        (6, None),
        (7, None),
    ];
    for (field, want) in t {
        let ctl = EN | 0x0400_0000 | (field << 10);
        assert_eq!(Tiling::decode(ctl), want, "tiling {field}");
        match want {
            Some(w) => assert_eq!(plane(ctl).unwrap().tiling, Some(w)),
            None => assert_eq!(plane(ctl), Err(ReadoutError::TilingReserved { plane: 1, field })),
        }
    }
    assert_eq!([Tiling::Linear.name(), Tiling::X.name(), Tiling::Y.name(), Tiling::Yf.name()], ["linear", "X", "Y", "Yf"]);
}

#[test]
fn rotation_family_all_members() {
    for (field, want) in [(0u32, Rotation::R0), (1, Rotation::R90), (2, Rotation::R180), (3, Rotation::R270)] {
        assert_eq!(plane(EN | 0x0400_0000 | field).unwrap().rotation, want, "rotation {field}");
    }
}

#[test]
fn flag_bits_are_their_own() {
    let base = EN | 0x0400_0000;
    let p = plane(base).unwrap();
    assert!(p.enabled && !p.rgb_order && !p.alpha && !p.render_decompression);
    assert!(plane(base | 0x10_0000).unwrap().rgb_order);
    assert!(plane(base | 0x20).unwrap().alpha); // PLANE_CTL_ALPHA_MASK 5:4, SW premultiply = 2
    assert!(plane(base | 0x30).unwrap().alpha); // HW premultiply = 3
    assert!(plane(base | 0x10).unwrap().alpha);
    assert!(plane(base | 0x8000).unwrap().render_decompression);
    assert!(!plane(0x0400_0000).unwrap().enabled);
}

#[test]
fn stride_unit_per_tiling() {
    // skl_plane_stride_mult (skl_universal_plane.c:751-765): linear 64; X 512; Y 128; Yf by cpp 1 -> 64, 2/4 -> 128, 8/16 -> 256
    assert_eq!(stride_unit_bytes(Tiling::Linear, None), Some(64));
    assert_eq!(stride_unit_bytes(Tiling::Linear, Some(4)), Some(64));
    assert_eq!(stride_unit_bytes(Tiling::X, Some(4)), Some(512));
    assert_eq!(stride_unit_bytes(Tiling::Y, Some(4)), Some(128));
    for (cpp, want) in [(1, 64), (2, 128), (4, 128), (8, 256), (16, 256)] {
        assert_eq!(stride_unit_bytes(Tiling::Yf, Some(cpp)), Some(want), "Yf cpp {cpp}");
    }
    assert_eq!(stride_unit_bytes(Tiling::Yf, Some(3)), None);
    assert_eq!(stride_unit_bytes(Tiling::Yf, None), None);
}

#[test]
fn stride_in_bytes_for_the_1080p_and_4k_framebuffers() {
    // X-tiled XRGB8888 1920x1080: 7680 bytes / 512 = 15 tiles; 3840x2160: 15360 / 512 = 30
    let x = EN | 0x0400_0000 | 0x400;
    assert_eq!(decode_plane(0, x, 15, 0x0437_077f, 0, 0, 0).unwrap().stride_bytes, Some(7680));
    assert_eq!(decode_plane(0, x, 30, 0x086f_0eff, 0, 0, 0).unwrap().stride_bytes, Some(15360));
    // linear: 64-byte units, so 1920x4 = 7680 is field 120
    let l = EN | 0x0400_0000;
    assert_eq!(decode_plane(0, l, 120, 0, 0, 0, 0).unwrap().stride_bytes, Some(7680));
    // Y-tiled: 128-byte units
    let y = EN | 0x0400_0000 | 0x1000;
    assert_eq!(decode_plane(0, y, 60, 0, 0, 0, 0).unwrap().stride_bytes, Some(7680));
    // the field is 12 bits: junk above is masked
    assert_eq!(decode_plane(0, l, 0xffff_f000 | 3, 0, 0, 0, 0).unwrap().stride_field, 3);
    assert_eq!(decode_plane(0, l, 0xfff, 0, 0, 0, 0).unwrap().stride_field, 0xfff);
    // a planar format has no cpp, so a Yf stride cannot be sized; a linear one still can
    let nv12_yf = EN | 0x0100_0000 | 0x1400;
    assert_eq!(decode_plane(0, nv12_yf, 4, 0, 0, 0, 0).unwrap().stride_bytes, None);
    let nv12_lin = EN | 0x0100_0000;
    assert_eq!(decode_plane(0, nv12_lin, 4, 0, 0, 0, 0).unwrap().stride_bytes, Some(256));
}

#[test]
fn size_pos_offset_surf_fields() {
    let p = decode_plane(1, EN | 0x0400_0000, 0, 0x0437_077f, 0x0010_0020, 0x0030_0040, 0x0010_0fe5).unwrap();
    assert_eq!(p.plane_id, 1);
    assert_eq!((p.width, p.height), (1920, 1080));
    assert_eq!((p.pos_x, p.pos_y), (0x20, 0x10));
    assert_eq!((p.offset_x, p.offset_y), (0x40, 0x30));
    assert_eq!(p.surf, 0x0010_0000); // low 12 bits (ASYNC_UPDATE, DECRYPT, ...) are not address
    assert_eq!(decode_plane(0, 0, 0, 0, 0, 0, 0xffff_ffff).unwrap().surf, 0xffff_f000);
}

#[test]
fn scaler_decode_and_pipe_fitter_rule() {
    // PS_CTRL: bit 31 enable, 27:25 binding (0 = pipe, n+1 = plane n)
    let s = decode_scaler(0, 0x8000_0000, 0x0000_0000, 0x0780_0438);
    assert!(s.enabled && s.is_pipe_fitter());
    assert_eq!(s.binding, ScalerBinding::Pipe);
    assert_eq!((s.win_x, s.win_y, s.win_w, s.win_h), (0, 0, 0x780, 0x438));
    for n in 0..7u8 {
        let s = decode_scaler(1, 0x8000_0000 | ((n as u32 + 1) << 25), 0, 0);
        assert_eq!(s.binding, ScalerBinding::Plane(n), "binding {n}");
        assert!(s.enabled && !s.is_pipe_fitter());
    }
    let off = decode_scaler(0, 0x0000_0000, 0, 0);
    assert!(!off.enabled && !off.is_pipe_fitter());
    let p = decode_scaler(1, 0, 0x0010_0020, 0x0030_0040);
    assert_eq!((p.win_x, p.win_y, p.win_w, p.win_h), (0x10, 0x20, 0x30, 0x40));
    assert_eq!(p.id, 1);
}

#[test]
fn error_plane_numbers_are_one_based() {
    // PLANE_2 (plane_id 1) is "plane 2" in the error, as in Linux's logs
    assert_eq!(decode_plane(1, 0x8000_0000 | (9 << 24), 0, 0, 0, 0, 0), Err(ReadoutError::FormatReserved { plane: 2, field: 9 }));
    assert_eq!(decode_plane(2, 0x8400_0000 | (2 << 10), 0, 0, 0, 0, 0), Err(ReadoutError::TilingReserved { plane: 3, field: 2 }));
}
