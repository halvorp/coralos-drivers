// SPDX-License-Identifier: GPL-2.0-only
//! eDP (27), PSR (9), backlight (43) blocks.
mod common;
use common::*;
use i915_vbt_core::backlight::*;
use i915_vbt_core::edp::*;
use i915_vbt_core::psr::*;
use i915_vbt_core::{Vbt, VbtError};

fn edp_block(len: usize, f: &dyn Fn(&mut Vec<u8>)) -> Vec<u8> {
    let mut d = vec![0u8; len];
    f(&mut d);
    d
}
fn put16(d: &mut [u8], o: usize, v: u16) { d[o..o + 2].copy_from_slice(&v.to_le_bytes()); }
fn put32(d: &mut [u8], o: usize, v: u32) { d[o..o + 4].copy_from_slice(&v.to_le_bytes()); }
fn put64(d: &mut [u8], o: usize, v: u64) { d[o..o + 8].copy_from_slice(&v.to_le_bytes()); }

#[test]
fn edp_constants_are_linux_literals() {
    // intel_vbt_defs.h:1044-1060
    assert_eq!([EDP_18BPP, EDP_24BPP, EDP_30BPP], [0, 1, 2]);
    assert_eq!([EDP_RATE_1_62, EDP_RATE_2_7, EDP_RATE_5_4], [0, 1, 2]);
    assert_eq!([EDP_LANE_1, EDP_LANE_2, EDP_LANE_4], [0, 1, 3]);
    assert_eq!([EDP_PREEMPHASIS_NONE, EDP_PREEMPHASIS_3_5DB, EDP_PREEMPHASIS_6DB, EDP_PREEMPHASIS_9_5DB], [0, 1, 2, 3]);
    assert_eq!([EDP_VSWING_0_4V, EDP_VSWING_0_6V, EDP_VSWING_0_8V, EDP_VSWING_1_2V], [0, 1, 2, 3]);
    assert_eq!(EDP_MIN_SIZE, 200);
}

#[test]
fn edp_power_sequence_for_each_panel() {
    // 16 distinct sequences; each panel reads its own 10 bytes (intel_vbt_defs.h:1036, intel_bios.c:1406-1414)
    let blk = edp_block(748, &|d| for p in 0..16usize { for k in 0..5usize { put16(d, p * 10 + k * 2, (p * 100 + k + 1) as u16); } });
    let b = vbt(221, &[(27, blk)]);
    let v = Vbt::parse(&b).unwrap();
    for p in 0..16u8 {
        let s = parse_edp(&v, p).unwrap().unwrap().power_seq;
        let base = p as u16 * 100;
        assert_eq!((s.t1_t3, s.t8, s.t9, s.t10, s.t11_t12), (base + 1, base + 2, base + 3, base + 4, base + 5));
    }
    assert_eq!(parse_edp(&v, 16), Err(VbtError::PanelTypeOutOfRange { panel_type: 16 }));
}

#[test]
fn edp_colour_depth_link_and_swing_families() {
    // every enum member through the decode (intel_bios.c:1428-1520)
    for (code, bpp) in [(0u32, Some(18u8)), (1, Some(24)), (2, Some(30)), (3, None)] {
        let blk = edp_block(748, &|d| put32(d, 160, code << 6)); // panel 3
        let e = parse_edp(&Vbt::parse(&vbt(221, &[(27, blk)])).unwrap(), 3).unwrap().unwrap();
        assert_eq!(e.bpp, bpp, "depth {code}");
    }
    for (r, khz) in [(0u8, Some(162000u32)), (1, Some(270000)), (2, Some(540000)), (3, None), (15, None)] {
        let blk = edp_block(748, &|d| d[164 + 2 * 5] = r); // panel 5 rate = low nibble of first byte
        let e = parse_edp(&Vbt::parse(&vbt(221, &[(27, blk)])).unwrap(), 5).unwrap().unwrap();
        assert_eq!(e.rate, khz, "rate {r}");
    }
    for (l, n) in [(0u8, Some(1u8)), (1, Some(2)), (2, None), (3, Some(4)), (4, None), (15, None)] {
        let blk = edp_block(748, &|d| d[164 + 2 * 5] = l << 4);
        let e = parse_edp(&Vbt::parse(&vbt(221, &[(27, blk)])).unwrap(), 5).unwrap().unwrap();
        assert_eq!(e.lanes, n, "lanes {l}");
    }
    for (pe, want) in [(0u8, Some(0u8)), (1, Some(0x08)), (2, Some(0x10)), (3, Some(0x18)), (4, None), (15, None)] {
        // DP_TRAIN_PRE_EMPH_LEVEL_n = n << 3 (include/drm/display/drm_dp.h:633-636)
        let blk = edp_block(748, &|d| d[164 + 2 * 5 + 1] = pe);
        let e = parse_edp(&Vbt::parse(&vbt(221, &[(27, blk)])).unwrap(), 5).unwrap().unwrap();
        assert_eq!(e.preemphasis, want, "preemph {pe}");
    }
    for (sw, want) in [(0u8, Some(0u8)), (1, Some(1)), (2, Some(2)), (3, Some(3)), (4, None), (15, None)] {
        let blk = edp_block(748, &|d| d[164 + 2 * 5 + 1] = sw << 4);
        let e = parse_edp(&Vbt::parse(&vbt(221, &[(27, blk)])).unwrap(), 5).unwrap().unwrap();
        assert_eq!(e.vswing, want, "vswing {sw}");
    }
    // sdrrs_msa_timing_delay: 2 bits per panel @196
    let b = vbt(221, &[(27, edp_block(748, &|d| put32(d, 196, 0b10 << 14)))]);
    assert_eq!(parse_edp(&Vbt::parse(&b).unwrap(), 7).unwrap().unwrap().drrs_msa_timing_delay, 2);
}

#[test]
fn edp_version_gated_fields() {
    let blk = edp_block(848, &|d| {
        d[164] = 0x01; // rate index 1 (2.7)
        put64(d, 204, 0x0000_0000_0000_0010); // panel 1 nibble = 1; panel 0 nibble = 0
        put16(d, 748, 270); // edp_fast_link_training_rate[0] (224+) -> 5400
        put16(d, 780, 540); // edp_max_port_link_rate[0] (244+) -> 10800
        put16(d, 812, 0b101); // dsc_disable bits (251+)
    });
    let at = |ver: u16, p: u8| parse_edp(&Vbt::parse(&vbt(ver, &[(27, blk.clone())])).unwrap(), p).unwrap().unwrap();
    // < 173: no low_vswing
    assert_eq!(at(172, 0).low_vswing, None);
    assert_eq!(at(173, 0).low_vswing, Some(true)); // nibble 0 == low vswing
    assert_eq!(at(173, 1).low_vswing, Some(false));
    // < 224 uses the 4-bit rate enum, >= 224 uses the *20 table (intel_bios.c:1447-1448)
    assert_eq!(at(223, 0).rate, Some(270000));
    assert_eq!(at(224, 0).rate, Some(5400));
    assert_eq!(at(243, 0).max_link_rate, None);
    assert_eq!(at(244, 0).max_link_rate, Some(10800));
    assert_eq!(at(250, 0).dsc_disable, None);
    assert_eq!(at(251, 0).dsc_disable, Some(true));
    assert_eq!(at(251, 1).dsc_disable, Some(false));
    assert_eq!(at(251, 2).dsc_disable, Some(true));
}

#[test]
fn edp_absent_and_short() {
    let b = vbt(221, &[(1, vec![0; 5])]);
    assert_eq!(parse_edp(&Vbt::parse(&b).unwrap(), 0), Ok(None));
    let b = vbt(221, &[(27, vec![0; 100])]);
    assert_eq!(parse_edp(&Vbt::parse(&b).unwrap(), 0), Err(VbtError::BlockTooSmall { id: 27, size: 100, need: 200 }));
}

#[test]
fn psr_constants_and_per_panel_decode() {
    assert_eq!(PSR_TABLE_SIZE, 6); // intel_vbt_defs.h:711-724
    assert_eq!(PSR2_WAKEUP_OFFSET, 96); // psr_table[16] then u32 (:727-731); the real block is 96 bytes at BDB 221
    let blk = {
        let mut d = vec![0u8; 100];
        for p in 0..16usize { d[p * 6] = (p & 3) as u8; d[p * 6 + 1] = (p as u8) | 0x30; put16(&mut d, p * 6 + 2, (p % 4) as u16); put16(&mut d, p * 6 + 4, ((p + 1) % 4) as u16); }
        d
    };
    let at = |ver: u16, p: u8, plat: PsrPlatform| parse_psr(&Vbt::parse(&vbt(ver, &[(9, blk.clone())])).unwrap(), p, plat).unwrap().unwrap();
    let k = PsrPlatform::KABYLAKE;
    let r = at(221, 7, k); // bits0 = 3 (both flags), idle_frames 7 (low nibble), tp1 = 3 -> 0us, tp2_tp3 = 0 -> 500us
    assert!(r.full_link && r.require_aux_wakeup);
    assert_eq!(r.idle_frames, 7);
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (0, 500));
    let r = at(221, 1, k); // bits0 = 1, tp1 1 -> 100, tp2_tp3 2 -> 2500
    assert!(r.full_link && !r.require_aux_wakeup);
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (100, 2500));
    // PSR2 reuses tp2_tp3 before 226 (intel_bios.c:1641)
    assert_eq!(r.psr2_tp2_tp3_wakeup_time_us, 2500);
}

#[test]
fn psr_wakeup_encodings() {
    // 2-bit table: 0=500, 1=100, 2=2500, 3=0, out of range = 2500 (intel_bios.c:1576-1614)
    for (v, us) in [(0u16, 500u32), (1, 100), (2, 2500), (3, 0), (4, 2500), (255, 2500)] { assert_eq!(wakeup_us_new(v), us, "{v}"); }
    // pre-205 or BXT or display ver < 9: the value IS the time in units of 100 us (intel_bios.c:1615-1618)
    let mut blk = vec![0u8; 100];
    put16(&mut blk, 2, 7); put16(&mut blk, 4, 12);
    let get = |ver: u16, plat: PsrPlatform| parse_psr(&Vbt::parse(&vbt(ver, &[(9, blk.clone())])).unwrap(), 0, plat).unwrap().unwrap();
    let r = get(204, PsrPlatform::KABYLAKE);
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (700, 1200));
    let r = get(205, PsrPlatform::KABYLAKE);
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (2500, 2500));
    let r = get(205, PsrPlatform { display_ver: 9, broxton: true });
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (700, 1200));
    let r = get(205, PsrPlatform { display_ver: 8, broxton: false });
    assert_eq!((r.tp1_wakeup_time_us, r.tp2_tp3_wakeup_time_us), (700, 1200));
    // PSR2 per-panel 2-bit field, BDB 226+ (intel_bios.c:1620-1646); panel 5 uses bits 10-11
    for (bits, us) in [(0u32, 500u32), (1, 100), (2, 2500), (3, 50)] {
        let mut b2 = vec![0u8; 100];
        put32(&mut b2, 96, bits << 10);
        let r = parse_psr(&Vbt::parse(&vbt(226, &[(9, b2)])).unwrap(), 5, PsrPlatform::KABYLAKE).unwrap().unwrap();
        assert_eq!(r.psr2_tp2_tp3_wakeup_time_us, us, "psr2 {bits}");
    }
    assert_eq!(parse_psr(&Vbt::parse(&vbt(221, &[(1, vec![0; 5])])).unwrap(), 0, PsrPlatform::KABYLAKE), Ok(None));
    let b = vbt(221, &[(9, vec![0; 50])]);
    assert_eq!(parse_psr(&Vbt::parse(&b).unwrap(), 0, PsrPlatform::KABYLAKE), Err(VbtError::BlockTooSmall { id: 9, size: 50, need: 96 }));
    assert_eq!(parse_psr(&Vbt::parse(&vbt(221, &[(9, vec![0; 96])])).unwrap(), 16, PsrPlatform::KABYLAKE), Err(VbtError::PanelTypeOutOfRange { panel_type: 16 }));
}

fn bl_block(len: usize, f: &dyn Fn(&mut Vec<u8>)) -> Vec<u8> {
    let mut d = vec![0u8; len];
    d[0] = 6;
    f(&mut d);
    d
}

#[test]
fn backlight_constants() {
    assert_eq!((BDB_BACKLIGHT_TYPE_NONE, BDB_BACKLIGHT_TYPE_PWM), (0, 2)); // intel_vbt_defs.h:1300-1301
    assert_eq!(LFP_BACKLIGHT_ENTRY_SIZE, 6); // :1303-1311
    assert_eq!(BACKLIGHT_MIN_SIZE, 113);
}

#[test]
fn backlight_pwm_entry_decode() {
    // entry: bits {type:2, active_low:1,...}, pwm_freq u16, min_brightness u8, i2c addr/cmd
    let blk = bl_block(129, &|d| {
        let e = 1 + 4 * 6;
        d[e] = 0b0000_0110; // type 2, active_low 1
        put16(d, e + 1, 4000);
        d[e + 3] = 9;
        d[97 + 4] = 77; // level[4]
        d[113 + 4] = 0x31; // control: type 1, controller 3
    });
    let at = |ver: u16| parse_lfp_backlight(&Vbt::parse(&vbt(ver, &[(43, blk.clone())])).unwrap(), 4).unwrap().unwrap();
    let b = at(221);
    assert!(b.present);
    assert_eq!((b.entry_type, b.active_low_pwm, b.pwm_freq_hz, b.min_brightness), (2, true, 4000, 9));
    assert_eq!((b.control_type, b.controller), (Some(1), 3));
    assert_eq!(b.hdr_dpcd_refresh_timeout, 30);
    // before 191 there is no control method
    let b = at(190);
    assert_eq!((b.control_type, b.controller), (None, 0));
    // types other than PWM: present = false
    for ty in [0u8, 1, 3] {
        let blk = bl_block(129, &|d| d[1 + 4 * 6] = ty);
        let b = parse_lfp_backlight(&Vbt::parse(&vbt(221, &[(43, blk)])).unwrap(), 4).unwrap().unwrap();
        assert!(!b.present, "type {ty}");
    }
}

#[test]
fn backlight_234_plus_levels_and_hdr_timeout() {
    let blk = bl_block(305, &|d| {
        let e = 1 + 2 * 6;
        d[e] = 2;
        put16(d, 129 + 2 * 4, 1000); // brightness_level[2].level (> 255)
        put16(d, 193 + 2 * 4, 5100); // brightness_min_level[2].level
        d[257 + 2] = 16; // precision bits
        put16(d, 273 + 2 * 2, 250); // hdr_dpcd_refresh_timeout[2]
    });
    let at = |ver: u16| parse_lfp_backlight(&Vbt::parse(&vbt(ver, &[(43, blk.clone())])).unwrap(), 2).unwrap().unwrap();
    // 236+: scale iff precision_bits == 16 -> 5100 / 255 = 20
    assert_eq!((at(236).min_brightness, at(236).brightness_precision_bits), (20, 16));
    // 234-235: scale iff level > 255 -> also scaled
    assert_eq!(at(234).min_brightness, 20);
    // pre-234: min_brightness comes from the entry byte, which is 0 here
    assert_eq!(at(233).min_brightness, 0);
    // 239+: refresh timeout = DIV_ROUND_UP(250, 100) = 3; earlier: 30
    assert_eq!(at(239).hdr_dpcd_refresh_timeout, 3);
    assert_eq!(at(238).hdr_dpcd_refresh_timeout, 30);
    // DIV_ROUND_UP(x, 100) boundaries
    for (raw, want) in [(0u16, 0u32), (1, 1), (100, 1), (101, 2), (200, 2), (201, 3), (300, 3), (301, 4)] {
        let mut b3 = blk.clone();
        put16(&mut b3, 273 + 2 * 2, raw);
        let r = parse_lfp_backlight(&Vbt::parse(&vbt(239, &[(43, b3)])).unwrap(), 2).unwrap().unwrap();
        assert_eq!(r.hdr_dpcd_refresh_timeout, want, "raw {raw}");
    }
    // 236+ with precision 8 and level > 255: NOT scaled
    let blk2 = bl_block(305, &|d| { d[1 + 2 * 6] = 2; put16(d, 129 + 8, 1000); put16(d, 193 + 8, 5100); d[257 + 2] = 8; });
    let b = parse_lfp_backlight(&Vbt::parse(&vbt(236, &[(43, blk2)])).unwrap(), 2).unwrap().unwrap();
    assert_eq!(b.min_brightness, 5100);
}

#[test]
fn backlight_refusals() {
    let b = vbt(221, &[(43, bl_block(129, &|d| d[0] = 5))]);
    assert_eq!(parse_lfp_backlight(&Vbt::parse(&b).unwrap(), 0), Err(VbtError::UnsupportedBacklightEntrySize { size: 5 }));
    let b = vbt(221, &[(43, vec![6; 50])]);
    assert_eq!(parse_lfp_backlight(&Vbt::parse(&b).unwrap(), 0), Err(VbtError::BlockTooSmall { id: 43, size: 50, need: 113 }));
    let b = vbt(221, &[(1, vec![0; 5])]);
    assert_eq!(parse_lfp_backlight(&Vbt::parse(&b).unwrap(), 0), Ok(None));
    let b = vbt(221, &[(43, bl_block(129, &|_| {}))]);
    assert_eq!(parse_lfp_backlight(&Vbt::parse(&b).unwrap(), 16), Err(VbtError::PanelTypeOutOfRange { panel_type: 16 }));
}
