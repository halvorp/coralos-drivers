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

// ─── rtl8168g_1_hw_phy_config, driven through a scripted PHY ───

use r8169_core::phy::{rtl8168g_1_hw_phy_config, Phy, SWR_EFFICIENCY};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Op {
    Read { page: u16, reg: u16, got: u16 },
    Modify { page: u16, reg: u16, mask: u16, set: u16 },
    Write { reg: u16, val: u16 },
}

/// A scripted PHY. Reads are answered ONLY from the script; anything unscripted PANICS, because a
/// zero would silently steer a branch nobody chose and the test would pass against an answer nobody
/// wrote.
struct ScriptedPhy {
    reads: Vec<((u16, u16), u16)>,
    log: Vec<Op>,
}

impl ScriptedPhy {
    fn new(reads: &[((u16, u16), u16)]) -> Self {
        ScriptedPhy { reads: reads.to_vec(), log: Vec::new() }
    }
    fn modifies_of(&self, page: u16, reg: u16) -> Vec<(u16, u16)> {
        self.log.iter().filter_map(|o| match o {
            Op::Modify { page: p, reg: r, mask, set } if *p == page && *r == reg => Some((*mask, *set)),
            _ => None,
        }).collect()
    }
    fn writes(&self) -> Vec<(u16, u16)> {
        self.log.iter().filter_map(|o| match o {
            Op::Write { reg, val } => Some((*reg, *val)),
            _ => None,
        }).collect()
    }
}

impl Phy for ScriptedPhy {
    fn read_paged(&mut self, page: u16, reg: u16) -> u16 {
        let got = self.reads.iter().find(|((p, r), _)| *p == page && *r == reg)
            .unwrap_or_else(|| panic!("unscripted read of page {page:#06x} reg {reg:#04x}")).1;
        self.log.push(Op::Read { page, reg, got });
        got
    }
    fn modify_paged(&mut self, page: u16, reg: u16, mask: u16, set: u16) {
        self.log.push(Op::Modify { page, reg, mask, set });
    }
    fn write(&mut self, reg: u16, val: u16) {
        self.log.push(Op::Write { reg, val });
    }
}

/// Both conditional reads are scripted; only their BIT(8) matters.
fn run(first_bit8: bool, second_bit8: bool) -> ScriptedPhy {
    let b = |set| if set { 1u16 << 8 } else { 0 };
    let mut phy = ScriptedPhy::new(&[
        ((0x0a46, 0x10), b(first_bit8)),
        ((0x0a46, 0x13), b(second_bit8)),
    ]);
    rtl8168g_1_hw_phy_config(&mut phy);
    phy
}

/// THE TWO CONDITIONALS ARE INVERSES OF EACH OTHER (:745-:748 vs :751-:754). They read like the
/// same idiom. Copying the first and editing the registers gets the second backwards, and a PHY
/// does not report a wrongly-configured tuning bit.
#[test]
fn the_first_conditional_is_inverted_and_the_second_is_direct() {
    // bit 8 SET -> CLEAR bit 15 (mask position)
    assert_eq!(run(true, false).modifies_of(0x0bcc, 0x12), vec![(1 << 15, 0)]);
    // bit 8 CLEAR -> SET bit 15 (set position)
    assert_eq!(run(false, false).modifies_of(0x0bcc, 0x12), vec![(0, 1 << 15)]);
    // ...and the SECOND goes the other way: bit 8 SET -> SET bit 1
    assert_eq!(run(false, true).modifies_of(0x0c41, 0x15), vec![(0, 1 << 1)]);
    // bit 8 CLEAR -> CLEAR bit 1
    assert_eq!(run(false, false).modifies_of(0x0c41, 0x15), vec![(1 << 1, 0)]);
}

/// Stated as the property itself: for the SAME input bit, the two blocks act in opposite senses.
#[test]
fn the_same_input_bit_drives_the_two_blocks_oppositely() {
    let set = run(true, true);
    let (m1, s1) = set.modifies_of(0x0bcc, 0x12)[0];
    let (m2, s2) = set.modifies_of(0x0c41, 0x15)[0];
    assert!(m1 != 0 && s1 == 0, "block one CLEARS when the bit is set");
    assert!(m2 == 0 && s2 != 0, "block two SETS when the same bit is set");
}

/// :757, :763, :768 — the unconditional modifies, including the one call using BOTH arguments.
#[test]
fn the_unconditional_modifies_match_linux() {
    let phy = run(false, false);
    // 0x0a44:0x11 IS MODIFIED TWICE IN ONE RUN, and both must happen: :757 sets BIT(3)|BIT(2) for
    // auto speed down, and :733 — inside rtl8168g_phy_adjust_10m_aldps — sets BIT(7)|BIT(6) on the
    // SAME register. My first version of this test asserted one and the scripted PHY caught it.
    // Deduplicating them, on the reasonable-looking grounds that a register is configured once,
    // drops half the configuration.
    assert_eq!(
        phy.modifies_of(0x0a44, 0x11),
        vec![(0, (1 << 3) | (1 << 2)), (0, (1 << 7) | (1 << 6))],
        "auto speed down at :757, then the aldps adjustment at :733"
    );
    assert_eq!(phy.modifies_of(0x0a4b, 0x11), vec![(0, 1 << 2)], "EEE auto-fallback");
    assert_eq!(phy.modifies_of(0x0c42, 0x11), vec![(1 << 13, 1 << 14)],
               ":768 is the one call that both clears and sets");
}

/// :772-:781 — ten raw writes, in order, ending on page 0x0000 so nothing is left selected.
#[test]
fn the_swr_block_is_written_verbatim_and_restores_the_page() {
    assert_eq!(SWR_EFFICIENCY.len(), 10);
    let phy = run(false, false);
    let w = phy.writes();
    let tail = &w[w.len() - 10..];
    assert_eq!(tail, SWR_EFFICIENCY.as_slice());
    assert_eq!(tail[tail.len() - 1], (0x1f, 0x0000), "the block leaves no page behind");
}

/// THE REPETITION IS DELIBERATE. Register 0x14 receives 0x1065, then 0x9065, then 0x1065 AGAIN —
/// written, pulsed via bit 15, written back. The trailing duplicate reads like a copy-paste slip;
/// removing it "tidies" the sequence into one that never performs the toggle.
#[test]
fn the_swr_block_pulses_a_register_rather_than_repeating_itself_by_accident() {
    let at_0x14: Vec<u16> = SWR_EFFICIENCY.iter().filter(|(r, _)| *r == 0x14).map(|(_, v)| *v).collect();
    assert_eq!(at_0x14, vec![0x5065, 0xd065, 0x1065, 0x9065, 0x1065]);
    // Each pair differs ONLY in bit 15 — that is what makes it a pulse and not five distinct values.
    assert_eq!(at_0x14[0] ^ at_0x14[1], 0x8000);
    assert_eq!(at_0x14[2] ^ at_0x14[3], 0x8000);
    assert_eq!(at_0x14[2], at_0x14[4], "the third write returns to the pre-pulse value");
}

/// An unscripted read must PANIC rather than answer zero — a zero would steer a branch nobody chose.
#[test]
#[should_panic(expected = "unscripted read")]
fn an_unscripted_read_is_refused_by_name() {
    let mut phy = ScriptedPhy::new(&[((0x0a46, 0x10), 0)]);
    rtl8168g_1_hw_phy_config(&mut phy);
}

use r8169_core::phy::{CONFIG_EEE_PHY, CONFIG_EEE_PHY_8168H_EXTRA, DISABLE_ALDPS};

/// :720-:723 and :88-:91 — one modify each, on the SAME page, in opposite senses.
#[test]
fn the_two_trailing_helpers_match_linux() {
    assert_eq!(DISABLE_ALDPS, PagedModify { page: 0x0a43, reg: 0x10, mask: 1 << 2, set: 0 });
    assert_eq!(CONFIG_EEE_PHY, PagedModify { page: 0x0a43, reg: 0x11, mask: 0, set: 1 << 4 });
    // disable_aldps CLEARS; config_eee SETS. Same page, adjacent registers, opposite argument slots.
    assert!(DISABLE_ALDPS.mask != 0 && DISABLE_ALDPS.set == 0);
    assert!(CONFIG_EEE_PHY.mask == 0 && CONFIG_EEE_PHY.set != 0);
}

/// PAGE 0x0a43 HOSTS BOTH the indirect parameter file (registers 0x13/0x14) AND ordinary direct
/// registers (0x10, 0x11). Treating the whole page as the parameter file — an easy assumption once
/// you have seen `r8168g_phy_param` — turns these writes into parameter selections that go nowhere.
#[test]
fn the_parameter_page_also_carries_direct_registers() {
    assert_eq!(DISABLE_ALDPS.page, PARAM_PAGE);
    assert_eq!(CONFIG_EEE_PHY.page, PARAM_PAGE);
    for r in [DISABLE_ALDPS.reg, CONFIG_EEE_PHY.reg] {
        assert!(r != 0x13 && r != 0x14, "{r:#x} is a direct register, not the selector/value pair");
    }
}

/// :93-:98 — the 8168h EXTENDS the g sequence rather than replacing it. The reference board is a g,
/// so only the g modify is on its path; the extra pair is carried so the relationship is visible.
#[test]
fn the_8168h_variant_extends_rather_than_replaces() {
    assert_eq!(CONFIG_EEE_PHY_8168H_EXTRA.len(), 2);
    assert_eq!(CONFIG_EEE_PHY_8168H_EXTRA[0],
               PagedModify { page: 0x0a4a, reg: 0x11, mask: 0x0000, set: 0x0200 });
    assert_eq!(CONFIG_EEE_PHY_8168H_EXTRA[1],
               PagedModify { page: 0x0a42, reg: 0x14, mask: 0x0000, set: 0x0080 });
    // It is an ADDITION: neither extra touches what the g version does.
    for e in CONFIG_EEE_PHY_8168H_EXTRA {
        assert_ne!((e.page, e.reg), (CONFIG_EEE_PHY.page, CONFIG_EEE_PHY.reg));
    }
}

/// The config function now runs both helpers, LAST and in Linux's order (:782 then :783).
#[test]
fn the_config_runs_both_helpers_at_the_end_in_order() {
    let phy = run(false, false);
    let mods: Vec<(u16, u16, u16, u16)> = phy.log.iter().filter_map(|o| match o {
        Op::Modify { page, reg, mask, set } => Some((*page, *reg, *mask, *set)),
        _ => None,
    }).collect();
    let last_two = &mods[mods.len() - 2..];
    assert_eq!(last_two[0], (0x0a43, 0x10, 1 << 2, 0), "disable_aldps at :782");
    assert_eq!(last_two[1], (0x0a43, 0x11, 0, 1 << 4), "config_eee_phy at :783");
}

/// 0x0a43:0x10 IS WRITTEN TWICE PER RUN — bits 0/1/12 set by the aldps ADJUSTMENT at :735, bit 2
/// cleared by disable_aldps at :782. The same shape that caught a wrong test of mine on the
/// scripted PHY's first outing, so it is asserted here rather than assumed.
#[test]
fn the_aldps_register_is_written_twice_with_non_overlapping_bits() {
    let phy = run(false, false);
    let at: Vec<(u16, u16)> = phy.modifies_of(0x0a43, 0x10);
    assert_eq!(at, vec![(0x0000, 0x1003), (1 << 2, 0)], "the adjustment, then the disable");
    // They do not overlap, so their order does not change the result — but both must happen.
    let (adjust_mask, adjust_set) = at[0];
    let (disable_mask, disable_set) = at[1];
    assert_eq!(adjust_set & disable_mask, 0, "the disable must not clear what the adjustment set");
    assert_eq!(disable_set & adjust_mask, 0);
}
