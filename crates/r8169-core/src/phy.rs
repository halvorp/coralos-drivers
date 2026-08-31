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
