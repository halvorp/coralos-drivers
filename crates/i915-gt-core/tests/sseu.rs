// SPDX-License-Identifier: GPL-2.0-only
//! Gen9 fuse topology through the GT crate: a KBL GT2 sample (24 EU) and a GT1 sample, worked by hand from
//! gt/intel_sseu.c:377-480 (`gen9_sseu_info_init`) with the register fields of intel_gt_regs.h:601-624.
use i915_gt_core::sseu::*;

/// GEN8_FUSE2 with slice-enable (27:25) = s and subslice-disable (23:20) = d.
fn fuse2(s: u32, d: u32) -> u32 {
    (s << 25) | (d << 20)
}

#[test]
fn kbl_gt2_sample_has_24_eus() {
    // GT2: one slice, three subslices of eight EUs. Subslice 3 fused off (disable field 0b1000); no EU disables.
    let i = gen9_sseu_info(fuse2(0b001, 0b1000), [0, 0, 0]);
    assert_eq!(i.slice_mask, 0b001);
    assert_eq!(i.subslice_mask, [0b0111, 0, 0]);
    assert_eq!(i.eu_total, 24);
    assert_eq!(i.eu_per_subslice, 8);
    assert_eq!(i.subslice_total(), 3);
    assert_eq!(i.slice_total(), 1);
    assert_eq!(i.subslice_7eu, [0, 0, 0]);
}

#[test]
fn kbl_gt1_sample_has_12_eus() {
    // GT1: one slice, two subslices (3 and 2 fused off: field 0b1100), six EUs each (EU 6 and 7 disabled: 0xc0 per subslice).
    let eu_dis0 = 0xc0 | (0xc0 << 8);
    let i = gen9_sseu_info(fuse2(0b001, 0b1100), [eu_dis0, 0, 0]);
    assert_eq!(i.subslice_mask, [0b0011, 0, 0]);
    assert_eq!(i.eu_mask[0], [0x3f, 0x3f, 0, 0]);
    assert_eq!(i.eu_total, 12);
    assert_eq!(i.eu_per_subslice, 6);
    assert_eq!(i.subslice_total(), 2);
    assert!(!i.has_slice_pg);
    assert!(i.has_eu_pg, "more than two EUs per subslice");
}

#[test]
fn a_seven_eu_subslice_is_recorded_for_die_recovery() {
    // subslice 1 loses EU 0: seven EUs
    let i = gen9_sseu_info(fuse2(0b001, 0b1000), [0x01 << 8, 0, 0]);
    assert_eq!(i.eu_total, 23);
    assert_eq!(i.subslice_7eu[0], 0b0010);
    assert_eq!(i.eu_per_subslice, 8, "DIV_ROUND_UP(23, 3)");
}

#[test]
fn eu_disable_of_a_disabled_slice_is_ignored() {
    // slice 1 not enabled: its GEN9_EU_DISABLE(1) value must not count (intel_sseu.c:404-407)
    let i = gen9_sseu_info(fuse2(0b001, 0b1000), [0, 0xffff_ffff, 0]);
    assert_eq!(i.eu_total, 24);
}

#[test]
fn steering_picks_slice_zero_subslice_zero_on_a_healthy_gt2() {
    let s = gen9_mcr_steer(&gen9_sseu_info(fuse2(0b001, 0b1000), [0; 3])).unwrap();
    assert_eq!((s.slice, s.subslice, s.value, s.mask), (0, 0, 0, 0x0f00_0000));
}

#[test]
fn steering_skips_a_fused_off_subslice_zero() {
    // subslice 0 disabled (field 0b0001): the first enabled one is subslice 1 -> GEN8_MCR_SUBSLICE(1) = 0x01000000
    let s = gen9_mcr_steer(&gen9_sseu_info(fuse2(0b001, 0b0001), [0; 3])).unwrap();
    assert_eq!((s.slice, s.subslice, s.value), (0, 1, 0x0100_0000));
}

#[test]
fn steering_with_nothing_enabled_is_refused_by_name() {
    assert_eq!(gen9_mcr_steer(&gen9_sseu_info(0, [0; 3])).unwrap_err().name(), "NO-ENABLED-SLICE");
    assert_eq!(gen9_mcr_steer(&gen9_sseu_info(fuse2(0b001, 0b1111), [0; 3])).unwrap_err().name(), "NO-ENABLED-SUBSLICE");
}
