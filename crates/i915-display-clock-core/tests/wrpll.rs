// SPDX-License-Identifier: GPL-2.0-only
//! WRPLL divider solver vs Linux's `skl_ddi_calculate_wrpll` / `skl_wrpll_params_populate` / `skl_ddi_hdmi_pll_dividers`.
//!
//! The expected values are Linux's. HOW they were obtained, so they can be re-derived: lines 1515-1752 of
//! linux-ref/drivers/gpu/drm/i915/display/intel_dpll_mgr.c (v7.0-rc6) were compiled UNMODIFIED with gcc behind a 12-line shim
//! (u64/u32/u8, div64_u64, div_u64, abs_diff, KHz/MHz, WARN) and run with `ref_clock = 24000`. The vectors below are its output; the
//! first three are also derived by hand in the comments.
use i915_display_clock_core::error::ClockError;
use i915_display_clock_core::regs::*;
use i915_display_clock_core::wrpll::*;
use i915_kbl_display_core::dpll::wrpll_freq_khz;

fn p(clock: u32) -> WrpllParams {
    calculate_wrpll(clock, 24000).unwrap_or_else(|e| panic!("{clock} kHz: {}", e.name()))
}

fn want(frac: u32, int: u32, qr: u32, qm: u32, kdiv: u32, pdiv: u32, central: u32) -> WrpllParams {
    WrpllParams { dco_fraction: frac, dco_integer: int, qdiv_ratio: qr, qdiv_mode: qm, kdiv, pdiv, central_freq: central }
}

#[test]
fn hdmi_148_5_mhz_by_hand() {
    // 148.5 MHz: AFE = 148500 kHz * 1000 * 5 = 742.5 MHz (intel_dpll_mgr.c:1708). Even dividers are tried first. Divider 12 puts the
    // DCO at 12 * 742.5 = 8.91 GHz, 90 MHz below the 9.0 GHz central: deviation = 10000 * 90e6 / 9e9 = 100 (0.01 % units, :1533-1534),
    // inside the -6 % window (< 600, :1546-1547) and the best any divider reaches (dividers 4..10 land far outside either window).
    // Multipliers for 12 (:1564-1567): half = 6 -> even -> (p0, p1, p2) = (2, 3, 2) (:1568-1571).
    // populate (:1614-1680): central 9.0 GHz -> 1 (:1626); p0 = 2 -> pdiv 1 (:1637); p2 = 2 -> kdiv 1 (:1654); qdiv_ratio = 3,
    // qdiv_mode = 1 (:1667-1668); DCO = 2*3*2 * 742.5 MHz = 8.91 GHz; integer = 8.91e9 / 24e6 = 371.25 -> 371 (:1676);
    // fraction = ((8.91e9 / 24 000) - 371e6) * 0x8000 / 1e6 = (371 250 000 - 371 000 000) * 32768 / 1e6 = 8192 (:1677-1679).
    assert_eq!(p(148_500), want(8192, 371, 3, 1, 1, 1, 1));
}

#[test]
fn hdmi_297_mhz_by_hand() {
    // 297 MHz: AFE = 1.485 GHz. 6 * 1.485 = 8.91 GHz again (deviation 100 to 9.0 GHz); no smaller even divider fits (4 -> 5.94 GHz).
    // Multipliers for 6: half = 3 -> (2, 1, 3) (:1564-1567). pdiv 1, kdiv: p2 = 3 -> 2 (:1657), qdiv_ratio 1 -> qdiv_mode 0 (:1668).
    // DCO = 2*1*3 * 1.485 GHz = 8.91 GHz -> integer 371, fraction 8192 as above.
    assert_eq!(p(297_000), want(8192, 371, 1, 0, 2, 1, 1));
}

#[test]
fn hdmi_594_mhz_by_hand() {
    // 594 MHz: AFE = 2.97 GHz. No EVEN divider reaches a DCO window (4 -> 11.88 GHz is 23.8 % above 9.6 GHz, 2 would be 5.94 but is
    // not in the list), so the odd list is used (:1730-1736 only breaks out of the even pass if it found something): 3 -> 8.91 GHz,
    // deviation 100 against 9.0 GHz. Multipliers for 3 (:1581-1584): (3, 1, 1): pdiv 2 (:1640), kdiv 3 (:1660), qdiv 1 / mode 0.
    assert_eq!(p(594_000), want(8192, 371, 1, 0, 3, 2, 1));
}

#[test]
fn hdmi_25_175_mhz_the_vga_clock() {
    // 25.175 MHz: AFE = 125.875 MHz; divider 76 -> DCO 9 566 500 000 Hz, 33.5 MHz below 9.6 GHz: deviation = 10000 * 33.5e6 / 9.6e9
    // = 34 (truncated), the smallest of any divider. Multipliers for 76: half 38 even -> (2, 19, 2). central 9.6 GHz -> 0 (:1623-1625).
    // integer = 9 566 500 000 / 24e6 = 398; fraction = (9 566 500 000 / 24 000 - 398e6) * 0x8000 / 1e6 = (398 604 166 - 398 000 000)
    // * 32768 / 1e6 = 19797 (truncated).
    assert_eq!(p(25_175), want(19797, 398, 19, 1, 1, 1, 0));
}

#[test]
fn more_standard_clocks_from_the_compiled_linux_source() {
    // (clock kHz, frac, int, qdiv_ratio, qdiv_mode, kdiv, pdiv, central) - output of the compiled intel_dpll_mgr.c.
    let v: [(u32, u32, u32, u32, u32, u32, u32, u32); 9] = [
        (25_200, 0, 399, 19, 1, 1, 1, 0),
        (27_000, 8192, 371, 11, 1, 1, 2, 1),
        (74_250, 8192, 371, 6, 1, 1, 1, 1),
        (148_352, 28835, 370, 3, 1, 1, 1, 1),
        (108_000, 0, 360, 4, 1, 1, 1, 1),
        (65_000, 5461, 379, 7, 1, 1, 1, 0),
        (25_000, 0, 375, 18, 1, 1, 1, 1),
        (600_000, 0, 375, 1, 0, 3, 2, 1),
        (297_000, 8192, 371, 1, 0, 2, 1, 1),
    ];
    for (c, f, i, qr, qm, k, pd, ce) in v {
        assert_eq!(p(c), want(f, i, qr, qm, k, pd, ce), "{c} kHz");
    }
}

#[test]
fn packed_registers_for_148_5() {
    // skl_ddi_hdmi_pll_dividers (:1842-1856). ctrl1 = OVERRIDE(0) | HDMI_MODE(0) = bit 0 | bit 5.
    let (hw, port_clock) = hdmi_hw_state(148_500).unwrap();
    assert_eq!(hw.ctrl1, 0x0000_0021);
    // cfgcr1 = FREQ_ENABLE (bit 31) | fraction 8192 << 9 (0x400000) | integer 371 (0x173)
    assert_eq!(hw.cfgcr1, DPLL_CFGCR1_FREQ_ENABLE | (8192 << 9) | 371);
    assert_eq!(hw.cfgcr1, 0x8040_0173);
    // cfgcr2 = qdiv_ratio 3 << 8 | qdiv_mode 1 << 7 | kdiv 1 << 5 | pdiv 1 << 2 | central 1
    assert_eq!(hw.cfgcr2, 0x0000_03a5);
    assert_eq!(hw.cfgcr2, (3 << 8) | DPLL_CFGCR2_QDIV_MODE_BIT | DPLL_CFGCR2_KDIV_2 | DPLL_CFGCR2_PDIV_2 | 1);
    // and skl_ddi_wrpll_get_freq (:1754-1824) of those words gives back the pixel clock: dco = 371 * 24000 + 8192 * 24000 / 0x8000 =
    // 8 904 000 + 6 000 = 8 910 000; / (2 * 3 * 2 * 5) = 148 500.
    assert_eq!(port_clock, 148_500);
    assert_eq!(wrpll_freq_khz(2, hw.cfgcr1, hw.cfgcr2).unwrap(), 148_500);
}

#[test]
fn packed_registers_for_297_and_594() {
    let (hw, pc) = hdmi_hw_state(297_000).unwrap();
    assert_eq!((hw.ctrl1, hw.cfgcr1, hw.cfgcr2, pc), (0x21, 0x8040_0173, (1 << 8) | (2 << 5) | (1 << 2) | 1, 297_000));
    let (hw, pc) = hdmi_hw_state(594_000).unwrap();
    assert_eq!((hw.ctrl1, hw.cfgcr1, hw.cfgcr2, pc), (0x21, 0x8040_0173, (1 << 8) | (3 << 5) | (2 << 2) | 1, 594_000));
    // 25.175: the programmed state's own port clock is 25174 (the truncating integer division of :1823), which Linux stores back
    // into crtc_state->port_clock.
    let (_, pc) = hdmi_hw_state(25_175).unwrap();
    assert_eq!(pc, 25_174);
}

#[test]
fn multipliers_table_every_divider_in_both_lists() {
    // skl_wrpll_get_multipliers (:1555-1602), written out per divider. (p -> p0, p1, p2)
    let even: [(u32, (u32, u32, u32)); 36] = [
        (4, (2, 1, 2)), (6, (2, 1, 3)), (8, (2, 2, 2)), (10, (2, 1, 5)), (12, (2, 3, 2)), (14, (7, 1, 2)), (16, (2, 4, 2)),
        (18, (3, 3, 2)), (20, (2, 5, 2)), (24, (2, 6, 2)), (28, (2, 7, 2)), (30, (3, 5, 2)), (32, (2, 8, 2)), (36, (2, 9, 2)),
        (40, (2, 10, 2)), (42, (3, 7, 2)), (44, (2, 11, 2)), (48, (2, 12, 2)), (52, (2, 13, 2)), (54, (3, 9, 2)), (56, (2, 14, 2)),
        (60, (2, 15, 2)), (64, (2, 16, 2)), (66, (3, 11, 2)), (68, (2, 17, 2)), (70, (7, 5, 2)), (72, (2, 18, 2)), (76, (2, 19, 2)),
        (78, (3, 13, 2)), (80, (2, 20, 2)), (84, (2, 21, 2)), (88, (2, 22, 2)), (90, (3, 15, 2)), (92, (2, 23, 2)), (96, (2, 24, 2)),
        (98, (7, 7, 2)),
    ];
    for (i, (d, m)) in even.iter().enumerate() {
        assert_eq!(multipliers(*d), Some(*m), "even {d}");
        assert_eq!(EVEN_DIVIDERS[i] as u32, *d, "EVEN_DIVIDERS[{i}]");
    }
    let odd: [(u32, (u32, u32, u32)); 7] =
        [(3, (3, 1, 1)), (5, (5, 1, 1)), (7, (7, 1, 1)), (9, (3, 1, 3)), (15, (3, 1, 5)), (21, (7, 1, 3)), (35, (7, 1, 5))];
    for (i, (d, m)) in odd.iter().enumerate() {
        assert_eq!(multipliers(*d), Some(*m), "odd {d}");
        assert_eq!(ODD_DIVIDERS[i] as u32, *d, "ODD_DIVIDERS[{i}]");
    }
    assert_eq!(multipliers(11), None);
}

#[test]
fn populate_encodings_every_member() {
    // pdiv: P0 1,2,3,7 -> 0,1,2,4; kdiv: P2 5,2,3,1 -> 0,1,2,3; central: 9.6 / 9.0 / 8.4 GHz -> 0 / 1 / 3 (:1622-1662)
    for (p0, pd) in [(1, 0), (2, 1), (3, 2), (7, 4)] {
        assert_eq!(params_populate(1_000_000_000, 24000, 9_000_000_000, p0, 1, 1).unwrap().pdiv, pd, "p0 {p0}");
    }
    for (p2, kd) in [(5, 0), (2, 1), (3, 2), (1, 3)] {
        assert_eq!(params_populate(1_000_000_000, 24000, 9_000_000_000, 1, 1, p2).unwrap().kdiv, kd, "p2 {p2}");
    }
    for (c, cf) in [(9_600_000_000u64, 0), (9_000_000_000, 1), (8_400_000_000, 3)] {
        assert_eq!(params_populate(1_000_000_000, 24000, c, 1, 1, 1).unwrap().central_freq, cf, "central {c}");
    }
    // the WARN arms are refusals
    assert_eq!(params_populate(1, 24000, 9_000_000_000, 5, 1, 1), None); // "Incorrect PDiv"
    assert_eq!(params_populate(1, 24000, 9_000_000_000, 1, 1, 4), None); // "Incorrect KDiv"
    assert_eq!(params_populate(1, 24000, 1, 1, 1, 1), None);
    // qdiv_mode is 0 exactly when the ratio is 1
    assert_eq!(params_populate(1, 24000, 9_000_000_000, 1, 1, 1).unwrap().qdiv_mode, 0);
    assert_eq!(params_populate(1, 24000, 9_000_000_000, 1, 2, 1).unwrap().qdiv_mode, 1);
}

#[test]
fn refusals_are_named() {
    // 1 MHz: no divider reaches a DCO window (-EINVAL, :1739-1740)
    assert_eq!(calculate_wrpll(1000, 24000), Err(ClockError::WrpllNoDivider { clock_khz: 1000 }));
    assert_eq!(ClockError::WrpllNoDivider { clock_khz: 0 }.name(), "WRPLL-NO-DIVIDER");
    // 360 MHz: the best divider is 5, whose P0 = 5 has no PDIV encoding; Linux WARNs and programs PDIV = 0 anyway.
    assert_eq!(calculate_wrpll(360_000, 24000), Err(ClockError::WrpllDividerUnencodable { clock_khz: 360_000, divider: 5 }));
    assert_eq!(ClockError::WrpllDividerUnencodable { clock_khz: 0, divider: 0 }.name(), "WRPLL-DIVIDER-UNENCODABLE");
    assert!(hdmi_hw_state(360_000).is_err());
    // an absurd clock must not overflow
    assert!(calculate_wrpll(u32::MAX, 24000).is_err());
    assert!(calculate_wrpll(0, 24000).is_err());
}

const fn fnv(mut h: u64, v: u64) -> u64 {
    let mut i = 0;
    while i < 8 {
        h ^= (v >> (8 * i)) & 0xff;
        h = h.wrapping_mul(1_099_511_628_211);
        i += 1;
    }
    h
}

#[test]
fn every_clock_from_0_to_700_mhz_matches_the_compiled_linux_source() {
    // The same sweep run in C over the unmodified Linux function: FNV-1a over (clock, class, fraction, integer, qdiv_ratio, qdiv_mode,
    // kdiv, pdiv, central), class 0 = ok, 1 = -EINVAL, 2 = success-with-WARN (the unencodable divider). 700 001 clocks, 517 083 solved.
    // A single changed divider, window, tie-break or encoding anywhere in the range changes this number.
    let mut h = 14_695_981_039_346_656_037u64;
    let mut ok = 0u32;
    for clk in 0..=700_000u32 {
        h = fnv(h, clk as u64);
        match calculate_wrpll(clk, 24000) {
            Ok(q) => {
                ok += 1;
                h = fnv(h, 0);
                for v in [q.dco_fraction, q.dco_integer, q.qdiv_ratio, q.qdiv_mode, q.kdiv, q.pdiv, q.central_freq] {
                    h = fnv(h, v as u64);
                }
            }
            Err(ClockError::WrpllNoDivider { .. }) => h = fnv(h, 1),
            Err(ClockError::WrpllDividerUnencodable { .. }) => h = fnv(h, 2),
            Err(e) => panic!("{}", e.name()),
        }
    }
    assert_eq!(ok, 517_083);
    assert_eq!(h, 3_875_370_677_031_319_688);
}

#[test]
fn deviation_windows_and_central_frequencies() {
    // +1 % / -6 % (:1522-1524), in 0.01 % units
    assert_eq!(DCO_MAX_PDEVIATION, 100);
    assert_eq!(DCO_MAX_NDEVIATION, 600);
    assert_eq!(DCO_CENTRAL_FREQ_HZ, [8_400_000_000, 9_000_000_000, 9_600_000_000]);
}
