// SPDX-License-Identifier: GPL-2.0-only
//! Chip identification — which Realtek part is actually on this board.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * the `rtl_chip_infos[]` table (:104-:174) — all 56 entries, transcribed mechanically from
//!     the C source rather than by hand, because a mistyped mask silently identifies the wrong
//!     part and then applies the wrong quirks to real silicon;
//!   * `rtl8169_get_chip_version()` (:2442-:2466) — the linear scan and the two gmii overrides;
//!   * the XID derivation at :5647, `xid = (txconfig >> 20) & 0xfcf`.
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! WHY THIS MATTERS FOR CORALOS. `userland/coral-net-frame/src/r8169.rs` currently hardcodes one
//! board: "Target NIC XID 0x4c0 = 8168g". That is correct for the Z83 and wrong for everything
//! else — the BSP rule this tree already keeps for other devices. This module replaces the
//! assumption with the detection Linux actually performs.

/// MAC version, as the number Linux gives it (`RTL_GIGA_MAC_VER_40` is `MacVersion(40)`).
///
/// A newtype rather than a 56-variant enum, because Linux uses the ORDERING in range checks
/// (`case RTL_GIGA_MAC_VER_40 ... RTL_GIGA_MAC_VER_LAST:`, r8169_main.c:1371) and a newtype over
/// `u8` keeps that comparable without inventing names the reference does not have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MacVersion(pub u8);

impl MacVersion {
    /// `RTL_GIGA_MAC_NONE` — the catch-all matched nothing more specific.
    pub const NONE: MacVersion = MacVersion(0);
    /// `RTL_GIGA_MAC_VER_EXTENDED` — identity lives in a second register (TX_CONFIG_V2) and this
    /// module does NOT resolve it. Callers must treat it as "ask again", not as a chip.
    pub const EXTENDED: MacVersion = MacVersion(255);
}

/// One row of Linux's `rtl_chip_infos[]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChipInfo {
    pub mask: u32,
    pub val: u32,
    pub version: MacVersion,
    pub name: &'static str,
}

/// `xid = (txconfig >> 20) & 0xfcf` (r8169_main.c:5647).
///
/// The mask is 0xfcf, NOT 0xfff: bits 4 and 5 of the shifted value are dropped before matching.
/// Reading "the top bits of TxConfig" without it identifies the wrong row.
pub fn xid_from_txconfig(txconfig: u32) -> u32 {
    (txconfig >> 20) & 0xfcf
}

/// Linux checks this before identifying anything (r8169_main.c:5644): an all-ones TxConfig is a
/// PCI read failure, not a chip that happens to report every bit.
pub fn txconfig_is_pci_read_failure(txconfig: u32) -> bool {
    txconfig == u32::MAX
}

/// The table, in Linux's order. ORDER IS LOAD-BEARING: the scan takes the FIRST match, and
/// narrower masks (0x7cf) deliberately precede the family-wide ones (0x7c8) that would also match.
pub const CHIP_INFOS: &[ChipInfo] = &[
    ChipInfo { mask: 0x7cf, val: 0x6c9, version: MacVersion(80), name: "RTL8127A" },
    ChipInfo { mask: 0x7cf, val: 0x64a, version: MacVersion(70), name: "RTL8126A" },
    ChipInfo { mask: 0x7cf, val: 0x649, version: MacVersion(70), name: "RTL8126A" },
    ChipInfo { mask: 0x7cf, val: 0x681, version: MacVersion(66), name: "RTL8125BP" },
    ChipInfo { mask: 0x7cf, val: 0x68b, version: MacVersion(64), name: "RTL9151A" },
    ChipInfo { mask: 0x7cf, val: 0x68a, version: MacVersion(64), name: "RTL8125K" },
    ChipInfo { mask: 0x7cf, val: 0x689, version: MacVersion(64), name: "RTL8125D" },
    ChipInfo { mask: 0x7cf, val: 0x688, version: MacVersion(64), name: "RTL8125D" },
    ChipInfo { mask: 0x7cf, val: 0x641, version: MacVersion(63), name: "RTL8125B" },
    ChipInfo { mask: 0x7cf, val: 0x609, version: MacVersion(61), name: "RTL8125A" },
    ChipInfo { mask: 0x7cf, val: 0x54b, version: MacVersion(52), name: "RTL8168fp/RTL8117" },
    ChipInfo { mask: 0x7cf, val: 0x54a, version: MacVersion(52), name: "RTL8168fp/RTL8117" },
    ChipInfo { mask: 0x7cf, val: 0x502, version: MacVersion(51), name: "RTL8168ep/8111ep" },
    ChipInfo { mask: 0x7cf, val: 0x541, version: MacVersion(46), name: "RTL8168h/8111h" },
    ChipInfo { mask: 0x7cf, val: 0x6c0, version: MacVersion(46), name: "RTL8168M" },
    ChipInfo { mask: 0x7cf, val: 0x5c8, version: MacVersion(44), name: "RTL8411b" },
    ChipInfo { mask: 0x7cf, val: 0x509, version: MacVersion(42), name: "RTL8168gu/8111gu" },
    ChipInfo { mask: 0x7cf, val: 0x4c0, version: MacVersion(40), name: "RTL8168g/8111g" },
    ChipInfo { mask: 0x7c8, val: 0x488, version: MacVersion(38), name: "RTL8411" },
    ChipInfo { mask: 0x7cf, val: 0x481, version: MacVersion(36), name: "RTL8168f/8111f" },
    ChipInfo { mask: 0x7cf, val: 0x480, version: MacVersion(35), name: "RTL8168f/8111f" },
    ChipInfo { mask: 0x7c8, val: 0x2c8, version: MacVersion(34), name: "RTL8168evl/8111evl" },
    ChipInfo { mask: 0x7cf, val: 0x2c1, version: MacVersion(32), name: "RTL8168e/8111e" },
    ChipInfo { mask: 0x7c8, val: 0x2c0, version: MacVersion(33), name: "RTL8168e/8111e" },
    ChipInfo { mask: 0x7cf, val: 0x281, version: MacVersion(25), name: "RTL8168d/8111d" },
    ChipInfo { mask: 0x7c8, val: 0x280, version: MacVersion(26), name: "RTL8168d/8111d" },
    ChipInfo { mask: 0x7cf, val: 0x28a, version: MacVersion(28), name: "RTL8168dp/8111dp" },
    ChipInfo { mask: 0x7cf, val: 0x28b, version: MacVersion(31), name: "RTL8168dp/8111dp" },
    ChipInfo { mask: 0x7cf, val: 0x3c9, version: MacVersion(23), name: "RTL8168cp/8111cp" },
    ChipInfo { mask: 0x7cf, val: 0x3c8, version: MacVersion(18), name: "RTL8168cp/8111cp" },
    ChipInfo { mask: 0x7c8, val: 0x3c8, version: MacVersion(24), name: "RTL8168cp/8111cp" },
    ChipInfo { mask: 0x7cf, val: 0x3c0, version: MacVersion(19), name: "RTL8168c/8111c" },
    ChipInfo { mask: 0x7cf, val: 0x3c2, version: MacVersion(20), name: "RTL8168c/8111c" },
    ChipInfo { mask: 0x7cf, val: 0x3c3, version: MacVersion(21), name: "RTL8168c/8111c" },
    ChipInfo { mask: 0x7c8, val: 0x3c0, version: MacVersion(22), name: "RTL8168c/8111c" },
    ChipInfo { mask: 0x7c8, val: 0x380, version: MacVersion(17), name: "RTL8168b/8111b" },
    ChipInfo { mask: 0x7c8, val: 0x300, version: MacVersion(11), name: "RTL8168b/8111b" },
    ChipInfo { mask: 0x7c8, val: 0x448, version: MacVersion(39), name: "RTL8106e" },
    ChipInfo { mask: 0x7c8, val: 0x440, version: MacVersion(37), name: "RTL8402" },
    ChipInfo { mask: 0x7cf, val: 0x409, version: MacVersion(29), name: "RTL8105e" },
    ChipInfo { mask: 0x7c8, val: 0x408, version: MacVersion(30), name: "RTL8105e" },
    ChipInfo { mask: 0x7cf, val: 0x349, version: MacVersion(8), name: "RTL8102e" },
    ChipInfo { mask: 0x7cf, val: 0x249, version: MacVersion(8), name: "RTL8102e" },
    ChipInfo { mask: 0x7cf, val: 0x348, version: MacVersion(7), name: "RTL8102e" },
    ChipInfo { mask: 0x7cf, val: 0x248, version: MacVersion(7), name: "RTL8102e" },
    ChipInfo { mask: 0x7cf, val: 0x240, version: MacVersion(14), name: "RTL8401" },
    ChipInfo { mask: 0x7c8, val: 0x348, version: MacVersion(9), name: "RTL8102e/RTL8103e" },
    ChipInfo { mask: 0x7c8, val: 0x248, version: MacVersion(9), name: "RTL8102e/RTL8103e" },
    ChipInfo { mask: 0x7c8, val: 0x340, version: MacVersion(10), name: "RTL8101e/RTL8100e" },
    ChipInfo { mask: 0xfc8, val: 0x980, version: MacVersion(6), name: "RTL8169sc/8110sc" },
    ChipInfo { mask: 0xfc8, val: 0x180, version: MacVersion(5), name: "RTL8169sc/8110sc" },
    ChipInfo { mask: 0xfc8, val: 0x100, version: MacVersion(4), name: "RTL8169sb/8110sb" },
    ChipInfo { mask: 0xfc8, val: 0x040, version: MacVersion(3), name: "RTL8110s" },
    ChipInfo { mask: 0xfc8, val: 0x008, version: MacVersion(2), name: "RTL8169s" },
    ChipInfo { mask: 0x7cf, val: 0x7c8, version: MacVersion::EXTENDED, name: "(unnamed)" },
    ChipInfo { mask: 0x000, val: 0x000, version: MacVersion::NONE, name: "(unnamed)" },
];

/// `rtl8169_get_chip_version()` (r8169_main.c:2442).
///
/// The C is `while ((xid & p->mask) != p->val) p++;` with NO bounds check — safe only because the
/// table ends in `{ 0x000, 0x000, RTL_GIGA_MAC_NONE }`, whose zero mask matches everything. This
/// port keeps the catch-all in the table AND cannot run off the end, so the invariant is expressed
/// twice rather than relied upon once.
pub fn identify(xid: u32, gmii: bool) -> ChipInfo {
    let hit = CHIP_INFOS
        .iter()
        .find(|c| xid & c.mask == c.val)
        .copied()
        .unwrap_or(ChipInfo { mask: 0, val: 0, version: MacVersion::NONE, name: "(no catch-all)" });

    // Chips combining a 1Gbps MAC with a 100Mbps PHY. Without gmii the part is NOT what the XID
    // says: VER_42 is really an RTL8106eus and VER_46 an RTL8107e. Dropping these two lines is the
    // easy way to port this function and get a plausible wrong answer on two real parts.
    match (hit.version, gmii) {
        (MacVersion(42), false) => ChipInfo { version: MacVersion(43), name: "RTL8106eus", ..hit },
        (MacVersion(46), false) => ChipInfo { version: MacVersion(48), name: "RTL8107e", ..hit },
        _ => hit,
    }
}
