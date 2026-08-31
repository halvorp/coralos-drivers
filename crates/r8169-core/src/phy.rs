// SPDX-License-Identifier: GPL-2.0-only
//! Paged PHY access — the vocabulary every r8169 PHY configuration sequence is written in.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_phy_config.c`:
//!   * `r8168g_phy_param` (:42-:51) — the indirect parameter file
//!   * `rtl8168g_phy_adjust_10m_aldps` (:730-:736) — a complete sequence, as data
//! and from phylib's `phy_modify` convention, which those calls are written against.
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! WHY THIS AND NOT THE WHOLE CONFIG FUNCTION. `rtl8168g_1_hw_phy_config` (:738) is a long list of
//! paged operations, some conditional on values read back from the PHY. Every one of them is
//! written in the four primitives below, and every one of them is silent when wrong — a PHY does
//! not report a misapplied tuning parameter, it just performs worse or intermittently. Pinning the
//! VOCABULARY first means the sequences that follow are transcription against a checked base,
//! rather than transcription on top of transcription.

/// One paged PHY operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyOp {
    /// `phy_select_page` — switch to `page`, remembering the old one.
    SelectPage(u16),
    /// `__phy_write(reg, val)`.
    Write { reg: u16, val: u16 },
    /// `__phy_modify(reg, mask, set)` — read, CLEAR the `mask` bits, SET the `set` bits, write.
    Modify { reg: u16, mask: u16, set: u16 },
    /// `phy_restore_page` — return to whatever page was selected before.
    RestorePage,
}

/// phylib's `phy_modify(reg, mask, set)`: clear the mask bits, then set the set bits.
///
/// THE TWO ARGUMENTS ARE NOT INTERCHANGEABLE AND THE ORDER IS THE CONVENTION. In the 8168g
/// sequence, `phy_modify_paged(.., 0x12, BIT(15), 0)` CLEARS bit 15 and
/// `phy_modify_paged(.., 0x12, 0, BIT(15))` SETS it — the same register, the same bit, opposite
/// meanings distinguished only by which argument it appears in. Swapping them inverts every tuning
/// operation in every sequence at once, and a PHY does not complain: it just performs worse.
pub fn apply_modify(current: u16, mask: u16, set: u16) -> u16 {
    (current & !mask) | set
}

/// `r8168g_phy_param` (:42-:51) — write a parameter into the indirect file on page 0x0a43.
///
/// Register 0x13 SELECTS which parameter; register 0x14 is its value. The selector must be written
/// BEFORE the value is modified — reversed, the modify lands on whichever parameter was selected
/// last, which is usually a different one and always silent.
pub const PARAM_PAGE: u16 = 0x0a43;
pub const PARAM_SELECT_REG: u16 = 0x13;
pub const PARAM_VALUE_REG: u16 = 0x14;

/// The four operations `r8168g_phy_param` performs, in order.
pub fn param_sequence(parm: u16, mask: u16, set: u16) -> [PhyOp; 4] {
    [
        PhyOp::SelectPage(PARAM_PAGE),
        PhyOp::Write { reg: PARAM_SELECT_REG, val: parm },
        PhyOp::Modify { reg: PARAM_VALUE_REG, mask, set },
        PhyOp::RestorePage,
    ]
}

/// One entry of a paged sequence: `phy_modify_paged(phydev, page, reg, mask, set)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PagedModify {
    pub page: u16,
    pub reg: u16,
    pub mask: u16,
    pub set: u16,
}

/// `rtl8168g_phy_adjust_10m_aldps` (:730-:736), transcribed as data.
///
/// Four operations, in order, with the third being an indirect parameter write rather than a direct
/// paged modify. Kept as data so the sequence can be compared against the C line by line, and so a
/// dropped step is a length mismatch rather than an absence nobody notices.
pub const ADJUST_10M_ALDPS: [PagedModify; 4] = [
    // :732  phy_modify_paged(phydev, 0x0bcc, 0x14, BIT(8), 0)
    PagedModify { page: 0x0bcc, reg: 0x14, mask: 1 << 8, set: 0 },
    // :733  phy_modify_paged(phydev, 0x0a44, 0x11, 0, BIT(7) | BIT(6))
    PagedModify { page: 0x0a44, reg: 0x11, mask: 0, set: (1 << 7) | (1 << 6) },
    // :734  r8168g_phy_param(phydev, 0x8084, 0x6000, 0x0000) — the INDIRECT file, page 0x0a43,
    // recorded here with its parameter number in `reg` so the sequence stays one list.
    PagedModify { page: PARAM_PAGE, reg: 0x8084, mask: 0x6000, set: 0x0000 },
    // :735  phy_modify_paged(phydev, 0x0a43, 0x10, 0x0000, 0x1003)
    PagedModify { page: 0x0a43, reg: 0x10, mask: 0x0000, set: 0x1003 },
];

/// The page-select register. `phy_write(phydev, 0x1f, page)` switches page directly, and the 8168g
/// sequence uses it that way for its raw block (:772-:781).
pub const PAGE_SELECT_REG: u16 = 0x1f;

/// A PHY the configuration talks to. A real one is MDIO; a test one is scripted.
///
/// The test implementation must PANIC on any page/register nobody scripted — returning zero would
/// let a mis-transcribed read take a branch nobody chose, and the test would pass against an answer
/// nobody wrote.
pub trait Phy {
    /// Read `reg` on `page`, restoring the previous page (phylib's `phy_read_paged`).
    fn read_paged(&mut self, page: u16, reg: u16) -> u16;
    /// Read-modify-write `reg` on `page` (phylib's `phy_modify_paged`).
    fn modify_paged(&mut self, page: u16, reg: u16, mask: u16, set: u16);
    /// A bare `phy_write` against whatever page is currently selected.
    fn write(&mut self, reg: u16, val: u16);
}

/// `rtl8168g_1_hw_phy_config` (r8169_phy_config.c:738-:783) — the configuration for the part on the
/// CoralOS reference board (VER_40 / RTL8168g, XID 0x4c0).
///
/// THE TWO CONDITIONALS HAVE OPPOSITE POLARITY, AND THAT IS THE WHOLE HAZARD.
///   * :745-:748 — bit 8 of 0x0a46:0x10 SET means CLEAR bit 15 of 0x0bcc:0x12; clear means SET it.
///   * :751-:754 — bit 8 of 0x0a46:0x13 SET means SET bit 1 of 0x0c41:0x15; clear means CLEAR it.
/// They read like the same idiom and are inverses of each other. A port that writes one loop for
/// both, or copies the first and edits the registers, gets exactly one of them backwards — and a
/// PHY does not report a wrongly-configured tuning bit, it just performs worse.
///
/// NOT INCLUDED: `r8169_apply_firmware` (:743), `rtl8168g_disable_aldps` and
/// `rtl8168g_config_eee_phy` (:782-:783). The first needs a firmware blob; the other two are their
/// own sequences and belong in their own increment rather than being half-transcribed here.
pub fn rtl8168g_1_hw_phy_config<P: Phy>(phy: &mut P) {
    // :745-:748 — INVERTED: bit set means clear.
    let v = phy.read_paged(0x0a46, 0x10);
    if v & (1 << 8) != 0 {
        phy.modify_paged(0x0bcc, 0x12, 1 << 15, 0);
    } else {
        phy.modify_paged(0x0bcc, 0x12, 0, 1 << 15);
    }

    // :751-:754 — DIRECT: bit set means set. The opposite of the block above.
    let v = phy.read_paged(0x0a46, 0x13);
    if v & (1 << 8) != 0 {
        phy.modify_paged(0x0c41, 0x15, 0, 1 << 1);
    } else {
        phy.modify_paged(0x0c41, 0x15, 1 << 1, 0);
    }

    // :757 Enable PHY auto speed down
    phy.modify_paged(0x0a44, 0x11, 0, (1 << 3) | (1 << 2));

    // :760 rtl8168g_phy_adjust_10m_aldps — the sequence carried as data above.
    for op in ADJUST_10M_ALDPS {
        if op.page == PARAM_PAGE && op.reg > 0xff {
            // The indirect entry: its `reg` is a PARAMETER number, not a register.
            for step in param_sequence(op.reg, op.mask, op.set) {
                match step {
                    PhyOp::SelectPage(p) => phy.write(PAGE_SELECT_REG, p),
                    PhyOp::Write { reg, val } => phy.write(reg, val),
                    PhyOp::Modify { reg, mask, set } => phy.modify_paged(PARAM_PAGE, reg, mask, set),
                    PhyOp::RestorePage => phy.write(PAGE_SELECT_REG, 0x0000),
                }
            }
        } else {
            phy.modify_paged(op.page, op.reg, op.mask, op.set);
        }
    }

    // :763 EEE auto-fallback
    phy.modify_paged(0x0a4b, 0x11, 0, 1 << 2);

    // :766 Enable UC LPF tune — the indirect parameter file.
    for step in param_sequence(0x8012, 0x0000, 0x8000) {
        match step {
            PhyOp::SelectPage(p) => phy.write(PAGE_SELECT_REG, p),
            PhyOp::Write { reg, val } => phy.write(reg, val),
            PhyOp::Modify { reg, mask, set } => phy.modify_paged(PARAM_PAGE, reg, mask, set),
            PhyOp::RestorePage => phy.write(PAGE_SELECT_REG, 0x0000),
        }
    }

    // :768 — clears BIT(13) AND sets BIT(14): the one call in this function using both arguments.
    phy.modify_paged(0x0c42, 0x11, 1 << 13, 1 << 14);

    // :771-:781 "Improve SWR Efficiency" — raw writes with 0x1f as the page register.
    for (reg, val) in SWR_EFFICIENCY {
        phy.write(reg, val);
    }
}

/// The "Improve SWR Efficiency" block (:772-:781), verbatim and in order.
///
/// TEN writes, and the repetition is deliberate. `0x14` receives 0x5065 then 0xd065 — the same value
/// with bit 15 toggled — and later 0x1065, 0x9065, 0x1065: written, pulsed, written AGAIN. The
/// trailing duplicate reads like a copy-paste slip and is a pulse; removing it "tidies" the sequence
/// into one that does not perform the toggle the hardware is waiting for. The block ends by
/// selecting page 0x0000, so it leaves no page behind.
pub const SWR_EFFICIENCY: [(u16, u16); 10] = [
    (0x1f, 0x0bcd),
    (0x14, 0x5065),
    (0x14, 0xd065),
    (0x1f, 0x0bc8),
    (0x11, 0x5655),
    (0x1f, 0x0bcd),
    (0x14, 0x1065),
    (0x14, 0x9065),
    (0x14, 0x1065),
    (0x1f, 0x0000),
];
