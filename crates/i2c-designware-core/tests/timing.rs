// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the SCL count arithmetic. Expected values are computed from LINUX'S OWN FORMULA with
//! the arithmetic written out, not read back from the implementation.

use i2c_designware_core::timing::{
    counts_for, is_programmable, scl_hcnt, scl_lcnt, DEFAULT_FALL_NS, FAST, FAST_PLUS, STANDARD,
};

/// i2c-designware-master.c:64/:71 (standard), :119/:126 (fast), :96/:103 (fast plus).
#[test]
fn the_per_mode_timing_values_match_linux() {
    assert_eq!((STANDARD.t_high_ns, STANDARD.t_low_ns), (4000, 4700));
    assert_eq!((FAST.t_high_ns, FAST.t_low_ns), (600, 1300));
    assert_eq!((FAST_PLUS.t_high_ns, FAST_PLUS.t_low_ns), (260, 500));
    // master.c:53-54 — `t->sda_fall_ns ?: 300`.
    assert_eq!(DEFAULT_FALL_NS, 300);
}

/// i2c-designware-common.c:547 — `DIV_ROUND_CLOSEST(ic_clk * (tSYMBOL + tf), MICRO) - 3 + offset`.
/// Worked by hand at 10 MHz, fast mode: 10_000_000 * (600 + 300) = 9_000_000_000;
/// 9_000_000_000 / 1_000_000 = 9000 exactly; 9000 - 3 = 8997.
#[test]
fn hcnt_is_linuxs_formula_worked_by_hand() {
    assert_eq!(scl_hcnt(10_000_000, 600, 300, 0), 8997);
}

/// i2c-designware-common.c:567 — the same shape but MINUS ONE.
/// 10_000_000 * (1300 + 300) = 16_000_000_000; / 1e6 = 16000; 16000 - 1 = 15999.
#[test]
fn lcnt_is_linuxs_formula_worked_by_hand() {
    assert_eq!(scl_lcnt(10_000_000, 1300, 300, 0), 15999);
}

/// THE ASYMMETRY IS THE POINT. Given identical inputs the two functions differ by exactly two,
/// because Linux pre-compensates the high count by three ticks and the low count by one. A port
/// that shares one constant between them gets one wrong and the bus still mostly works.
#[test]
fn hcnt_subtracts_three_where_lcnt_subtracts_one() {
    let (clk, t, tf) = (10_000_000, 1000, 300);
    assert_eq!(scl_lcnt(clk, t, tf, 0) - scl_hcnt(clk, t, tf, 0), 2);
}

/// DIV_ROUND_CLOSEST, not truncation. At 3 MHz with t+tf = 833 ns the product is 2_499_000_000,
/// which is 2499.0 exactly — so pick a case that actually has a remainder over half:
/// 3_000_000 * (900 + 300) = 3_600_000_000 -> 3600 exact. Use 1_500_000 * (901 + 300) =
/// 1_801_500_000; / 1e6 = 1801.5, which rounds to 1802 and TRUNCATES to 1801.
#[test]
fn the_division_rounds_to_nearest_rather_than_truncating() {
    // round: (1_801_500_000 + 500_000) / 1_000_000 = 1802 ; then -1 for lcnt = 1801
    assert_eq!(scl_lcnt(1_500_000, 901, 300, 0), 1801);
    // Truncation would have given 1801 - 1 = 1800. Pin the difference explicitly.
    assert_ne!(scl_lcnt(1_500_000, 901, 300, 0), 1800);
}

/// The offset is added AFTER the subtraction, so it shifts the result one-for-one.
#[test]
fn the_offset_shifts_the_result_one_for_one() {
    let base = scl_hcnt(10_000_000, 600, 300, 0);
    assert_eq!(scl_hcnt(10_000_000, 600, 300, 5), base + 5);
    assert_eq!(scl_hcnt(10_000_000, 600, 300, -5), base - 5);
}

/// master.c passes sda_falling_time to hcnt and scl_falling_time to lcnt — two DIFFERENT inputs
/// that happen to share a default. Collapsing them passes every default-configured test and then
/// ignores a platform that specifies only one.
#[test]
fn the_two_fall_times_are_separate_inputs() {
    let (h_same, l_same) = counts_for(FAST, 10_000_000, 300, 300);
    let (h_diff, l_diff) = counts_for(FAST, 10_000_000, 300, 500);
    assert_eq!(h_same, h_diff, "changing the SCL fall time must not move the HIGH count");
    assert_ne!(l_same, l_diff, "changing the SCL fall time must move the LOW count");
    // ...and symmetrically.
    let (h2, l2) = counts_for(FAST, 10_000_000, 500, 300);
    assert_ne!(h_same, h2);
    assert_eq!(l_same, l2);
}

/// `ic_clk == 0` means "read the register instead" (common.c:530, :553) and must never reach the
/// arithmetic: the formula would return a negative count, and a cast would turn -3 into 65533.
#[test]
fn a_nonsense_count_is_rejected_rather_than_cast() {
    assert_eq!(scl_hcnt(0, 600, 300, 0), -3, "the formula itself goes negative");
    assert!(!is_programmable(scl_hcnt(0, 600, 300, 0)));
    assert!(!is_programmable(0));
    assert!(!is_programmable(-1));
    // A count wider than the register is equally unprogrammable — at 100 MHz, standard mode needs
    // ~430000 ticks, far past a 16-bit field. That is a real configuration, not a contrived one.
    let (h, _l) = counts_for(STANDARD, 100_000_000, 300, 300);
    assert!(h > u16::MAX as i64, "expected an over-wide count, got {h}");
    assert!(!is_programmable(h));
    // And a sane one is accepted.
    assert!(is_programmable(scl_hcnt(1_000_000, 600, 300, 0)));
}
