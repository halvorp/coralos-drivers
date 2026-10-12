// SPDX-License-Identifier: GPL-2.0-only
mod common;
use common::*;
use i915_vbt_core::child::*;
use i915_vbt_core::general::*;
use i915_vbt_core::{Vbt, VbtError};

#[test]
fn constants_are_linux_literals() {
    // intel_vbt_defs.h:312-323
    assert_eq!(DEVICE_TYPE_INTERNAL_CONNECTOR, 0x1000);
    assert_eq!(DEVICE_TYPE_NOT_HDMI_OUTPUT, 0x0800);
    assert_eq!(DEVICE_TYPE_MIPI_OUTPUT, 0x0400);
    assert_eq!(DEVICE_TYPE_TMDS_DVI_SIGNALING, 0x0010);
    assert_eq!(DEVICE_TYPE_DISPLAYPORT_OUTPUT, 0x0004);
    assert_eq!(DEVICE_TYPE_ANALOG_OUTPUT, 0x0001);
    assert_eq!((DEVICE_HANDLE_LFP1, DEVICE_HANDLE_LFP2), (0x0008, 0x0080)); // :270-271
    // :351-363
    assert_eq!([DVO_PORT_HDMIA, DVO_PORT_HDMIB, DVO_PORT_HDMIC, DVO_PORT_HDMID], [0, 1, 2, 3]);
    assert_eq!((DVO_PORT_LVDS, DVO_PORT_TV, DVO_PORT_CRT), (4, 5, 6));
    assert_eq!([DVO_PORT_DPB, DVO_PORT_DPC, DVO_PORT_DPD, DVO_PORT_DPA, DVO_PORT_DPE, DVO_PORT_HDMIE], [7, 8, 9, 10, 11, 12]);
    // :414-418
    assert_eq!([DP_AUX_A, DP_AUX_B, DP_AUX_C, DP_AUX_D, DP_AUX_E], [0x40, 0x10, 0x20, 0x30, 0x50]);
    assert_eq!(LEGACY_CHILD_DEVICE_CONFIG_SIZE, 33); // :384
    assert_eq!(CHILD_DEVICE_CONFIG_SIZE, 44);
}

#[test]
fn dvo_port_to_port_family() {
    // intel_bios.c:2344-2352: each DDI takes its HDMI and DP dvo values; E also takes CRT; everything else none
    let want: [(u8, Option<Port>); 16] = [
        (0, Some(Port::A)), (1, Some(Port::B)), (2, Some(Port::C)), (3, Some(Port::D)), (4, None), (5, None),
        (6, Some(Port::E)), (7, Some(Port::B)), (8, Some(Port::C)), (9, Some(Port::D)), (10, Some(Port::A)),
        (11, Some(Port::E)), (12, Some(Port::E)), (13, None), (14, None), (15, None),
    ];
    for (dvo, p) in want { assert_eq!(dvo_port_to_port(dvo), p, "dvo {dvo}"); }
    assert_eq!(dvo_port_to_port(255), None);
    assert_eq!([Port::A as u8, Port::B as u8, Port::C as u8, Port::D as u8, Port::E as u8], [0, 1, 2, 3, 4]);
}

#[test]
fn aux_channel_family() {
    // direct_aux_ch_map, intel_bios.c:3675-3685: A=0x40 B=0x10 C=0x20 D=0x30 E=0x50
    assert_eq!(map_aux_ch(0x40), Some(AuxCh::A));
    assert_eq!(map_aux_ch(0x10), Some(AuxCh::B));
    assert_eq!(map_aux_ch(0x20), Some(AuxCh::C));
    assert_eq!(map_aux_ch(0x30), Some(AuxCh::D));
    assert_eq!(map_aux_ch(0x50), Some(AuxCh::E));
    for bad in [0x00u8, 0x01, 0x60, 0x70, 0x80, 0x90, 0xff] { assert_eq!(map_aux_ch(bad), None, "{bad:#x}"); }
}

/// Pin every offset of the packed layout by setting one byte and reading the field back.
#[test]
fn field_offsets() {
    let c = |o: usize, v: u8| ChildDevice::from_bytes(221, &child(44, &[(o, v)]));
    assert_eq!(c(0, 0x34).handle(), 0x34); assert_eq!(c(1, 0x12).handle(), 0x1200);
    assert_eq!(c(2, 0x06).device_type(), 0x06); assert_eq!(c(3, 0x18).device_type(), 0x1800);
    assert_eq!(c(16, 7).dvo_port(), 7);
    assert_eq!(c(17, 8).i2c_pin(), 8);
    assert_eq!(c(18, 0x70).target_addr(), 0x70);
    assert_eq!(c(19, 3).ddc_pin(), 3);
    assert_eq!(c(22, 0x0a).dvo_cfg(), 0x0a);
    assert_eq!(c(25, 0x40).aux_channel(), 0x40);
    assert_eq!(c(28, 5).dvo_wiring(), 5);
    // byte 23 bits
    assert!(c(23, 1).efp_routed());
    assert!(c(23, 1 << 1).lane_reversal());
    assert!(c(23, 1 << 2).lspcon());
    assert!(c(23, 1 << 3).iboost());
    assert!(c(23, 1 << 4).hpd_invert());
    assert!(c(23, 1 << 5).use_vbt_vswing());
    assert_eq!(c(23, 0xc0).dp_max_lane_count_field(), 3);
    // each bit drives only its own flag
    for bit in 0..6 {
        let d = c(23, 1 << bit);
        let set = [d.efp_routed(), d.lane_reversal(), d.lspcon(), d.iboost(), d.hpd_invert(), d.use_vbt_vswing()];
        assert_eq!(set.iter().filter(|x| **x).count(), 1);
        assert!(set[bit]);
    }
    // byte 24 bits
    assert!(c(24, 1).hdmi_support() && !c(24, 1).dp_support() && !c(24, 1).tmds_support());
    assert!(c(24, 2).dp_support() && !c(24, 2).hdmi_support() && !c(24, 2).tmds_support());
    assert!(c(24, 4).tmds_support() && !c(24, 4).hdmi_support() && !c(24, 4).dp_support());
    // byte 33 bits
    assert!(c(33, 1).dp_usb_type_c() && !c(33, 1).tbt());
    assert!(c(33, 2).tbt() && !c(33, 2).dp_usb_type_c());
    assert!(c(33, 4).dedicated_external() && !c(33, 4).dyn_port_over_tc());
    assert!(c(33, 8).dyn_port_over_tc() && !c(33, 8).dedicated_external());
    // byte 37 nibbles, 38, 40-41, byte 7
    assert_eq!((c(37, 0x21).dp_iboost_level(), c(37, 0x21).hdmi_iboost_level()), (1, 2));
    assert_eq!(c(38, 0x0b).dp_max_link_rate_field(), 3);
    let o = ChildDevice::from_bytes(263, &child(44, &[(40, 0xff), (41, 0xff)]));
    assert_eq!(o.edp_data_rate_override(), 0x0fff);
    assert_eq!(c(7, 0xff).hdmi_level_shifter_value_raw(), 0x1f);
    assert_eq!(c(7, 0xe0).hdmi_max_data_rate_field(), 7);
}

#[test]
fn short_child_is_zero_padded_and_oversize_is_truncated() {
    // intel_bios.c:2942: memcpy min(child_dev_size, sizeof)
    let short = ChildDevice::from_bytes(221, &child(33, &[(16, 10), (23, 0xff)]));
    assert_eq!(short.dp_iboost_level(), 0); // byte 37 absent -> 0
    assert_eq!(short.dvo_port(), 10);
    let long = ChildDevice::from_bytes(263, &child(60, &[(44, 0xaa), (16, 3)]));
    assert_eq!(long.dvo_port(), 3);
    assert_eq!(long.raw().len(), 44);
}

#[test]
fn device_type_predicates() {
    let t = |ty: u16| ChildDevice::from_bytes(221, &child(39, &[(2, ty as u8), (3, (ty >> 8) as u8)]));
    // atlas eDP 0x1806: DP | INTERNAL | (1<<1 digital) | (1<<11 .. wait 0x0800 = NOT_HDMI)
    let edp = t(0x1806);
    assert!(edp.supports_dp() && edp.supports_edp() && !edp.supports_dvi() && !edp.supports_hdmi() && !edp.supports_dsi() && !edp.supports_crt());
    // DEVICE_TYPE_DP 0x68c6 (intel_vbt_defs.h:299) has no INTERNAL_CONNECTOR bit and no TMDS bit: dp, not edp, not dvi
    let dp = t(0x68c6);
    assert!(dp.supports_dp() && !dp.supports_edp());
    assert!(!dp.supports_dvi() && !dp.supports_hdmi());
    // DEVICE_TYPE_DP_DUAL_MODE 0x60d6: DP + TMDS, NOT_HDMI clear -> hdmi capable (:300)
    let dual = t(0x60d6);
    assert!(dual.supports_dp() && dual.supports_dvi() && dual.supports_hdmi());
    // 0x60d2 (DEVICE_TYPE_HDMI): TMDS set, NOT_HDMI clear
    let hdmi = t(0x60d2);
    assert!(hdmi.supports_dvi() && hdmi.supports_hdmi() && !hdmi.supports_dp());
    assert!(t(0x0401).supports_dsi() && t(0x0401).supports_crt());
    assert!(!t(0).supports_dp() && !t(0).supports_dvi() && !t(0).supports_crt() && !t(0).supports_dsi());
}

#[test]
fn sanitize_device_type_strips_dvi_from_port_a_only() {
    // intel_bios.c:2578-2597 (display ver < 12): port A cannot do DVI/HDMI
    let mk = |dvo: u8, ty: u16| ChildDevice::from_bytes(221, &child(39, &[(2, ty as u8), (3, (ty >> 8) as u8), (16, dvo)]));
    let mut a = mk(DVO_PORT_DPA, 0x6012 | 0x0010);
    assert!(a.supports_hdmi());
    a.sanitize_device_type();
    assert!(!a.supports_dvi());
    assert_eq!(a.device_type() & DEVICE_TYPE_NOT_HDMI_OUTPUT, DEVICE_TYPE_NOT_HDMI_OUTPUT);
    let mut b = mk(DVO_PORT_DPB, 0x6012 | 0x0010);
    let before = b.device_type();
    b.sanitize_device_type();
    assert_eq!(b.device_type(), before);
    let mut a2 = mk(DVO_PORT_DPA, 0x0004); // port A without DVI: untouched
    a2.sanitize_device_type();
    assert_eq!(a2.device_type(), 0x0004);
}

#[test]
fn version_gated_accessors() {
    let mk = |ver: u16, set: &[(usize, u8)]| ChildDevice::from_bytes(ver, &child(44, set));
    // hdmi_level_shift: -1 below 158 (intel_bios.c:2666)
    assert_eq!(mk(157, &[(7, 9)]).hdmi_level_shift(), -1);
    assert_eq!(mk(158, &[(7, 9)]).hdmi_level_shift(), 9);
    // hdmi_max_tmds_clock: 0 below 204, then the table (intel_bios.c:2675-2697) — every member
    assert_eq!(mk(203, &[(7, 3 << 5)]).hdmi_max_tmds_clock(), 0);
    for (f, khz) in [(0u8, 0u32), (1, 297000), (2, 165000), (3, 594000), (4, 340000), (5, 300000), (6, 0), (7, 0)] {
        assert_eq!(mk(204, &[(7, f << 5)]).hdmi_max_tmds_clock(), khz, "field {f}");
    }
    // dp_max_link_rate: 0 below 216; 216 table; 230 table (intel_bios.c:2441-2507)
    assert_eq!(mk(215, &[(38, 1)]).dp_max_link_rate(), 0);
    for (f, khz) in [(0u8, 810000u32), (1, 540000), (2, 270000), (3, 162000), (4, 810000), (7, 810000)] {
        assert_eq!(mk(216, &[(38, f)]).dp_max_link_rate(), khz, "216 field {f}");
        assert_eq!(dp_max_link_rate_216(f), khz);
    }
    for (f, khz) in [(0u8, 0u32), (1, 162000), (2, 270000), (3, 540000), (4, 810000), (5, 1000000), (6, 1350000), (7, 2000000)] {
        assert_eq!(mk(230, &[(38, f)]).dp_max_link_rate(), khz, "230 field {f}");
        assert_eq!(dp_max_link_rate_230(f), khz);
    }
    // dp_max_lane_count: 0 below 244, else field + 1 (intel_bios.c:2509)
    assert_eq!(mk(243, &[(23, 0xc0)]).dp_max_lane_count(), 0);
    for f in 0..4u8 { assert_eq!(mk(244, &[(23, f << 6)]).dp_max_lane_count(), f + 1); }
    // iboost: needs BDB 196+ and the iboost flag; translate 0,1,2 -> 1,3,7, else 0 (intel_bios.c:2181, 3747-3761)
    assert_eq!([0u8, 1, 2, 3, 15].map(ChildDevice::translate_iboost), [1, 3, 7, 0, 0]);
    let b = |ver: u16, flag: u8| mk(ver, &[(23, flag), (37, 0x21)]);
    assert_eq!((b(196, 8).dp_boost_level(), b(196, 8).hdmi_boost_level()), (3, 7));
    assert_eq!((b(195, 8).dp_boost_level(), b(195, 8).hdmi_boost_level()), (0, 0));
    assert_eq!((b(196, 0).dp_boost_level(), b(196, 0).hdmi_boost_level()), (0, 0));
}

#[test]
fn expected_child_size_table() {
    // intel_bios.c:2833-2855, every band edge
    let t: [(u16, Option<usize>); 18] = [
        (105, Some(22)), (106, Some(27)), (110, Some(27)), (111, Some(33)), (194, Some(33)), (195, Some(37)),
        (196, Some(38)), (215, Some(38)), (216, Some(39)), (255, Some(39)), (256, Some(40)), (262, Some(40)),
        (263, Some(44)), (264, Some(44)), (265, None), (0, Some(22)), (221, Some(39)), (65535, None),
    ];
    for (v, s) in t { assert_eq!(child_device_expected_size(v), s, "version {v}"); }
}

#[test]
fn general_definitions_parse_and_refuse() {
    let edp = child(39, &[(2, 0x06), (3, 0x18), (0, 8), (16, 10), (25, 0x40)]);
    let empty = child(39, &[(0, 1)]); // device_type 0 -> skipped even with a handle
    let dp = child(39, &[(2, 0xc6), (3, 0x68), (0, 4), (16, 7)]);
    let b = vbt(221, &[(2, general_defs(2, 39, &[edp, empty, dp]))]);
    let v = Vbt::parse(&b).unwrap();
    let g = parse_general_definitions(&v).unwrap();
    assert_eq!((g.crt_ddc_pin, g.child_dev_size, g.size_mismatch), (2, 39, None));
    assert_eq!(g.children.len(), 2);
    assert_eq!(g.child_by_handle(8).unwrap().aux_ch(), Some(AuxCh::A));
    assert_eq!(g.child_by_handle(4).unwrap().port(), Some(Port::B));
    assert!(g.child_by_handle(99).is_none());

    // unexpected size: continue, but report (actual, expected) (intel_bios.c:2870-2882)
    let b = vbt(221, &[(2, general_defs(2, 40, &[child(40, &[(2, 6)])]))]);
    let g = parse_general_definitions(&Vbt::parse(&b).unwrap()).unwrap();
    assert_eq!(g.size_mismatch, Some((40, 39)));
    assert_eq!(g.children.len(), 1);
    // version > 264: expected falls back to sizeof(child) = 44
    let b = vbt(300, &[(2, general_defs(2, 44, &[child(44, &[(2, 6)])]))]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()).unwrap().size_mismatch, None);

    // size < 33 refuses; 33 is the minimum
    let b = vbt(221, &[(2, general_defs(2, 32, &[]))]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()), Err(VbtError::ChildDeviceSizeTooSmall { size: 32 }));
    let b = vbt(221, &[(2, general_defs(2, 33, &[child(33, &[(2, 6)])]))]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()).unwrap().children.len(), 1);
    let b = vbt(221, &[(2, general_defs(2, 0, &[]))]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()), Err(VbtError::ChildDeviceSizeTooSmall { size: 0 }));
    // block shorter than sizeof(bdb_general_definitions) = 5 (intel_bios.c:2904)
    let b = vbt(221, &[(2, vec![1, 2, 3])]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()), Err(VbtError::GeneralDefinitionsTooSmall { size: 3 }));
    // missing block
    let b = vbt(221, &[(1, vec![0; 5])]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()), Err(VbtError::BlockMissing { id: 2 }));
    // trailing bytes that do not make a whole child are ignored (integer division)
    let mut d = general_defs(2, 39, &[child(39, &[(2, 6)])]);
    d.extend_from_slice(&[9; 20]);
    let b = vbt(221, &[(2, d)]);
    assert_eq!(parse_general_definitions(&Vbt::parse(&b).unwrap()).unwrap().children.len(), 1);
}

#[test]
fn general_features_decode() {
    // bytes: [b1, b2, b3, legacy, b5]: enable_ssc b1.bit1, ssc_freq b1.bit2, display_clock_mode b1.bit6,
    // rotate_180 b2.bit2, fdi_rx_polarity_inverted b2.bit3, int_crt b5.bit0, int_tv b5.bit1 (intel_vbt_defs.h:200-240)
    let mk = |ver: u16, d: Vec<u8>| { let b = vbt(ver, &[(1, d)]); parse_general_features(&Vbt::parse(&b).unwrap()).unwrap().unwrap() };
    let f = mk(221, vec![0, 0b0100_0110, 0b0000_1100, 0, 0b0000_0011]);
    assert!(f.lvds_use_ssc && f.display_clock_mode && f.fdi_rx_polarity_inverted && f.int_tv_support);
    assert_eq!(f.lvds_ssc_freq, 100000);
    assert_eq!((f.int_crt_support, f.rotate_180), (Some(true), Some(true)));
    let f = mk(154, vec![0, 0, 0, 0, 0]);
    assert!(!f.lvds_use_ssc && !f.display_clock_mode && !f.int_tv_support);
    assert_eq!(f.lvds_ssc_freq, 120000);
    assert_eq!((f.int_crt_support, f.rotate_180), (None, None));
    let f = mk(180, vec![0, 0, 0b100, 0, 1]);
    assert_eq!((f.int_crt_support, f.rotate_180), (Some(true), None));
    let b = vbt(221, &[(1, vec![0; 3])]);
    assert!(matches!(parse_general_features(&Vbt::parse(&b).unwrap()), Err(VbtError::BlockTooSmall { id: 1, .. })));
    let b = vbt(221, &[(2, vec![0; 5])]);
    assert_eq!(parse_general_features(&Vbt::parse(&b).unwrap()), Ok(None));
}
