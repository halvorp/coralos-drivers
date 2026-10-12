// SPDX-License-Identifier: GPL-2.0-only
//! DPLL routing / link rate / WRPLL frequency and CDCLK, each family driven member by member with literal words.
use i915_kbl_display_core::cdclk::*;
use i915_kbl_display_core::dpll::*;
use i915_kbl_display_core::error::ReadoutError;
use i915_kbl_display_core::types::Ddi;

#[test]
fn ctrl2_routing_per_port() {
    // DPLL_CTRL2 (intel_display_regs.h:2571-2576): per port p: OVERRIDE = bit 3p, SEL = 2 bits at 3p+1, CLK_OFF = bit p+15
    let ports = [
        (Ddi::A, 0x0000_0001u32, 0x0000_0006u32, 0x0000_8000u32),
        (Ddi::B, 0x0000_0008, 0x0000_0030, 0x0001_0000),
        (Ddi::C, 0x0000_0040, 0x0000_0180, 0x0002_0000),
        (Ddi::D, 0x0000_0200, 0x0000_0c00, 0x0004_0000),
        (Ddi::E, 0x0000_1000, 0x0000_6000, 0x0008_0000),
    ];
    for (d, ovr, sel_mask, off) in ports {
        assert_eq!(ctrl2_sel_override(d), ovr, "{d:?} override");
        assert_eq!(ctrl2_clk_off(d), off, "{d:?} clk off");
        // select every DPLL id 0..=3 through the decode
        for id in 0..4u32 {
            let w = ovr | (id << (sel_mask.trailing_zeros()));
            let c = decode_ctrl2(d, w);
            assert_eq!(c, DdiClock { select_override: true, clk_off: false, pll: id as u8 }, "{d:?} -> dpll {id}");
        }
        // override clear: the select field is not honoured, and clk_off reads independently
        let c = decode_ctrl2(d, sel_mask | off);
        assert_eq!(c, DdiClock { select_override: false, clk_off: true, pll: 3 }, "{d:?} no override");
        assert_eq!(ctrl2_clk_sel_shift(d), sel_mask.trailing_zeros());
    }
    // the ports do not read each other's bits
    let word = 0x0000_0001 | 0x0000_0030; // A override + B select 3, no B override
    assert!(decode_ctrl2(Ddi::A, word).select_override);
    assert!(!decode_ctrl2(Ddi::B, word).select_override);
    assert_eq!(decode_ctrl2(Ddi::B, word).pll, 3);
    assert_eq!(decode_ctrl2(Ddi::A, word).pll, 0);
}

#[test]
fn pll_register_selection() {
    // skl_dpll_regs (intel_dpll_mgr.c:1356-1379): ctl 0x46010 0x46014 0x46040 0x46060; cfgcr 1..=3 at 0x6c040/48/50, 0x6c044/4c/54
    assert_eq!([dpll_ctl(0), dpll_ctl(1), dpll_ctl(2), dpll_ctl(3)], [0x46010, 0x46014, 0x46040, 0x46060]);
    assert_eq!([dpll_cfgcr1(1), dpll_cfgcr1(2), dpll_cfgcr1(3)], [0x6c040, 0x6c048, 0x6c050]);
    assert_eq!([dpll_cfgcr2(1), dpll_cfgcr2(2), dpll_cfgcr2(3)], [0x6c044, 0x6c04c, 0x6c054]);
}

#[test]
fn ctrl1_per_pll_bit_positions() {
    // DPLL_CTRL1 (intel_display_regs.h:2557-2562): per PLL id, 6 bits at id*6: OVERRIDE +0, rate +1..3, SSC +4, HDMI +5
    for id in 0..4u8 {
        let b = id as u32 * 6;
        assert_eq!(ctrl1_override(id), 1 << b);
        assert_eq!(ctrl1_link_rate_mask(id), 7 << (b + 1));
        assert_eq!(ctrl1_link_rate_shift(id), b + 1);
        assert_eq!(ctrl1_ssc(id), 1 << (b + 4));
        assert_eq!(ctrl1_hdmi_mode(id), 1 << (b + 5));
        assert_eq!(dpll_lock(id), 1 << (id as u32 * 8)); // DPLL_STATUS DPLL_LOCK(id), intel_display_regs.h:2580
    }
    assert_eq!(dpll_lock(0), 1);
    assert_eq!(dpll_lock(1), 0x100);
    assert_eq!(dpll_lock(2), 0x1_0000);
    assert_eq!(dpll_lock(3), 0x100_0000);
}

#[test]
fn lcpll_link_rate_family_all_members() {
    // intel_dpll_mgr.c:1909-1934: field -> link clock; port clock = 2x. 6 and 7 are "Unsupported link rate".
    let t: [(u32, Option<u32>); 8] = [
        (0, Some(270_000)), // 2700
        (1, Some(135_000)), // 1350
        (2, Some(81_000)),  // 810
        (3, Some(162_000)), // 1620
        (4, Some(108_000)), // 1080
        (5, Some(216_000)), // 2160
        (6, None),
        (7, None),
    ];
    for (field, want) in t {
        assert_eq!(lcpll_link_clock_khz(field), want, "rate field {field}");
    }
    // through the whole PLL decode, for each PLL id: enabled LCPLL, DP mode, rate field in that PLL's slice
    for id in 0..4u8 {
        for (field, want) in t {
            let ctrl1 = (field << ctrl1_link_rate_shift(id)) | ctrl1_override(id);
            let r = decode_dpll(id, 0x8000_0000, 0, ctrl1, 0, 0);
            match want {
                Some(k) => {
                    let s = r.unwrap();
                    assert_eq!(s.port_clock_khz, k * 2, "id {id} field {field}");
                    assert_eq!(s.link_rate_field, field);
                    assert!(!s.hdmi_mode && s.override_set && s.enabled);
                }
                None => assert_eq!(r, Err(ReadoutError::LinkRateReserved { pll: id, field })),
            }
        }
    }
}

#[test]
fn dp_port_clocks_are_the_dp_link_rates() {
    // The DisplayPort rates (RBR 1.62, HBR 2.70, HBR2 5.40 Gbit/s) reach the PLL as DPLL_CTRL1 fields 810, 1350 and
    // 2700, whose link clocks 81000 / 135000 / 270000 double into the port clocks 162000 / 270000 / 540000
    // (`return link_clock * 2`, intel_dpll_mgr.c:1934).
    let rbr = decode_dpll(0, 0x8000_0000, 0, 2 << 1, 0, 0).unwrap();
    let hbr = decode_dpll(0, 0x8000_0000, 0, 1 << 1, 0, 0).unwrap();
    let hbr2 = decode_dpll(0, 0x8000_0000, 0, 0, 0, 0).unwrap();
    assert_eq!((rbr.port_clock_khz, hbr.port_clock_khz, hbr2.port_clock_khz), (162_000, 270_000, 540_000));
    assert_eq!((dp_rate_cgbps(162_000), dp_rate_cgbps(270_000), dp_rate_cgbps(540_000)), (162, 270, 540));
}

#[test]
fn dpll_enabled_locked_and_slices() {
    // LCPLL1 enabled + locked, DPLL0 override + 1350
    let s = decode_dpll(0, 0xc000_0000, 0x1, 0x3, 0, 0).unwrap();
    assert!(s.enabled && s.locked && s.override_set && !s.ssc && !s.hdmi_mode);
    assert_eq!(s.ctrl1, 0x3);
    // enabled but DPLL_STATUS lock bit of ANOTHER PLL is not this PLL's lock
    let s = decode_dpll(1, 0x8000_0000, 0x1, 0x40 | (1 << 7), 0, 0).unwrap();
    assert!(!s.locked);
    let s = decode_dpll(1, 0x8000_0000, 0x100, 0x40 | (1 << 7), 0, 0).unwrap();
    assert!(s.locked);
    // disabled PLL: nothing decoded beyond the flags, no error even with a reserved rate field
    let s = decode_dpll(2, 0, 0, 7 << 13, 0xffff_ffff, 0xffff_ffff).unwrap();
    assert!(!s.enabled);
    assert_eq!(s.port_clock_khz, 0);
    assert_eq!((s.cfgcr1, s.cfgcr2), (0, 0));
    // SSC bit
    assert!(decode_dpll(3, 0, 0, 1 << 22, 0, 0).unwrap().ssc);
    // slice isolation: PLL 0's whole slice (bits 0..=5, HDMI bit included) is invisible to PLL 1
    let s = decode_dpll(1, 0x8000_0000, 0, 0x3f, 0, 0).unwrap();
    assert_eq!((s.ctrl1, s.hdmi_mode, s.override_set, s.ssc, s.link_rate_field), (0, false, false, false, 0));
    assert_eq!(s.port_clock_khz, 540_000);
    // and PLL 1's slice (bits 6..=11) is PLL 1's: its own rate field 3 (1620) -> 324000
    let s = decode_dpll(1, 0x8000_0000, 0, 3 << 7, 0, 0).unwrap();
    assert_eq!(s.link_rate_field, 3);
    assert_eq!(s.port_clock_khz, 324_000);
    assert_eq!(s.ctrl1, 3 << 1);
}

#[test]
fn wrpll_frequency_vectors() {
    // 148.5 MHz HDMI: DCO 371.25 x 24 MHz = 8910 MHz, P0 = 2 (PDIV_2), P1 = 2 (QDIV on, ratio 2), P2 = 3 (KDIV_3)
    //   p = 12; freq = (371 * 24000 + 0x2000 * 24000 / 0x8000) / (12 * 5) = 8910000 / 60 = 148500
    let cfgcr1 = 0x8040_0173; // FREQ_ENABLE | fraction 0x2000 << 9 | integer 0x173
    let cfgcr2 = 0x0000_02c5; // ratio 2 << 8 | QDIV_MODE 0x80 | KDIV_3 0x40 | PDIV_2 0x4 | central 1
    assert_eq!(wrpll_freq_khz(2, cfgcr1, cfgcr2), Ok(148_500));
    // without the Q divider: P1 = 1  -> 297000 (4K30 pixel clock)
    assert_eq!(wrpll_freq_khz(2, cfgcr1, 0x0000_0045), Ok(297_000));
    // PDIV family: 0 -> 1, 1 -> 2, 2 -> 3, 4 -> 7, 5 (invalid) -> 7 (treated as 7), 3/6/7 undefined
    for (pdiv, p0) in [(0u32, Some(1u32)), (1, Some(2)), (2, Some(3)), (3, None), (4, Some(7)), (5, Some(7)), (6, None), (7, None)] {
        let c2 = pdiv << 2; // KDIV_5, no Q divider
        let r = wrpll_freq_khz(3, 0x0000_0100, c2);
        match p0 {
            Some(p) => assert_eq!(r, Ok(256 * 24_000 / (p * 5 * 5)), "pdiv field {pdiv}"),
            None => assert_eq!(r, Err(ReadoutError::WrpllPdivReserved { pll: 3, field: pdiv })),
        }
    }
    // KDIV family: 0 -> 5, 1 -> 2, 2 -> 3, 3 -> 1
    for (kdiv, p2) in [(0u32, 5u32), (1, 2), (2, 3), (3, 1)] {
        let r = wrpll_freq_khz(2, 0x0000_0100, kdiv << 5);
        assert_eq!(r, Ok(256 * 24_000 / (p2 * 5)), "kdiv field {kdiv}");
    }
    // Q divider ratio 0 with QDIV_MODE on is a zero divider -> named error
    assert_eq!(wrpll_freq_khz(2, 0x0000_0100, 0x80), Err(ReadoutError::WrpllDividerZero { pll: 2 }));
    // DCO fraction: integer 0, fraction 0x4000 (0.5) -> 12000 kHz DCO, /(1*1*5*5) = 480
    assert_eq!(wrpll_freq_khz(2, 0x4000 << 9, 0), Ok(12_000 / 25));
    // integer field is 9 bits (0x1ff), not 10
    assert_eq!(wrpll_freq_khz(2, 0x3ff, 0), Ok((0x1ff * 24_000) / 25));
}

#[test]
fn wrpll_through_the_pll_decode() {
    // DPLL2 (a WRPLL) in HDMI mode, enabled: ctl 0x8000_0000, CTRL1 slice for id 2 = HDMI (bit 17) | override (bit 12)
    let s = decode_dpll(2, 0x8000_0000, 0x1_0000, 0x2_1000, 0x8040_0173, 0x0000_02c5).unwrap();
    assert!(s.hdmi_mode && s.override_set && s.locked);
    assert_eq!(s.port_clock_khz, 148_500);
    assert_eq!((s.cfgcr1, s.cfgcr2), (0x8040_0173, 0x0000_02c5));
    // HDMI mode off for the same PLL: CFGCR is not consulted (DP mode, rate field 0 -> 540000)
    let s = decode_dpll(2, 0x8000_0000, 0, 0x1000, 0x8040_0173, 0x0000_02c5).unwrap();
    assert!(!s.hdmi_mode);
    assert_eq!(s.port_clock_khz, 540_000);
    assert_eq!((s.cfgcr1, s.cfgcr2), (0, 0));
}

#[test]
fn cdclk_table_all_cells() {
    // intel_cdclk.c:1006-1066. rows: (DPLL0 link-rate field, VCO), columns: CDCLK_FREQ_SEL 0..3
    let rows: [(u32, u32, [u32; 4]); 6] = [
        (0, 8_100_000, [450_000, 540_000, 337_500, 675_000]), // 2700
        (1, 8_100_000, [450_000, 540_000, 337_500, 675_000]), // 1350
        (2, 8_100_000, [450_000, 540_000, 337_500, 675_000]), // 810
        (3, 8_100_000, [450_000, 540_000, 337_500, 675_000]), // 1620
        (4, 8_640_000, [432_000, 540_000, 308_571, 617_143]), // 1080
        (5, 8_640_000, [432_000, 540_000, 308_571, 617_143]), // 2160
    ];
    for (field, vco, cd) in rows {
        for sel in 0..4u32 {
            // LCPLL1 enabled + locked; CTRL1: override | rate field << 1; CDCLK_CTL: select << 26
            let c = decode_cdclk(0xc000_0000, 1 | (field << 1), sel << 26);
            assert_eq!(c.vco_khz, vco, "rate {field}");
            assert_eq!(c.cdclk_khz, cd[sel as usize], "rate {field} sel {sel}");
            assert_eq!(c.anomaly, CdclkAnomaly::None);
            assert_eq!(c.ref_khz, 24_000);
        }
    }
}

#[test]
fn cdclk_bypass_and_anomalies() {
    // DPLL0 off: bypass, 24 MHz (intel_cdclk.c:974-975, :1013-1015)
    let c = decode_cdclk(0, 0x3, 0x0400_0000);
    assert_eq!((c.vco_khz, c.cdclk_khz, c.anomaly), (0, 24_000, CdclkAnomaly::None));
    // enabled, not locked: warn + bypass
    let c = decode_cdclk(0x8000_0000, 0x3, 0);
    assert_eq!((c.vco_khz, c.cdclk_khz, c.anomaly), (0, 24_000, CdclkAnomaly::Dpll0NotLocked));
    // locked but HDMI mode / SSC set or override clear: unexpected CTRL1
    for ctrl1 in [0x21u32, 0x11, 0x2] {
        let c = decode_cdclk(0xc000_0000, ctrl1, 0);
        assert_eq!(c.cdclk_khz, 24_000, "ctrl1 {ctrl1:#x}");
        assert_eq!(c.anomaly, CdclkAnomaly::Dpll0Ctrl1Unexpected { ctrl1 }, "ctrl1 {ctrl1:#x}");
    }
    // the reported slice is DPLL0's six bits only: other PLLs' bits above it are not part of it
    let c = decode_cdclk(0xc000_0000, 0xffff_ffc0 | 0x21, 0);
    assert_eq!(c.anomaly, CdclkAnomaly::Dpll0Ctrl1Unexpected { ctrl1: 0x21 });
    let c = decode_cdclk(0xc000_0000, 0x40 | 0x2, 0); // override clear, DPLL1 bit set
    assert_eq!(c.anomaly, CdclkAnomaly::Dpll0Ctrl1Unexpected { ctrl1: 0x2 });
    // reserved link-rate fields 6 and 7
    for f in [6u32, 7] {
        let c = decode_cdclk(0xc000_0000, 1 | (f << 1), 0);
        assert_eq!((c.vco_khz, c.cdclk_khz), (0, 24_000));
        assert_eq!(c.anomaly, CdclkAnomaly::Dpll0LinkRateReserved { field: f });
    }
    // another PLL's CTRL1 bits are not DPLL0's: DPLL1 slice set with DPLL0 rate 1350
    let c = decode_cdclk(0xc000_0000, 0x3 | (0x3f << 6), 0);
    assert_eq!(c.anomaly, CdclkAnomaly::None);
    assert_eq!(c.cdclk_khz, 450_000);
}

#[test]
fn cdclk_voltage_level_and_decimal() {
    // skl_calc_voltage_level (intel_cdclk.c:953-963): > 540000 -> 3, > 450000 -> 2, > 337500 -> 1, else 0
    for (khz, lvl) in [
        (24_000, 0),
        (308_571, 0),
        (337_500, 0),
        (337_501, 1),
        (432_000, 1),
        (450_000, 1),
        (450_001, 2),
        (540_000, 2),
        (540_001, 3),
        (617_143, 3),
        (675_000, 3),
    ] {
        assert_eq!(voltage_level(khz), lvl, "{khz} kHz");
    }
    // and wired into the decode
    assert_eq!(decode_cdclk(0xc000_0000, 1, 0x0c00_0000).voltage_level, 3); // 675000 on the 8.1 GHz VCO
    assert_eq!(decode_cdclk(0xc000_0000, 1, 0).voltage_level, 1); // 450000
    // skl_cdclk_decimal (intel_cdclk.c:1068-1071): (cdclk - 1000) / 500 rounded to nearest
    assert_eq!(cdclk_decimal(450_000), 898);
    assert_eq!(cdclk_decimal(540_000), 1078);
    assert_eq!(cdclk_decimal(337_500), 673);
    assert_eq!(cdclk_decimal(308_571), 615); // 307571 / 500 = 615.14
    assert_eq!(cdclk_decimal(617_143), 1232); // 616143 / 500 = 1232.29
    // DIV_ROUND_CLOSEST rounds a half up, and the 1000 kHz offset is exact: probe both sides of the first boundary
    assert_eq!(cdclk_decimal(1_249), 0); // 249 / 500 = 0.498
    assert_eq!(cdclk_decimal(1_250), 1); // 250 / 500 = 0.5 -> 1
    assert_eq!(cdclk_decimal(1_749), 1); // 749 / 500 = 1.498
    assert_eq!(cdclk_decimal(1_750), 2); // 750 / 500 = 1.5 -> 2
    // the raw field is surfaced, masked to 11 bits
    assert_eq!(decode_cdclk(0xc000_0000, 1, 0xffff_ffff).decimal, 0x7ff);
    assert_eq!(decode_cdclk(0xc000_0000, 1, 0x382).decimal, 0x382);
}
