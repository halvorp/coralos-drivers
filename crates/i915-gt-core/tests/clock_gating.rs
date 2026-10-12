// SPDX-License-Identifier: GPL-2.0-only
//! kbl_init_clock_gating (intel_clock_gating.c:387-409 over gen9_init_clock_gating :49-74). Register offsets and bit values are
//! GCC-evaluated Linux literals; every statement is an unconditional read-modify-write.
use i915_common_core::Reg;
use i915_devinfo_core::Platform;
use i915_gt_core::clock_gating::*;
use i915_gt_core::step::Step;
use i915_gt_core::wa_tables::WaContext;
use i915_gt_core::{Effect, RmwPlan};

fn ctx(step: Step) -> WaContext {
    WaContext { platform: Platform::Kabylake, graphics_ver: 9, has_llc: true, step }
}
fn rmw(reg: u32, set: u32) -> Effect {
    Effect::Rmw(RmwPlan { reg: Reg::new(reg), clear: 0, set, skip_if_unchanged: false })
}

#[test]
fn kbl_at_c0_or_later() {
    let p = plan_kbl_init_clock_gating(&ctx(Step::C0)).unwrap();
    let want = [
        rmw(0x42080, 0x8000),     // CHICKEN_PAR1_1 |= SKL_DE_COMPRESSED_HASH_MODE (HAS_LLC)
        rmw(0x42080, 0x8),        // CHICKEN_PAR1_1 |= SKL_EDP_PSR_FIX_RDWRAP
        rmw(0x46430, 0x2000),     // GEN8_CHICKEN_DCPR_1 |= MASK_WAKEMEM
        rmw(0x45000, 0x8000_0000), // DISP_ARB_CTL |= DISP_FBC_MEMORY_WAKE
        rmw(0x9044, 0x4000_0000), // FBC_LLC_READ_CTRL |= FBC_LLC_FULLY_OPEN
        rmw(0x45000, 0x8000),     // DISP_ARB_CTL |= DISP_FBC_WM_DIS
    ];
    assert_eq!(p.as_slice(), want);
}

#[test]
fn kbl_before_c0_adds_the_sde_unit_and_gam_clock_gating_disables() {
    let p = plan_kbl_init_clock_gating(&ctx(Step::B0)).unwrap();
    let want = [
        rmw(0x42080, 0x8000),
        rmw(0x42080, 0x8),
        rmw(0x46430, 0x2000),
        rmw(0x45000, 0x8000_0000),
        rmw(0x9044, 0x4000_0000),
        rmw(0x9430, 0x4000),    // GEN8_UCGCTL6 |= GEN8_SDEUNIT_CLOCK_GATE_DISABLE
        rmw(0x9400, 0x40_0000), // GEN6_UCGCTL1 |= GEN6_GAMUNIT_CLOCK_GATE_DISABLE
        rmw(0x45000, 0x8000),
    ];
    assert_eq!(p.as_slice(), want);
}

#[test]
fn without_llc_the_compressed_hash_mode_row_is_dropped() {
    let mut c = ctx(Step::C0);
    c.has_llc = false;
    let p = plan_kbl_init_clock_gating(&c).unwrap();
    assert_eq!(p.len(), 5);
    assert_eq!(p.as_slice()[0], rmw(0x42080, 0x8));
}

#[test]
fn every_statement_writes_unconditionally() {
    for e in plan_kbl_init_clock_gating(&ctx(Step::B0)).unwrap().as_slice() {
        let Effect::Rmw(r) = e else { panic!("clock gating is read-modify-write only") };
        assert!(!r.skip_if_unchanged, "intel_uncore_rmw writes back even when unchanged");
        assert_eq!(r.clear, 0);
        assert_eq!(r.result(r.set), Some(r.set), "an already-set register is still written");
    }
}

#[test]
fn row_tables_cite_file_and_line() {
    assert_eq!(GEN9_CLOCK_GATING_ROWS.len(), 4);
    assert_eq!(KBL_CLOCK_GATING_ROWS.len(), 4);
    for r in GEN9_CLOCK_GATING_ROWS.iter().chain(KBL_CLOCK_GATING_ROWS.iter()) {
        assert!(r.src.starts_with("intel_clock_gating.c:"));
        assert!(r.src["intel_clock_gating.c:".len()..].parse::<u32>().is_ok());
        assert!(!r.name.is_empty() && !r.linux_reg.is_empty());
    }
}
