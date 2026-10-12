// SPDX-License-Identifier: GPL-2.0-only
//! Gen9 topology and timestamp-frequency decode, by hand-worked vectors from gt/intel_sseu.c and gt/intel_gt_clock_utils.c.
use i915_devinfo_core::clock::*;
use i915_devinfo_core::sseu::*;

/// FUSE2 with slice-enable field (27:25) `s` and subslice-disable field (23:20) `d`.
fn fuse2(s: u32, d: u32) -> u32 {
    (s << 25) | (d << 20)
}

#[test]
fn skl_kbl_gt2_one_slice_three_subslices_all_eus() {
    // 1 slice (bit 0), subslice 3 disabled (bit 3 of the disable field = 0b1000), no EUs fused: 3 x 8 = 24 EUs
    let i = gen9_sseu_info(fuse2(0b001, 0b1000), [0, 0, 0]);
    assert_eq!(i.slice_mask, 0b001);
    assert_eq!(i.slice_total(), 1);
    assert_eq!(i.subslice_mask, [0b0111, 0, 0]);
    assert_eq!(i.subslice_total(), 3);
    assert_eq!(i.eu_mask[0], [0xff, 0xff, 0xff, 0]);
    assert_eq!(i.eu_total, 24);
    assert_eq!(i.eu_per_subslice, 8);
    assert!(!i.has_slice_pg, "one slice: no slice power gating");
    assert!(!i.has_subslice_pg, "not Gen9 LP");
    assert!(i.has_eu_pg);
    assert_eq!((i.max_slices, i.max_subslices, i.max_eus_per_subslice), (3, 4, 8));
}

#[test]
fn one_eu_fused_off_for_die_recovery_gives_a_seven_eu_subslice() {
    // EU 0 of subslice 1 disabled: eu_disable bits 8
    let i = gen9_sseu_info(fuse2(0b001, 0b1000), [1 << 8, 0, 0]);
    assert_eq!(i.eu_mask[0], [0xff, 0xfe, 0xff, 0]);
    assert_eq!(i.eu_total, 23);
    assert_eq!(i.subslice_7eu[0], 0b0010);
    assert_eq!(i.eu_per_subslice, 8, "DIV_ROUND_UP(23, 3)");
}

#[test]
fn gt3_three_slices_gets_slice_power_gating_and_per_slice_eu_registers() {
    // slices 0 and 1 enabled; slice 2 disabled even though its EU_DISABLE register holds garbage (not consulted)
    let i = gen9_sseu_info(fuse2(0b011, 0b0000), [0, 0x0f00_0000, 0xffff_ffff]);
    assert_eq!(i.slice_total(), 2);
    assert_eq!(i.subslice_mask, [0xf, 0xf, 0]);
    assert_eq!(i.eu_mask[1][3], 0xf0, "slice 1 subslice 3 had its low four EUs disabled");
    assert_eq!(i.eu_mask[2], [0, 0, 0, 0], "a disabled slice is skipped");
    assert_eq!(i.eu_total, 7 * 8 + 4);
    assert!(i.has_slice_pg);
}

#[test]
fn no_enabled_slice_means_zero_everything() {
    let i = gen9_sseu_info(0, [0xffff_ffff; 3]);
    assert_eq!((i.slice_mask, i.eu_total, i.eu_per_subslice, i.subslice_total()), (0, 0, 0, 0));
    assert!(!i.has_eu_pg);
}

#[test]
fn ctc_crystal_clock_shifts_down_from_24mhz() {
    // freq >>= 3 - shift (intel_gt_clock_utils.c:102); CTC_MODE shift field is bits 2:1, source bit 0 = 0 (crystal)
    assert_eq!(gen9_clock_from_ctc(0b000), Gen9Clock::Hz(3_000_000)); // 24 MHz >> 3
    assert_eq!(gen9_clock_from_ctc(0b010), Gen9Clock::Hz(6_000_000)); // shift 1: >> 2
    assert_eq!(gen9_clock_from_ctc(0b100), Gen9Clock::Hz(12_000_000)); // shift 2: >> 1
    assert_eq!(gen9_clock_from_ctc(0b110), Gen9Clock::Hz(24_000_000)); // shift 3: >> 0
    // upper bits of CTC_MODE are not the source
    assert_eq!(gen9_clock_from_ctc(0xffff_fff0), Gen9Clock::Hz(3_000_000));
    assert_eq!(GEN9_BC_CRYSTAL_HZ, 24_000_000);
}

#[test]
fn ctc_divide_logic_defers_to_the_timestamp_override_register() {
    assert_eq!(gen9_clock_from_ctc(0b001), Gen9Clock::NeedsTimestampOverride);
    assert_eq!(gen9_clock_from_ctc(0b111), Gen9Clock::NeedsTimestampOverride);
}

#[test]
fn reference_ts_freq_is_divider_plus_one_mhz_plus_fraction() {
    // divider 0 -> 1 MHz base; denominator 0 -> 1 MHz / 1 = +1 MHz
    assert_eq!(reference_ts_freq(0), 2_000_000);
    // divider 23 -> 24 MHz base; denominator field 1 -> 1 MHz / 2 = 500 kHz
    assert_eq!(reference_ts_freq(23 | (1 << 12)), 24_500_000);
    // divider 0x3ff -> 1024 MHz; denominator 0xf -> 1 MHz / 16 = 62500
    assert_eq!(reference_ts_freq(0x3ff | (0xf << 12)), 1_024_062_500);
}
