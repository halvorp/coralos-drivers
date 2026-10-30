// SPDX-License-Identifier: GPL-2.0-only
//! The P-state layout against the Pixelbook Go's own `RP_STATE_CAP` (ring 2026-10-30: 0x60615) and Linux's unit arithmetic.
use i915_devinfo_core::lookup;
use i915_devinfo_core::rps::RpsLayout;

/// The Pixelbook Go (KBL GT2 ULX, 8086:591C) as its ring reported it: `[I915-GT-FUSE ... rp_state_cap=0x60615 ...]`.
const ATLAS_RP_STATE_CAP: u32 = 0x60615;

#[test]
fn kaby_lake_decodes_rp0_1050_and_rpn_300_mhz_from_the_atlas_cap() {
    let l = RpsLayout::for_platform(&lookup(0x591c).unwrap()).expect("KBL is GEN9_BC");
    let c = l.caps(ATLAS_RP_STATE_CAP);
    // RP0 = 0x15 * GEN9_FREQ_SCALER, RP1 = RPn = 0x06 * 3 (intel_rps.c:1149-1167)
    assert_eq!((c.rp0, c.rp1, c.rpn), (63, 18, 18));
    assert_eq!(l.mhz(c.rp0), 1050);
    assert_eq!(l.mhz(c.rpn), 300);
    // one unit is 50/3 MHz, rounded to the closest (DIV_ROUND_CLOSEST)
    assert_eq!(l.mhz(1), 17);
    assert_eq!(l.mhz(2), 33);
}

#[test]
fn the_request_and_cagf_fields_are_bits_31_to_23() {
    let l = RpsLayout::for_platform(&lookup(0x5916).unwrap()).unwrap();
    assert_eq!(l.swreq(63), 63 << 23);
    assert_eq!(l.cagf(63 << 23 | 0x7f_ffff), 63, "the low 23 bits are not CAGF");
    assert_eq!(l.cagf(l.swreq(18)), 18);
}

#[test]
fn skylake_takes_the_same_gen9_bc_layout() {
    assert_eq!(RpsLayout::for_platform(&lookup(0x1916).unwrap()), RpsLayout::for_platform(&lookup(0x591c).unwrap()));
}

#[test]
fn cherryview_has_no_layout_its_punit_path_is_not_ported() {
    // the NEGATIVE CONTROL: a Gen8 part is never handed the Gen9 request field
    assert_eq!(RpsLayout::for_platform(&lookup(0x22b0).unwrap()), None);
}
