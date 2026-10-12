// SPDX-License-Identifier: GPL-2.0-only
//! CDCLK: step choice, minimum-clock solving, the max clamp, and the `skl_set_cdclk` transaction (exact steps, simulated end state,
//! rollback). Expected values are Linux's own literals, with the file:line they come from.
mod common;
use common::*;
use i915_common_core::Reg;
use i915_display_clock_core::cdclk::*;
use i915_display_clock_core::error::{Bound, Handshake};
use i915_display_clock_core::pcode::{PcodeOp, PcodeRequest, PcodeWrite};
use i915_display_clock_core::plan::{OnTimeout, PollPlan, Step, Transaction};
use i915_display_clock_core::regs::*;
use i915_display_clock_core::ClockError;

#[test]
fn calc_cdclk_every_threshold_both_tables() {
    // skl_calc_cdclk (intel_cdclk.c:930-951): (min_cdclk, vco) -> cdclk, at each boundary and one kHz above it.
    let t8640: [(u32, u32); 10] = [
        (0, 308571), (308571, 308571), (308572, 432000), (432000, 432000), (432001, 540000), (540000, 540000), (540001, 617143),
        (617143, 617143), (700000, 617143), (1, 308571),
    ];
    for (min, want) in t8640 {
        assert_eq!(calc_cdclk(min, 8_640_000), want, "8640: min {min}");
    }
    let t8100: [(u32, u32); 10] = [
        (0, 337500), (337500, 337500), (337501, 450000), (450000, 450000), (450001, 540000), (540000, 540000), (540001, 675000),
        (675000, 675000), (700000, 675000), (1, 337500),
    ];
    for (min, want) in t8100 {
        assert_eq!(calc_cdclk(min, 8_100_000), want, "8100: min {min}");
    }
}

#[test]
fn cdclk_for_standard_pixel_rates() {
    // intel_pixel_rate_to_cdclk on display ver 9: DIV_ROUND_UP(pixel_rate * 100, guardband 100 * ppc 1) = pixel_rate
    // (intel_cdclk.c:2869 ppc = 1, :2877 guardband = 100, :2890).
    let rates = [
        (25_175u32, 25_175u32), (74_250, 74_250), (148_500, 148_500), (241_500, 241_500), (297_000, 297_000), (533_250, 533_250), (594_000, 594_000),
    ];
    for (px, min) in rates {
        assert_eq!(pixel_rate_to_cdclk(px), min, "pixel rate {px}");
    }
    // ... and the step each lands on, VCO 8100 (DP / HDMI, the usual) and 8640 (eDP 1.4 rates).
    //   1080p60 (148.5 MHz) -> the smallest step; 1440p60 (241.5) -> still the smallest; 4K30 (297) -> the smallest;
    //   4K60 reduced blanking (533.25) -> 540 MHz; 4K60 (594) -> 675 (8100) or 617.143 (8640).
    let want_8100 = [337500, 337500, 337500, 337500, 337500, 540000, 675000];
    let want_8640 = [308571, 308571, 308571, 308571, 308571, 540000, 617143];
    for (i, (px, _)) in rates.iter().enumerate() {
        let min = pixel_rate_to_cdclk(*px);
        assert_eq!(calc_cdclk(min, 8_100_000), want_8100[i], "{px} @ 8100");
        assert_eq!(calc_cdclk(min, 8_640_000), want_8640[i], "{px} @ 8640");
    }
}

#[test]
fn pixel_rate_rounds_up_at_the_guardband() {
    // DIV_ROUND_UP(x * 100, 100) never rounds, but the u64 widening must not overflow at the top of the u32 range.
    assert_eq!(pixel_rate_to_cdclk(u32::MAX), u32::MAX);
    assert_eq!(pixel_rate_to_cdclk(0), 0);
    assert_eq!(pixel_rate_to_cdclk(1), 1);
}

#[test]
fn audio_minimums() {
    // intel_audio_min_cdclk (intel_audio.c:996-1040) on display ver 9
    let hdmi_audio = CrtcLoad { pixel_rate_khz: 25_175, has_audio: true, dp_link: None };
    let dp_hbr2_x4 = CrtcLoad { pixel_rate_khz: 25_175, has_audio: true, dp_link: Some((540_000, 4)) };
    let dp_hbr2_x2 = CrtcLoad { pixel_rate_khz: 25_175, has_audio: true, dp_link: Some((540_000, 2)) };
    let dp_hbr_x4 = CrtcLoad { pixel_rate_khz: 25_175, has_audio: true, dp_link: Some((270_000, 4)) };
    let no_audio = CrtcLoad { pixel_rate_khz: 25_175, has_audio: false, dp_link: Some((540_000, 4)) };
    assert_eq!(audio_min_cdclk(&hdmi_audio), 192_000); // 2 * 96000, intel_audio.c:1021-1026
    assert_eq!(audio_min_cdclk(&dp_hbr2_x4), 432_000); // WA #1144, intel_audio.c:1015-1018
    assert_eq!(audio_min_cdclk(&dp_hbr2_x2), 192_000);
    assert_eq!(audio_min_cdclk(&dp_hbr_x4), 192_000);
    assert_eq!(audio_min_cdclk(&no_audio), 0); // intel_audio.c:1001
    // and the CRTC minimum is the larger of pixel-rate and audio
    assert_eq!(crtc_min_cdclk(&hdmi_audio), 192_000);
    assert_eq!(crtc_min_cdclk(&CrtcLoad { pixel_rate_khz: 297_000, ..hdmi_audio }), 297_000);
    assert_eq!(crtc_min_cdclk(&no_audio), 25_175);
}

#[test]
fn max_cdclk_from_the_dfsm_fuse() {
    // intel_update_max_cdclk (intel_cdclk.c:3668-3680): the fuse picks the 8640 guess, skl_calc_cdclk corrects it for the VCO.
    let rows = [
        (SKL_DFSM_CDCLK_LIMIT_675, 675_000, 617_143),
        (SKL_DFSM_CDCLK_LIMIT_540, 540_000, 540_000),
        (SKL_DFSM_CDCLK_LIMIT_450, 450_000, 432_000),
        (SKL_DFSM_CDCLK_LIMIT_337_5, 337_500, 308_571),
    ];
    for (limit, at8100, at8640) in rows {
        assert_eq!(max_cdclk(limit, 8_100_000), at8100, "limit {limit:#x} @ 8100");
        assert_eq!(max_cdclk(limit, 8_640_000), at8640, "limit {limit:#x} @ 8640");
    }
    // other DFSM bits do not disturb the field
    assert_eq!(max_cdclk(SKL_DFSM_CDCLK_LIMIT_540 | 0x0800_0000 | 0x4000_0000, 8_100_000), 540_000);
    // the field values themselves (intel_display_regs.h:1542-1546)
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_MASK, 3 << 23);
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_675, 0);
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_540, 1 << 23);
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_450, 2 << 23);
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_337_5, 3 << 23);
}

#[test]
fn min_cdclk_is_the_largest_demand_and_refuses_above_max() {
    let c = |px, au| CrtcLoad { pixel_rate_khz: px, has_audio: au, dp_link: None };
    assert_eq!(compute_min_cdclk(&[c(148_500, false), c(297_000, false)], 0, 675_000), Ok(297_000));
    assert_eq!(compute_min_cdclk(&[c(148_500, true)], 0, 675_000), Ok(192_000));
    assert_eq!(compute_min_cdclk(&[c(148_500, false)], 400_000, 675_000), Ok(400_000)); // force_min_cdclk
    assert_eq!(compute_min_cdclk(&[], 0, 675_000), Ok(0));
    // intel_cdclk.c:3048-3053
    let e = compute_min_cdclk(&[c(594_000, false)], 0, 540_000).unwrap_err();
    assert_eq!(e, ClockError::CdclkAboveMax { min_khz: 594_000, max_khz: 540_000 });
    assert_eq!(e.name(), "CDCLK-ABOVE-MAX");
    // exactly at the max is allowed
    assert_eq!(compute_min_cdclk(&[c(540_000, false)], 0, 540_000), Ok(540_000));
}

#[test]
fn dpll0_vco_follows_the_edp_link_rate() {
    // skl_dpll0_vco (intel_cdclk.c:3167-3203): port_clock / 2 in {108000, 216000} -> 8640, any other eDP rate -> 8100
    assert_eq!(dpll0_vco(0, 8_100_000, None), 8_100_000); // falls back to the preferred
    assert_eq!(dpll0_vco(8_640_000, 8_100_000, None), 8_640_000); // keeps the logical
    assert_eq!(dpll0_vco(8_100_000, 8_100_000, Some(216_000)), 8_640_000);
    assert_eq!(dpll0_vco(8_100_000, 8_100_000, Some(432_000)), 8_640_000);
    for rate in [162_000, 270_000, 324_000, 540_000] {
        assert_eq!(dpll0_vco(8_640_000, 8_640_000, Some(rate)), 8_100_000, "eDP {rate}");
    }
}

#[test]
fn modeset_calc_picks_step_level_and_checks_vco_and_max() {
    // skl_modeset_calc_cdclk (intel_cdclk.c:3205-3230): level = skl_calc_voltage_level(cdclk) (intel_cdclk.c:953-963)
    let c = modeset_calc_cdclk(148_500, 8_100_000, SKL_DFSM_CDCLK_LIMIT_675).unwrap();
    assert_eq!(c, CdclkConfig { cdclk_khz: 337_500, vco_khz: 8_100_000, voltage_level: 0 });
    let c = modeset_calc_cdclk(400_000, 8_100_000, SKL_DFSM_CDCLK_LIMIT_675).unwrap();
    assert_eq!(c, CdclkConfig { cdclk_khz: 450_000, vco_khz: 8_100_000, voltage_level: 1 });
    let c = modeset_calc_cdclk(533_250, 8_100_000, SKL_DFSM_CDCLK_LIMIT_675).unwrap();
    assert_eq!(c, CdclkConfig { cdclk_khz: 540_000, vco_khz: 8_100_000, voltage_level: 2 });
    let c = modeset_calc_cdclk(594_000, 8_100_000, SKL_DFSM_CDCLK_LIMIT_675).unwrap();
    assert_eq!(c, CdclkConfig { cdclk_khz: 675_000, vco_khz: 8_100_000, voltage_level: 3 });
    // fuse-limited part: 594 MHz needed, 540 MHz allowed
    assert_eq!(
        modeset_calc_cdclk(594_000, 8_100_000, SKL_DFSM_CDCLK_LIMIT_540),
        Err(ClockError::CdclkAboveMax { min_khz: 594_000, max_khz: 540_000 })
    );
    assert_eq!(modeset_calc_cdclk(1, 8_000_000, 0), Err(ClockError::CdclkVcoInvalid { vco_khz: 8_000_000 }));
    assert_eq!(ClockError::CdclkVcoInvalid { vco_khz: 0 }.name(), "CDCLK-VCO-INVALID");
}

#[test]
fn init_target_is_the_smallest_step_of_the_preferred_vco() {
    // skl_cdclk_init_hw (intel_cdclk.c:1292-1296)
    assert_eq!(init_target(0), CdclkConfig { cdclk_khz: 337_500, vco_khz: 8_100_000, voltage_level: 0 });
    assert_eq!(init_target(8_640_000), CdclkConfig { cdclk_khz: 308_571, vco_khz: 8_640_000, voltage_level: 0 });
}

#[test]
fn freq_sel_every_member_and_every_refusal() {
    // skl_cdclk_freq_sel (intel_cdclk.c:1135-1156), by (cdclk, vco)
    let ok = [
        (308_571, 8_640_000, CDCLK_FREQ_337_308), (337_500, 8_100_000, CDCLK_FREQ_337_308),
        (432_000, 8_640_000, CDCLK_FREQ_450_432), (450_000, 8_100_000, CDCLK_FREQ_450_432),
        (540_000, 8_640_000, CDCLK_FREQ_540), (540_000, 8_100_000, CDCLK_FREQ_540),
        (617_143, 8_640_000, CDCLK_FREQ_675_617), (675_000, 8_100_000, CDCLK_FREQ_675_617),
    ];
    for (c, v, sel) in ok {
        assert_eq!(freq_sel(c, v), Ok(sel), "{c} @ {v}");
    }
    // the select values themselves (intel_display_regs.h:2520-2523): field 27:26
    assert_eq!([CDCLK_FREQ_450_432, CDCLK_FREQ_540, CDCLK_FREQ_337_308, CDCLK_FREQ_675_617], [0, 1 << 26, 2 << 26, 3 << 26]);
    // a step of the OTHER table, and a frequency that is no step at all
    for (c, v) in [(450_000, 8_640_000), (432_000, 8_100_000), (675_000, 8_640_000), (24_000, 0), (500_000, 8_100_000)] {
        assert_eq!(freq_sel(c, v), Err(ClockError::CdclkNotAStep { cdclk_khz: c, vco_khz: v }), "{c} @ {v}");
    }
}

fn ctl_for(cdclk: u32, vco: u32) -> u32 {
    freq_sel(cdclk, vco).unwrap() | cdclk_decimal(cdclk)
}

#[test]
fn decimal_and_ctl_words_are_linuxs() {
    // skl_cdclk_decimal = DIV_ROUND_CLOSEST(cdclk - 1000, 500) (intel_cdclk.c:1068-1071), written out per step.
    assert_eq!(ctl_for(337_500, 8_100_000), 0x0800_02a1); // 673
    assert_eq!(ctl_for(450_000, 8_100_000), 0x0000_0382); // 898
    assert_eq!(ctl_for(540_000, 8_100_000), 0x0400_0436); // 1078
    assert_eq!(ctl_for(675_000, 8_100_000), 0x0c00_0544); // 1348
    assert_eq!(ctl_for(308_571, 8_640_000), 0x0800_0267); // 615
    assert_eq!(ctl_for(432_000, 8_640_000), 0x0000_035e); // 862
    assert_eq!(ctl_for(617_143, 8_640_000), 0x0c00_04d0); // 1232
}

fn rd(o: u32) -> Reg {
    Reg::new(o)
}
fn hw(vco: u32, cdclk: u32) -> CdclkHw {
    CdclkHw { vco_khz: vco, cdclk_khz: cdclk, voltage_level: voltage_level(cdclk), cdclk_ctl: ctl_for(cdclk, vco) }
}
fn cfg(vco: u32, cdclk: u32) -> CdclkConfig {
    CdclkConfig { cdclk_khz: cdclk, vco_khz: vco, voltage_level: voltage_level(cdclk) }
}
const OVR: u32 = CDCLK_DIVMUX_CD_OVERRIDE;
const SEL_DEC: u32 = CDCLK_FREQ_SEL_MASK | CDCLK_FREQ_DECIMAL_MASK;

#[test]
fn same_vco_change_is_eight_steps_in_linux_order() {
    // skl_set_cdclk (intel_cdclk.c:1158-1228), 337.5 -> 450 MHz on VCO 8100: no PLL work.
    let h = hw(8_100_000, 337_500);
    let t = plan_set_cdclk(&h, &cfg(8_100_000, 450_000), false).unwrap();
    let c0 = h.cdclk_ctl; // 0x080002a1
    let with_ovr = c0 | OVR;
    let cleared = with_ovr & !SEL_DEC;
    let want = [
        Step::Pcode(PcodeOp::Request(PcodeRequest::new(7, 3, 1, 1, 3))), // :1178-1181 (mbox 7, PREPARE 3 -> READY 1, 3 ms)
        Step::Write { reg: rd(CDCLK_CTL), value: with_ovr },             // :1204-1205
        Step::PostingRead { reg: rd(CDCLK_CTL) },                         // :1206
        Step::Write { reg: rd(CDCLK_CTL), value: cleared },               // :1212-1213
        Step::Write { reg: rd(CDCLK_CTL), value: cleared | 0x382 },       // :1215-1217 (450_432 select = 0, decimal 898)
        Step::Write { reg: rd(CDCLK_CTL), value: 0x382 },                 // :1219-1221
        Step::PostingRead { reg: rd(CDCLK_CTL) },                         // :1221
        Step::Pcode(PcodeOp::Write(PcodeWrite { mbox: 7, val: 1, timeout_ms: 1 })), // :1224-1225, level of 450 MHz = 1
    ];
    assert_eq!(t.forward.len(), want.len());
    for (i, w) in want.iter().enumerate() {
        assert_eq!(t.forward.get(i), Some(w), "step {i}");
    }
}

#[test]
fn vco_change_is_fifteen_steps_with_pll_off_and_on() {
    // 450 MHz @ 8100 -> 432 MHz @ 8640 (a KBL panel with an eDP 1.4 rate).
    let h = hw(8_100_000, 450_000);
    let t = plan_set_cdclk(&h, &cfg(8_640_000, 432_000), false).unwrap();
    let want_ctl = 0x35e;
    let want = [
        Step::Pcode(PcodeOp::Request(PcodeRequest::cdclk_prepare())),
        Step::Rmw { reg: rd(LCPLL1_CTL), clear: LCPLL_PLL_ENABLE, set: 0 }, // skl_dpll0_disable :1126-1127
        Step::Poll(PollPlan::wait_clear(Handshake::Dpll0Unlock, rd(LCPLL1_CTL), LCPLL_PLL_LOCK, 1, OnTimeout::Fail)), // :1129
        Step::Write { reg: rd(CDCLK_CTL), value: want_ctl },                // :1196-1200
        Step::Write { reg: rd(CDCLK_CTL), value: want_ctl | OVR },           // :1204
        Step::PostingRead { reg: rd(CDCLK_CTL) },
        // skl_dpll0_enable (:1102-1122): CTRL1 clear HDMI|SSC|RATE (0x3e), set OVERRIDE | 1080 << 1 = 0x9
        Step::Rmw { reg: rd(DPLL_CTRL1), clear: 0x3e, set: 0x9 },
        Step::PostingRead { reg: rd(DPLL_CTRL1) },
        Step::Rmw { reg: rd(LCPLL1_CTL), clear: 0, set: LCPLL_PLL_ENABLE },
        Step::Poll(PollPlan::wait_set(Handshake::Dpll0Lock, rd(LCPLL1_CTL), LCPLL_PLL_LOCK, 5, OnTimeout::Fail)),
        Step::Write { reg: rd(CDCLK_CTL), value: OVR },
        Step::Write { reg: rd(CDCLK_CTL), value: OVR | want_ctl },
        Step::Write { reg: rd(CDCLK_CTL), value: want_ctl },
        Step::PostingRead { reg: rd(CDCLK_CTL) },
        Step::Pcode(PcodeOp::Write(PcodeWrite::new(7, 1))),
    ];
    assert_eq!(t.forward.len(), 15);
    for (i, w) in want.iter().enumerate() {
        assert_eq!(t.forward.get(i), Some(w), "step {i}");
    }
}

#[test]
fn dpll0_off_to_on_skips_the_disable() {
    // hw.vco == 0 (firmware left DPLL0 off): no skl_dpll0_disable (`hw.vco != 0 && hw.vco != vco`, intel_cdclk.c:1190-1191)
    let h = CdclkHw { vco_khz: 0, cdclk_khz: BYPASS_KHZ, voltage_level: 0, cdclk_ctl: 0 };
    let t = plan_set_cdclk(&h, &cfg(8_100_000, 337_500), false).unwrap();
    assert!(t.forward.iter().all(|s| !matches!(s, Step::Poll(p) if p.what == Handshake::Dpll0Unlock)));
    assert!(t.forward.iter().any(|s| matches!(s, Step::Poll(p) if p.what == Handshake::Dpll0Lock)));
    assert_eq!(t.forward.len(), 13);
}

#[test]
fn vco_link_rate_field_is_the_lowest_for_the_vco() {
    // skl_dpll0_link_rate (intel_cdclk.c:1083-1100): 810 for 8100, 1080 for 8640.
    for (vco, field) in [(8_100_000u32, 2u32), (8_640_000, 4)] {
        let cdclk = calc_cdclk(0, vco);
        let off = CdclkHw { vco_khz: 0, cdclk_khz: BYPASS_KHZ, voltage_level: 0, cdclk_ctl: 0 };
        let t = plan_set_cdclk(&off, &cfg(vco, cdclk), false).unwrap();
        let rmw = t.forward.iter().find_map(|s| match s {
            Step::Rmw { reg, clear, set } if reg.offset() == DPLL_CTRL1 => Some((*clear, *set)),
            _ => None,
        });
        assert_eq!(rmw, Some((0x3e, 1 | (field << 1))), "vco {vco}");
    }
}

#[test]
fn refusals_are_named() {
    let h = hw(8_100_000, 337_500);
    assert_eq!(
        plan_set_cdclk(&h, &cfg(8_000_000, 337_500), false).unwrap_err(),
        ClockError::CdclkVcoInvalid { vco_khz: 8_000_000 }
    );
    // Skylake with the 8640 VCO (WA #1183, intel_cdclk.c:1175-1176) - allowed on Kaby Lake
    let e = plan_set_cdclk(&h, &cfg(8_640_000, 432_000), true).unwrap_err();
    assert_eq!(e, ClockError::CdclkSkylakeVco8640 { vco_khz: 8_640_000 });
    assert_eq!(e.name(), "CDCLK-SKL-VCO-8640-WA1183");
    assert!(plan_set_cdclk(&h, &cfg(8_640_000, 432_000), false).is_ok());
    let bad = CdclkConfig { cdclk_khz: 400_000, vco_khz: 8_100_000, voltage_level: 1 };
    assert_eq!(plan_set_cdclk(&h, &bad, false).unwrap_err(), ClockError::CdclkNotAStep { cdclk_khz: 400_000, vco_khz: 8_100_000 });
}

fn sim_with_hw(h: &CdclkHw) -> Sim {
    let mut s = Sim::new();
    s.set(CDCLK_CTL, h.cdclk_ctl);
    if h.vco_khz != 0 {
        s.set(LCPLL1_CTL, LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK);
        s.set(DPLL_CTRL1, 1 | ((if h.vco_khz == 8_640_000 { 4 } else { 2 }) << 1));
    }
    s
}

#[test]
fn simulated_vco_change_ends_where_linux_ends() {
    let h = hw(8_100_000, 450_000);
    let mut s = sim_with_hw(&h);
    let t = plan_set_cdclk(&h, &cfg(8_640_000, 432_000), false).unwrap();
    let (r, _) = run_tx(&mut s, &t);
    r.unwrap();
    assert_eq!(s.get(CDCLK_CTL), 0x35e); // select 450_432, decimal 862, override cleared
    assert_eq!(s.get(LCPLL1_CTL) & (LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK), LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK);
    assert_eq!(s.get(DPLL_CTRL1) & 0x3f, 0x9); // override + 1080 link rate
    assert_eq!(s.get(GEN6_PCODE_MAILBOX) & GEN6_PCODE_READY, 0);
    assert_eq!(s.writes_to(GEN6_PCODE_DATA).last(), Some(&1)); // the last pcode write carried voltage level 1
    // the decode that the readout crate runs on those registers agrees: 432 MHz on 8640
    let st = decode_cdclk(s.get(LCPLL1_CTL), s.get(DPLL_CTRL1), s.get(CDCLK_CTL));
    assert_eq!((st.vco_khz, st.cdclk_khz), (8_640_000, 432_000));
}

#[test]
fn simulated_same_vco_change_leaves_dpll0_alone() {
    let h = hw(8_100_000, 337_500);
    let mut s = sim_with_hw(&h);
    let t = plan_set_cdclk(&h, &cfg(8_100_000, 540_000), false).unwrap();
    run_tx(&mut s, &t).0.unwrap();
    assert_eq!(s.get(CDCLK_CTL), 0x0400_0436);
    assert!(s.writes_to(LCPLL1_CTL).is_empty());
    assert!(s.writes_to(DPLL_CTRL1).is_empty());
    let st = decode_cdclk(s.get(LCPLL1_CTL), s.get(DPLL_CTRL1), s.get(CDCLK_CTL));
    assert_eq!(st.cdclk_khz, 540_000);
}

#[test]
fn a_dpll0_that_never_locks_fails_by_name_and_rolls_back() {
    let h = hw(8_100_000, 450_000);
    let mut s = sim_with_hw(&h);
    s.pll_never_locks = true;
    let t = plan_set_cdclk(&h, &cfg(8_640_000, 432_000), false).unwrap();
    // the forward plan dies at the unlock wait of the disable? No: unlock succeeds (lock bit clears), the ENABLE lock never comes.
    let r = run_plan(&mut s, &t.forward).unwrap_err();
    match r {
        ClockError::HandshakeTimeout { what, bound, reg, mask, want, got, .. } => {
            assert_eq!(what, Handshake::Dpll0Lock);
            assert_eq!(what.name(), "DPLL0-LOCK");
            assert_eq!(bound, Bound::Time);
            assert_eq!((reg.offset(), mask, want, got), (LCPLL1_CTL, LCPLL_PLL_LOCK, LCPLL_PLL_LOCK, 0));
        }
        e => panic!("{}", e.name()),
    }
    assert_eq!(r.name(), "HANDSHAKE-TIMEOUT");
    // run the rollback: it forces PLL off/on toward 8100/450. With a PLL that locks again it ends at the original state.
    s.pll_never_locks = false;
    run_plan(&mut s, &t.rollback).unwrap();
    let st = decode_cdclk(s.get(LCPLL1_CTL), s.get(DPLL_CTRL1), s.get(CDCLK_CTL));
    assert_eq!((st.vco_khz, st.cdclk_khz), (8_100_000, 450_000));
    assert_eq!(s.get(CDCLK_CTL), h.cdclk_ctl);
}

#[test]
fn rollback_from_a_pll_off_snapshot_turns_dpll0_off() {
    let h = CdclkHw { vco_khz: 0, cdclk_khz: BYPASS_KHZ, voltage_level: 0, cdclk_ctl: 0x0800_0000 };
    let t = plan_set_cdclk(&h, &cfg(8_100_000, 337_500), false).unwrap();
    let mut s = sim_with_hw(&h);
    run_plan(&mut s, &t.forward).unwrap();
    assert_ne!(s.get(LCPLL1_CTL) & LCPLL_PLL_ENABLE, 0);
    run_plan(&mut s, &t.rollback).unwrap();
    assert_eq!(s.get(LCPLL1_CTL) & (LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK), 0);
}

#[test]
fn uninit_goes_to_bypass_without_re_enabling_dpll0() {
    // skl_cdclk_uninit_hw (intel_cdclk.c:1301-1310) - Linux re-enables DPLL0 with vco 0 via skl_set_cdclk; this crate does not.
    let h = hw(8_100_000, 450_000);
    let t = plan_uninit(&h).unwrap();
    let mut s = sim_with_hw(&h);
    run_plan(&mut s, &t.forward).unwrap();
    assert_eq!(s.get(LCPLL1_CTL) & (LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK), 0);
    assert_eq!(s.writes_to(GEN6_PCODE_DATA).last(), Some(&0)); // voltage level of the bypass clock
    assert_eq!(decode_cdclk(s.get(LCPLL1_CTL), s.get(DPLL_CTRL1), s.get(CDCLK_CTL)).cdclk_khz, BYPASS_KHZ);
}

#[test]
fn sanitize_decisions_follow_skl_sanitize_cdclk() {
    // skl_sanitize_cdclk (intel_cdclk.c:1230-1270)
    let good_ctl = ctl_for(450_000, 8_100_000);
    let good = decode_cdclk(LCPLL_PLL_ENABLE | LCPLL_PLL_LOCK, 1 | (2 << 1), good_ctl);
    assert_eq!((good.vco_khz, good.cdclk_khz), (8_100_000, 450_000));
    assert!(!needs_sanitize(0x00ab_cdef, &good, good_ctl)); // all well
    assert!(needs_sanitize(0x0000_0000, &good, good_ctl)); // SWF18 low 24 bits zero: the pre-OS never initialised the display
    assert!(needs_sanitize(0xff00_0000, &good, good_ctl)); // ... the top byte does not count
    assert!(!needs_sanitize(0x0000_0001, &good, good_ctl));
    let off = decode_cdclk(0, 0, 0);
    assert!(needs_sanitize(0x1, &off, 0)); // PLL not enabled
    assert!(needs_sanitize(0x1, &good, good_ctl ^ 1)); // decimal programmed wrong
    // Linux does NOT cross-check the select field against the clock: expected = (cdctl & SEL_MASK) | decimal (intel_cdclk.c:1257-1259)
    assert!(!needs_sanitize(0x1, &good, (good_ctl & !CDCLK_FREQ_SEL_MASK) | CDCLK_FREQ_540));
    // the forced state: vco ~0 so the set path takes the "vco changed" arm
    let s = sanitized(&hw(8_100_000, 450_000));
    assert_eq!((s.vco_khz, s.cdclk_khz), (VCO_UNKNOWN, 0));
    let t = plan_set_cdclk(&s, &cfg(8_100_000, 450_000), false).unwrap();
    assert!(t.forward.iter().any(|x| matches!(x, Step::Poll(p) if p.what == Handshake::Dpll0Unlock)));
    assert!(t.forward.iter().any(|x| matches!(x, Step::Poll(p) if p.what == Handshake::Dpll0Lock)));
}

#[test]
fn transaction_then_orders_rollbacks_in_reverse() {
    let mut a = Transaction::new();
    a.forward.write(1, 1).unwrap();
    a.rollback.write(1, 0).unwrap();
    let mut b = Transaction::new();
    b.forward.write(2, 1).unwrap();
    b.rollback.write(2, 0).unwrap();
    a.then(&b).unwrap();
    let off = |p: &i915_display_clock_core::plan::WritePlan| -> Vec<u32> {
        p.iter().map(|s| match s { Step::Write { reg, .. } => reg.offset(), _ => unreachable!() }).collect()
    };
    assert_eq!(off(&a.forward), [1, 2]);
    assert_eq!(off(&a.rollback), [2, 1]);
}
