// SPDX-License-Identifier: GPL-2.0-only
//! DPLL0..3: DP link-rate words, the enable / disable plans (exact), lock handshakes by name, DPLL0's VCO coupling to CDCLK, DDI clock
//! routing per port, and the shared-PLL allocator.
mod common;
use common::*;
use i915_common_core::Reg;
use i915_display_clock_core::dpll::*;
use i915_display_clock_core::error::{Bound, Handshake};
use i915_display_clock_core::plan::{OnTimeout, PollPlan, Step};
use i915_display_clock_core::regs::*;
use i915_display_clock_core::wrpll::{hdmi_hw_state, SklDpllHwState};
use i915_display_clock_core::ClockError;
use i915_kbl_display_core::dpll::{decode_ctrl2, dpll_cfgcr1, dpll_cfgcr2, lcpll_link_clock_khz, DdiClock};
use i915_kbl_display_core::types::Ddi;

fn rd(o: u32) -> Reg {
    Reg::new(o)
}

#[test]
fn dp_link_rate_words_every_rate() {
    // skl_ddi_dp_set_dpll_hw_state (intel_dpll_mgr.c:1864-1900): ctrl1 = OVERRIDE(0) (bit 0) | rate << 1, rate by port_clock / 2.
    // (port_clock kHz, DPLL_CTRL1_LINK_RATE_* field value, ctrl1 word) - fields from intel_display_regs.h:2563-2568
    let rows = [
        (162_000u32, 2u32, 0x05u32), // 1.62 Gbit/s -> 810
        (270_000, 1, 0x03),          // 2.70 -> 1350
        (540_000, 0, 0x01),          // 5.40 -> 2700
        (324_000, 3, 0x07),          // eDP 1.4 3.24 -> 1620
        (216_000, 4, 0x09),          // eDP 1.4 2.16 -> 1080
        (432_000, 5, 0x0b),          // eDP 1.4 4.32 -> 2160
    ];
    for (clk, field, word) in rows {
        let hw = dp_hw_state(clk).unwrap();
        assert_eq!(hw.ctrl1, word, "{clk}");
        assert_eq!(hw.ctrl1, 1 | (field << 1), "{clk}");
        assert_eq!((hw.cfgcr1, hw.cfgcr2), (0, 0));
        // the readout's lcpll decode of the same field names the same clock (skl_ddi_lcpll_get_freq: link clock * 2)
        assert_eq!(lcpll_link_clock_khz(field).unwrap() * 2, clk, "{clk} via the readout decode");
    }
}

#[test]
fn unsupported_dp_rates_are_refused_where_linux_would_silently_pick_2700() {
    for clk in [0u32, 1, 100_000, 243_000, 297_000, 648_000, 810_000, u32::MAX] {
        let e = dp_hw_state(clk).unwrap_err();
        assert_eq!(e, ClockError::DpRateUnsupported { port_clock_khz: clk }, "{clk}");
        assert_eq!(e.name(), "DP-RATE-UNSUPPORTED");
    }
}

#[test]
fn link_rate_vco_families() {
    // 810, 1350, 1620, 2700 -> 8100; 1080, 2160 -> 8640 (intel_cdclk.c:990-998)
    for f in [DPLL_CTRL1_LINK_RATE_810, DPLL_CTRL1_LINK_RATE_1350, DPLL_CTRL1_LINK_RATE_1620, DPLL_CTRL1_LINK_RATE_2700] {
        assert_eq!(link_rate_vco_khz(f), 8_100_000, "field {f}");
    }
    for f in [DPLL_CTRL1_LINK_RATE_1080, DPLL_CTRL1_LINK_RATE_2160] {
        assert_eq!(link_rate_vco_khz(f), 8_640_000, "field {f}");
    }
    assert_eq!(DPLL_CTRL1_LINK_RATE_2700, 0);
    assert_eq!(DPLL_CTRL1_LINK_RATE_1080, 4);
}

#[test]
fn dpll0_follows_cdclks_vco_or_is_refused() {
    let snap = DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: LCPLL_PLL_ENABLE };
    // 2.7 GHz on 8100: fine; on 8640: refused
    let hbr = dp_hw_state(270_000).unwrap();
    assert!(plan_dpll0_edp(&hbr, 8_100_000, &snap).is_ok());
    let e = plan_dpll0_edp(&hbr, 8_640_000, &snap).unwrap_err();
    assert_eq!(e, ClockError::Dpll0VcoMismatch { needs_khz: 8_100_000, running_khz: 8_640_000 });
    assert_eq!(e.name(), "DPLL0-VCO-MISMATCH");
    // 4.32 GHz needs 8640
    let hi = dp_hw_state(432_000).unwrap();
    assert!(plan_dpll0_edp(&hi, 8_640_000, &snap).is_ok());
    assert_eq!(plan_dpll0_edp(&hi, 8_100_000, &snap).unwrap_err(), ClockError::Dpll0VcoMismatch { needs_khz: 8_640_000, running_khz: 8_100_000 });
    // every rate agrees with the cdclk side's choice of VCO
    for (clk, vco) in [(162_000u32, 8_100_000u32), (270_000, 8_100_000), (540_000, 8_100_000), (324_000, 8_100_000), (216_000, 8_640_000), (432_000, 8_640_000)] {
        assert_eq!(i915_display_clock_core::cdclk::dpll0_vco(0, 8_100_000, Some(clk)), vco, "{clk}");
        assert!(plan_dpll0_edp(&dp_hw_state(clk).unwrap(), vco, &snap).is_ok(), "{clk} on {vco}");
    }
}

#[test]
fn dpll0_enable_writes_only_ctrl1() {
    // skl_ddi_dpll0_enable (intel_dpll_mgr.c:1418-1425): the PLL itself is CDCLK's.
    let snap = DpllSnapshot { ctrl1: 0x5, cfgcr1: 0, cfgcr2: 0, ctl: LCPLL_PLL_ENABLE };
    let t = plan_enable(0, &dp_hw_state(270_000).unwrap(), &snap).unwrap();
    assert_eq!(t.forward.len(), 2);
    assert_eq!(t.forward.get(0), Some(&Step::Rmw { reg: rd(DPLL_CTRL1), clear: 0x3e, set: 0x3 }));
    assert_eq!(t.forward.get(1), Some(&Step::PostingRead { reg: rd(DPLL_CTRL1) }));
    // rollback restores the override bit and the 810 rate it had
    assert_eq!(t.rollback.get(0), Some(&Step::Rmw { reg: rd(DPLL_CTRL1), clear: 0x3f, set: 0x5 }));
    // disable is a no-op for DPLL0 (skl_ddi_dpll0_disable)
    assert!(plan_disable(0, &dp_hw_state(270_000).unwrap(), &snap).unwrap().forward.is_empty());
}

#[test]
fn dpll_1_2_3_register_map_and_enable_sequence_is_exact() {
    // skl_dpll_regs (intel_dpll_mgr.c:1356-1379): (id, ctl, cfgcr1, cfgcr2, DPLL_STATUS lock bit, CTRL1 shift)
    let rows = [
        (1u8, 0x46014u32, 0x6c040u32, 0x6c044u32, 1u32 << 8, 6u32),
        (2, 0x46040, 0x6c048, 0x6c04c, 1 << 16, 12),
        (3, 0x46060, 0x6c050, 0x6c054, 1 << 24, 18),
    ];
    let (hw, _) = hdmi_hw_state(148_500).unwrap();
    assert_eq!((hw.ctrl1, hw.cfgcr1, hw.cfgcr2), (0x21, 0x8040_0173, 0x3a5));
    for (id, ctl, c1, c2, lock, sh) in rows {
        let snap = DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: 0 };
        let t = plan_enable(id, &hw, &snap).unwrap();
        let want = [
            Step::Rmw { reg: rd(DPLL_CTRL1), clear: 0x3e << sh, set: 0x21 << sh }, // skl_ddi_pll_write_ctrl1 :1388-1392
            Step::PostingRead { reg: rd(DPLL_CTRL1) },                               // :1393
            Step::Write { reg: rd(c1), value: 0x8040_0173 },                         // :1406
            Step::Write { reg: rd(c2), value: 0x3a5 },                               // :1407
            Step::PostingRead { reg: rd(c1) },                                       // :1408
            Step::PostingRead { reg: rd(c2) },                                       // :1409
            Step::Rmw { reg: rd(ctl), clear: 0, set: 0x8000_0000 },                  // :1412
            Step::Poll(PollPlan::wait_set(Handshake::DpllLock(id), rd(DPLL_STATUS), lock, 5, OnTimeout::Fail)), // :1415
        ];
        assert_eq!(t.forward.len(), want.len(), "dpll {id}");
        for (i, w) in want.iter().enumerate() {
            assert_eq!(t.forward.get(i), Some(w), "dpll {id} step {i}");
        }
    }
}

#[test]
fn simulated_enable_programs_and_locks() {
    let (hw, _) = hdmi_hw_state(148_500).unwrap();
    for (id, ctl, c1, c2) in [(1u8, 0x46014u32, 0x6c040u32, 0x6c044u32), (2, 0x46040, 0x6c048, 0x6c04c), (3, 0x46060, 0x6c050, 0x6c054)] {
        let mut s = Sim::new();
        let snap = DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: 0 };
        run_tx(&mut s, &plan_enable(id, &hw, &snap).unwrap()).0.unwrap();
        assert_eq!(s.get(c1), 0x8040_0173);
        assert_eq!(s.get(c2), 0x3a5);
        assert_eq!(s.get(DPLL_CTRL1), 0x21 << (id as u32 * 6));
        assert_ne!(s.get(ctl) & LCPLL_PLL_ENABLE, 0);
        assert_ne!(s.get(DPLL_STATUS) & (1 << (id as u32 * 8)), 0);
        // the readout crate decodes the result as the HDMI mode PLL for 148.5 MHz
        let st = i915_kbl_display_core::dpll::decode_dpll(id, s.get(ctl), s.get(DPLL_STATUS), s.get(DPLL_CTRL1), s.get(c1), s.get(c2)).unwrap();
        assert!(st.enabled && st.locked && st.hdmi_mode);
        assert_eq!(st.port_clock_khz, 148_500);
    }
}

#[test]
fn every_dpll_lock_times_out_by_name_and_rolls_back() {
    let (hw, _) = hdmi_hw_state(148_500).unwrap();
    for (id, name, ctl, c1, c2) in [
        (1u8, "DPLL1-LOCK", 0x46014u32, 0x6c040u32, 0x6c044u32),
        (2, "DPLL2-LOCK", 0x46040, 0x6c048, 0x6c04c),
        (3, "DPLL3-LOCK", 0x46060, 0x6c050, 0x6c054),
    ] {
        let mut s = Sim::new();
        s.pll_never_locks = true;
        // firmware state to restore: a different, disabled configuration
        s.set(c1, 0x1234);
        s.set(c2, 0x5678);
        s.set(DPLL_CTRL1, 0x1 << (id as u32 * 6));
        let snap = DpllSnapshot { ctrl1: s.get(DPLL_CTRL1), cfgcr1: 0x1234, cfgcr2: 0x5678, ctl: 0 };
        let t = plan_enable(id, &hw, &snap).unwrap();
        let (r, rb) = run_tx(&mut s, &t);
        match r.unwrap_err() {
            ClockError::HandshakeTimeout { what, bound, reg, mask, want, got, .. } => {
                assert_eq!(what, Handshake::DpllLock(id));
                assert_eq!(what.name(), name);
                assert_eq!(bound, Bound::Time);
                assert_eq!((reg.offset(), mask, want, got), (DPLL_STATUS, 1 << (id as u32 * 8), 1 << (id as u32 * 8), 0));
            }
            e => panic!("{name}: {}", e.name()),
        }
        rb.unwrap().unwrap();
        assert_eq!(s.get(ctl) & LCPLL_PLL_ENABLE, 0, "{name}: PLL stopped");
        assert_eq!(s.get(c1), 0x1234, "{name}");
        assert_eq!(s.get(c2), 0x5678, "{name}");
        assert_eq!(s.get(DPLL_CTRL1), snap.ctrl1, "{name}: CTRL1 restored");
    }
}

#[test]
fn dpll_lock_sample_bound_is_enforced() {
    let (hw, _) = hdmi_hw_state(148_500).unwrap();
    let mut s = Sim::new();
    s.pll_never_locks = true;
    s.us_per_sample = 0;
    let t = plan_enable(2, &hw, &DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: 0 }).unwrap();
    match run_plan(&mut s, &t.forward).unwrap_err() {
        ClockError::HandshakeTimeout { bound, polls, .. } => assert_eq!((bound, polls), (Bound::Polls, 100_000)),
        e => panic!("{}", e.name()),
    }
}

#[test]
fn disable_is_rmw_and_posting_read_and_rolls_back_to_enabled() {
    let (hw, _) = hdmi_hw_state(148_500).unwrap();
    let snap = DpllSnapshot { ctrl1: 0x21 << 12, cfgcr1: hw.cfgcr1, cfgcr2: hw.cfgcr2, ctl: LCPLL_PLL_ENABLE };
    let t = plan_disable(2, &hw, &snap).unwrap();
    assert_eq!(t.forward.len(), 2);
    assert_eq!(t.forward.get(0), Some(&Step::Rmw { reg: rd(0x46040), clear: LCPLL_PLL_ENABLE, set: 0 })); // :1434
    assert_eq!(t.forward.get(1), Some(&Step::PostingRead { reg: rd(0x46040) })); // :1435
    // it was enabled, so the rollback is the enable sequence
    assert_eq!(t.rollback.len(), 8);
    // the enable plan's own rollback reads back every restore write (sentinel 2026-10-12)
    let en = plan_enable(2, &hw, &snap).unwrap();
    let posted: Vec<_> = en.rollback.iter().filter(|s| matches!(s, Step::PostingRead { .. })).collect();
    assert!(posted.contains(&&Step::PostingRead { reg: rd(dpll_cfgcr1(2)) }) && posted.contains(&&Step::PostingRead { reg: rd(dpll_cfgcr2(2)) }));
    // it was NOT enabled: nothing to restore
    let off = DpllSnapshot { ctl: 0, ..snap };
    assert!(plan_disable(2, &hw, &off).unwrap().rollback.is_empty());
}

#[test]
fn no_such_dpll() {
    let hw = SklDpllHwState { ctrl1: 0, cfgcr1: 0, cfgcr2: 0 };
    let snap = DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: 0 };
    for id in [4u8, 5, 255] {
        assert_eq!(plan_enable(id, &hw, &snap).unwrap_err(), ClockError::NoSuchDpll { id });
        assert_eq!(plan_disable(id, &hw, &snap).unwrap_err(), ClockError::NoSuchDpll { id });
        assert_eq!(plan_ddi_clock_enable(Ddi::A, id, 0).unwrap_err(), ClockError::NoSuchDpll { id });
    }
    assert_eq!(ClockError::NoSuchDpll { id: 4 }.name(), "NO-SUCH-DPLL");
}

#[test]
fn ddi_clock_routing_every_port_every_pll() {
    // DPLL_CTRL2 (intel_display_regs.h:2571-2576): per port p: CLK_OFF = 1 << (p + 15), SEL = 3 << (3p + 1), OVERRIDE = 1 << 3p.
    // (port, clk_off, sel_mask, sel_shift, override) written out.
    let ports = [
        (Ddi::A, 0x0000_8000u32, 0x0000_0006u32, 1u32, 0x0000_0001u32),
        (Ddi::B, 0x0001_0000, 0x0000_0030, 4, 0x0000_0008),
        (Ddi::C, 0x0002_0000, 0x0000_0180, 7, 0x0000_0040),
        (Ddi::D, 0x0004_0000, 0x0000_0c00, 10, 0x0000_0200),
        (Ddi::E, 0x0008_0000, 0x0000_6000, 13, 0x0000_1000),
    ];
    for (port, off, sel_mask, shift, ovr) in ports {
        for pll in 0..4u8 {
            // skl_ddi_enable_clock (intel_ddi.c:1964-1968): clear CLK_OFF | SEL_MASK, set SEL(pll) | OVERRIDE
            let t = plan_ddi_clock_enable(port, pll, off).unwrap();
            assert_eq!(t.forward.len(), 2);
            assert_eq!(t.forward.get(0), Some(&Step::Rmw { reg: rd(DPLL_CTRL2), clear: off | sel_mask, set: ((pll as u32) << shift) | ovr }), "{port:?} -> {pll}");
            assert_eq!(t.forward.get(1), Some(&Step::PostingRead { reg: rd(DPLL_CTRL2) }), "sentinel 2026-10-12: CTRL2 write is read back");
            assert_eq!(t.rollback.get(1), Some(&Step::PostingRead { reg: rd(DPLL_CTRL2) }));
            // run it on a register that had the clock gated and another PLL selected
            let mut s = Sim::new();
            s.set(DPLL_CTRL2, off | sel_mask | 0x4000_0000);
            run_tx(&mut s, &t).0.unwrap();
            assert_eq!(decode_ctrl2(port, s.get(DPLL_CTRL2)), DdiClock { select_override: true, clk_off: false, pll });
            assert_eq!(s.get(DPLL_CTRL2) & 0x4000_0000, 0x4000_0000, "other bits untouched");
        }
        // skl_ddi_disable_clock (intel_ddi.c:1980-1981): set CLK_OFF
        let t = plan_ddi_clock_disable(port, 0).unwrap();
        assert_eq!(t.forward.get(0), Some(&Step::Rmw { reg: rd(DPLL_CTRL2), clear: 0, set: off }));
        assert_eq!(t.forward.get(1), Some(&Step::PostingRead { reg: rd(DPLL_CTRL2) }));
        assert_eq!(t.rollback.get(1), Some(&Step::PostingRead { reg: rd(DPLL_CTRL2) }));
    }
}

#[test]
fn ddi_clock_rollback_restores_the_snapshot_bits() {
    let snap = 0x0000_0008 | (2 << 4) | 0x0001_0000; // port B: override, PLL 2, clock off
    let t = plan_ddi_clock_enable(Ddi::B, 3, snap).unwrap();
    let mut s = Sim::new();
    s.set(DPLL_CTRL2, snap | 0x100);
    run_plan(&mut s, &t.forward).unwrap();
    run_plan(&mut s, &t.rollback).unwrap();
    assert_eq!(s.get(DPLL_CTRL2), snap | 0x100);
    let t = plan_ddi_clock_disable(Ddi::B, 0).unwrap();
    let mut s = Sim::new();
    run_plan(&mut s, &t.forward).unwrap();
    assert_eq!(s.get(DPLL_CTRL2), 0x0001_0000);
    run_plan(&mut s, &t.rollback).unwrap();
    assert_eq!(s.get(DPLL_CTRL2), 0);
}

fn st(n: u32) -> SklDpllHwState {
    SklDpllHwState { ctrl1: 0x21, cfgcr1: n, cfgcr2: n }
}

#[test]
fn allocator_edp_gets_only_dpll0() {
    let mut a = DpllAllocator::new();
    assert_eq!(OutputKind::Edp.allowed_mask(), 0b0001);
    assert_eq!(OutputKind::Other.allowed_mask(), 0b1110);
    assert_eq!(a.allocate(OutputKind::Edp, &st(1)), Ok(0));
    // a second eDP user with a different state: DPLL0 is taken, no other PLL is allowed
    let e = a.allocate(OutputKind::Edp, &st(2)).unwrap_err();
    assert_eq!(e, ClockError::NoFreeDpll { allowed_mask: 1 });
    assert_eq!(e.name(), "NO-FREE-DPLL");
    // the same state shares it
    assert_eq!(a.allocate(OutputKind::Edp, &st(1)), Ok(0));
    assert_eq!(a.users(0), 2);
}

#[test]
fn allocator_others_take_dpll1_then_2_then_3_and_share_identical_states() {
    let mut a = DpllAllocator::new();
    assert_eq!(a.allocate(OutputKind::Other, &st(1)), Ok(1)); // first unused, ascending (intel_dpll_mgr.c:392-404)
    assert_eq!(a.allocate(OutputKind::Other, &st(2)), Ok(2));
    assert_eq!(a.allocate(OutputKind::Other, &st(1)), Ok(1)); // identical hw_state shares (:406-416)
    assert_eq!(a.allocate(OutputKind::Other, &st(3)), Ok(3));
    assert_eq!(a.allocate(OutputKind::Other, &st(4)), Err(ClockError::NoFreeDpll { allowed_mask: 0b1110 }));
    assert_eq!(a.allocate(OutputKind::Other, &st(3)), Ok(3));
    assert_eq!((a.users(0), a.users(1), a.users(2), a.users(3)), (0, 2, 1, 2));
    // DPLL0 is never handed to a non-eDP output
    assert_eq!(a.users(0), 0);
}

#[test]
fn allocator_release_and_reuse() {
    let mut a = DpllAllocator::new();
    a.allocate(OutputKind::Other, &st(1)).unwrap();
    a.allocate(OutputKind::Other, &st(1)).unwrap();
    assert_eq!(a.release(1), Ok(false));
    assert_eq!(a.release(1), Ok(true)); // last user gone: the PLL may be disabled
    let e = a.release(1).unwrap_err();
    assert_eq!(e, ClockError::UnbalancedPut { what: "dpll" });
    // freed: a different state takes DPLL1 again
    assert_eq!(a.allocate(OutputKind::Other, &st(9)), Ok(1));
    assert_eq!(a.release(7).unwrap_err(), ClockError::NoSuchDpll { id: 7 });
    assert_eq!(a.users(9), 0);
}

#[test]
fn allocator_adopts_what_firmware_left_running() {
    let mut a = DpllAllocator::new();
    a.adopt(2, st(5)).unwrap();
    assert_eq!(a.allocate(OutputKind::Other, &st(5)), Ok(2)); // shares the adopted PLL
    assert_eq!(a.allocate(OutputKind::Other, &st(6)), Ok(1)); // first unused
    assert_eq!(a.adopt(4, st(0)).unwrap_err(), ClockError::NoSuchDpll { id: 4 });
}

#[test]
fn compute_dpll_is_skl_compute_dpll() {
    // skl_compute_dpll (intel_dpll_mgr.c:1937-1950): HDMI -> WRPLL words, DP -> link-rate word
    let (hw, pc) = compute_dpll(Link::Hdmi { clock_khz: 148_500 }).unwrap();
    assert_eq!((hw.ctrl1, hw.cfgcr1, hw.cfgcr2, pc), (0x21, 0x8040_0173, 0x3a5, 148_500));
    let (hw, pc) = compute_dpll(Link::Dp { port_clock_khz: 270_000 }).unwrap();
    assert_eq!((hw.ctrl1, hw.cfgcr1, hw.cfgcr2, pc), (0x03, 0, 0, 270_000));
    assert_eq!(compute_dpll(Link::Dp { port_clock_khz: 123 }).unwrap_err().name(), "DP-RATE-UNSUPPORTED");
    assert_eq!(compute_dpll(Link::Hdmi { clock_khz: 1000 }).unwrap_err().name(), "WRPLL-NO-DIVIDER");
}
