// SPDX-License-Identifier: GPL-2.0-only
//! Gen9 fuse topology through the GT crate: a KBL GT2 sample (24 EU) and a GT1 sample, worked by hand from
//! gt/intel_sseu.c:377-480 (`gen9_sseu_info_init`) with the register fields of intel_gt_regs.h:601-624.
use i915_gt_core::error::GtError;
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

// ---- intel_sseu_make_rpcs (moved from i915-submit-core) ----

fn sseu(slice_pg: bool, subslice_pg: bool, eu_pg: bool) -> SseuInfo {
    SseuInfo {
        slice_mask: 1,
        max_slices: 1,
        max_subslices: 3,
        max_eus_per_subslice: 8,
        subslice_mask: [0xf, 0, 0],
        eu_mask: [[0; 4]; 3],
        subslice_7eu: [0; 3],
        eu_total: 24,
        eu_per_subslice: 8,
        has_slice_pg: slice_pg,
        has_subslice_pg: subslice_pg,
        has_eu_pg: eu_pg,
    }
}

/// ((has_slice_pg, has_subslice_pg, has_eu_pg), (slice_mask, subslice_mask, min_eus, max_eus), rpcs): the real
/// `intel_sseu_make_rpcs` (intel_sseu.c:654-760) for graphics version 9, GCC-evaluated.
const RPCS: [((bool, bool, bool), (u32, u32, u32, u32), u32); 32] = [
((false, false, false), (1, 0x7, 2, 8), 0x00000000),
    ((false, false, false), (7, 0xf, 2, 8), 0x00000000),
    ((false, false, false), (1, 0x1, 8, 8), 0x00000000),
    ((false, false, false), (3, 0x3, 0, 1), 0x00000000),
    ((true, false, false), (1, 0x7, 2, 8), 0x80048000),
    ((true, false, false), (7, 0xf, 2, 8), 0x80058000),
    ((true, false, false), (1, 0x1, 8, 8), 0x80048000),
    ((true, false, false), (3, 0x3, 0, 1), 0x80050000),
    ((false, true, false), (1, 0x7, 2, 8), 0x80000b00),
    ((false, true, false), (7, 0xf, 2, 8), 0x80000c00),
    ((false, true, false), (1, 0x1, 8, 8), 0x80000900),
    ((false, true, false), (3, 0x3, 0, 1), 0x80000a00),
    ((true, true, false), (1, 0x7, 2, 8), 0x80048b00),
    ((true, true, false), (7, 0xf, 2, 8), 0x80058c00),
    ((true, true, false), (1, 0x1, 8, 8), 0x80048900),
    ((true, true, false), (3, 0x3, 0, 1), 0x80050a00),
    ((false, false, true), (1, 0x7, 2, 8), 0x80000082),
    ((false, false, true), (7, 0xf, 2, 8), 0x80000082),
    ((false, false, true), (1, 0x1, 8, 8), 0x80000088),
    ((false, false, true), (3, 0x3, 0, 1), 0x80000010),
    ((true, false, true), (1, 0x7, 2, 8), 0x80048082),
    ((true, false, true), (7, 0xf, 2, 8), 0x80058082),
    ((true, false, true), (1, 0x1, 8, 8), 0x80048088),
    ((true, false, true), (3, 0x3, 0, 1), 0x80050010),
    ((false, true, true), (1, 0x7, 2, 8), 0x80000b82),
    ((false, true, true), (7, 0xf, 2, 8), 0x80000c82),
    ((false, true, true), (1, 0x1, 8, 8), 0x80000988),
    ((false, true, true), (3, 0x3, 0, 1), 0x80000a10),
    ((true, true, true), (1, 0x7, 2, 8), 0x80048b82),
    ((true, true, true), (7, 0xf, 2, 8), 0x80058c82),
    ((true, true, true), (1, 0x1, 8, 8), 0x80048988),
    ((true, true, true), (3, 0x3, 0, 1), 0x80050a10),
];

#[test]
fn rpcs_equals_intel_sseu_make_rpcs() {
    for ((s, ss, eu), (slice, subslice, emin, emax), want) in RPCS {
        let req = SseuRequest { slice_mask: slice as u8, subslice_mask: subslice as u8, min_eus_per_subslice: emin as u8, max_eus_per_subslice: emax as u8 };
        assert_eq!(make_rpcs_gen9(&sseu(s, ss, eu), &req).unwrap(), want, "pg=({s},{ss},{eu}) req=({slice},{subslice:#x},{emin},{emax})");
    }
}

#[test]
fn a_request_wider_than_the_rpcs_field_is_refused_by_name() {
    // 8 slices do not fit the 3-bit S_CNT field (GEM_BUG_ON(val & ~mask), intel_sseu.c:731)
    let req = SseuRequest { slice_mask: 0xff, subslice_mask: 1, min_eus_per_subslice: 1, max_eus_per_subslice: 1 };
    let e = make_rpcs_gen9(&sseu(true, false, false), &req).unwrap_err();
    assert_eq!(e, GtError::RpcsFieldOverflow { field: "slice count", value: 8 });
    assert_eq!(e.name(), "RPCS-FIELD-OVERFLOW");
    // 8 subslices do not fit the 3-bit SS_CNT field
    let req = SseuRequest { slice_mask: 1, subslice_mask: 0xff, min_eus_per_subslice: 1, max_eus_per_subslice: 1 };
    assert_eq!(make_rpcs_gen9(&sseu(false, true, false), &req).unwrap_err().name(), "RPCS-FIELD-OVERFLOW");
    // 16 EUs do not fit the 4-bit field
    let req = SseuRequest { slice_mask: 1, subslice_mask: 1, min_eus_per_subslice: 1, max_eus_per_subslice: 16 };
    assert_eq!(make_rpcs_gen9(&sseu(false, false, true), &req).unwrap_err().name(), "RPCS-FIELD-OVERFLOW");
}

#[test]
fn the_default_request_is_the_whole_device() {
    let s = sseu(true, true, true);
    let r = sseu_from_device_info(&s);
    // intel_sseu_from_device_info sets BOTH eu bounds to max_eus_per_subslice (intel_sseu.h:113-114)
    assert_eq!(r, SseuRequest { slice_mask: 1, subslice_mask: 0xf, min_eus_per_subslice: 8, max_eus_per_subslice: 8 });
}
