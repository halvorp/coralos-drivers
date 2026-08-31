// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the paged PHY vocabulary. Expected values are LINUX literals with their lines.

use r8169_core::phy::{
    apply_modify, param_sequence, PagedModify, PhyOp, ADJUST_10M_ALDPS, PARAM_PAGE,
    PARAM_SELECT_REG, PARAM_VALUE_REG,
};

/// phylib's convention, as the 8168g sequence uses it (r8169_phy_config.c:744-:748):
/// `phy_modify(reg, mask, set)` CLEARS mask, then SETS set.
#[test]
fn modify_clears_the_mask_then_sets_the_set() {
    // Clear bit 15, leave everything else.
    assert_eq!(apply_modify(0xffff, 1 << 15, 0), 0x7fff);
    // Set bit 15, clear nothing.
    assert_eq!(apply_modify(0x0000, 0, 1 << 15), 0x8000);
    // Untouched bits survive both ways.
    assert_eq!(apply_modify(0x1234, 0x00f0, 0x0005), 0x1205);
}

/// THE TWO ARGUMENTS ARE NOT INTERCHANGEABLE. The 8168g config uses the SAME register and the SAME
/// bit with opposite meanings, distinguished only by which argument it appears in (:745 vs :747).
/// Swapping them inverts every tuning operation in every sequence at once, silently.
#[test]
fn the_mask_and_set_arguments_have_opposite_effects() {
    let clear_15 = apply_modify(0xffff, 1 << 15, 0);
    let set_15 = apply_modify(0x0000, 0, 1 << 15);
    assert_eq!(clear_15 & (1 << 15), 0, "mask position CLEARS");
    assert_eq!(set_15 & (1 << 15), 1 << 15, "set position SETS");
    assert_ne!(clear_15 & (1 << 15), set_15 & (1 << 15));
    // And when a bit appears in BOTH, set wins — clear happens first.
    assert_eq!(apply_modify(0x0000, 0xffff, 0x0001), 0x0001);
}

/// :42-:51 — page 0x0a43, selector at 0x13, value at 0x14.
#[test]
fn the_indirect_parameter_file_uses_linuxs_page_and_registers() {
    assert_eq!(PARAM_PAGE, 0x0a43);
    assert_eq!(PARAM_SELECT_REG, 0x13);
    assert_eq!(PARAM_VALUE_REG, 0x14);
}

/// THE SELECTOR IS WRITTEN BEFORE THE VALUE IS MODIFIED (:47 before :48). Reversed, the modify
/// lands on whichever parameter happened to be selected last — a different parameter, silently.
/// And the page is selected first and restored last, so the sequence leaves no page behind.
#[test]
fn a_parameter_write_selects_page_then_selector_then_value_then_restores() {
    let ops = param_sequence(0x8012, 0x0000, 0x8000);
    assert_eq!(
        ops,
        [
            PhyOp::SelectPage(0x0a43),
            PhyOp::Write { reg: 0x13, val: 0x8012 },
            PhyOp::Modify { reg: 0x14, mask: 0x0000, set: 0x8000 },
            PhyOp::RestorePage,
        ]
    );
    // Stated as ordering facts too, so a reordering fails for a readable reason.
    let sel = ops.iter().position(|o| matches!(o, PhyOp::Write { reg: 0x13, .. })).unwrap();
    let val = ops.iter().position(|o| matches!(o, PhyOp::Modify { reg: 0x14, .. })).unwrap();
    assert!(sel < val, "the selector must be written before the value is modified");
    assert!(matches!(ops[0], PhyOp::SelectPage(_)), "the page is selected first");
    assert_eq!(ops[ops.len() - 1], PhyOp::RestorePage, "and restored last");
}

/// :730-:736 — four operations, in order, transcribed against the C line by line.
#[test]
fn the_aldps_sequence_matches_linux_line_by_line() {
    assert_eq!(ADJUST_10M_ALDPS.len(), 4, "a dropped step is a length mismatch, not an absence");
    assert_eq!(ADJUST_10M_ALDPS[0], PagedModify { page: 0x0bcc, reg: 0x14, mask: 1 << 8, set: 0 });
    assert_eq!(ADJUST_10M_ALDPS[1],
               PagedModify { page: 0x0a44, reg: 0x11, mask: 0, set: (1 << 7) | (1 << 6) });
    assert_eq!(ADJUST_10M_ALDPS[2],
               PagedModify { page: 0x0a43, reg: 0x8084, mask: 0x6000, set: 0x0000 });
    assert_eq!(ADJUST_10M_ALDPS[3],
               PagedModify { page: 0x0a43, reg: 0x10, mask: 0x0000, set: 0x1003 });
}

/// The first two entries CLEAR and SET respectively, and confusing them would be invisible in a
/// register dump taken afterwards — the value would simply be wrong.
#[test]
fn the_aldps_sequence_clears_one_bit_and_sets_two() {
    let clear = ADJUST_10M_ALDPS[0];
    assert_eq!(clear.mask.count_ones(), 1);
    assert_eq!(clear.set, 0, ":732 clears BIT(8) and sets nothing");
    let set = ADJUST_10M_ALDPS[1];
    assert_eq!(set.mask, 0, ":733 clears nothing");
    assert_eq!(set.set.count_ones(), 2, "and sets BIT(7) | BIT(6)");
    assert_eq!(set.set, 0xc0);
}
