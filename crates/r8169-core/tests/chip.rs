// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for chip identification. Every expected value is the LINUX literal, quoted with the
//! line it comes from — asserting against `CHIP_INFOS` itself would only prove the table equals
//! itself.

use r8169_core::chip::{identify, txconfig_is_pci_read_failure, xid_from_txconfig, MacVersion,
                       ChipInfo, CHIP_INFOS};

/// r8169_main.c:5647 — `xid = (txconfig >> 20) & 0xfcf`.
#[test]
fn the_xid_mask_drops_bits_four_and_five() {
    // A TxConfig whose shifted value is 0xfff: the two dropped bits must not survive.
    assert_eq!(xid_from_txconfig(0xfff0_0000), 0xfcf);
    // The Z83's part, from the CoralOS driver's own comment: XID 0x4c0.
    assert_eq!(xid_from_txconfig(0x4c0 << 20), 0x4c0);
    // 0xfff, NOT 0xfcf, is what a naive "top bits" read would yield — pin the difference.
    assert_ne!(xid_from_txconfig(0xfff0_0000), 0xfff);
}

/// r8169_main.c:5644 — `if (txconfig == ~0U) return dev_err_probe(..., -EIO, "PCI read failed")`.
#[test]
fn an_all_ones_txconfig_is_a_dead_bus_not_a_chip() {
    assert!(txconfig_is_pci_read_failure(u32::MAX));
    assert!(!txconfig_is_pci_read_failure(0x4c0 << 20));
}

/// r8169_main.c:145 — `{ 0x7cf, 0x4c0, RTL_GIGA_MAC_VER_40, "RTL8168g/8111g", ... }`.
/// This is the part on the Z83, which coral-net-frame currently HARDCODES.
#[test]
fn the_z83_part_identifies_as_8168g_ver_40() {
    let c = identify(0x4c0, true);
    assert_eq!(c.version, MacVersion(40));
    assert_eq!(c.name, "RTL8168g/8111g");
}

/// r8169_main.c:2459-2464 — the two chips that combine a 1Gbps MAC with a 100Mbps PHY. Without
/// gmii the XID's own answer is WRONG, and dropping these lines is the easy porting mistake.
#[test]
fn without_gmii_two_versions_are_redirected() {
    // :143 { 0x7cf, 0x509, RTL_GIGA_MAC_VER_42, "RTL8168gu/8111gu" } -> VER_43 RTL8106eus
    assert_eq!(identify(0x509, true).version, MacVersion(42));
    assert_eq!(identify(0x509, false).version, MacVersion(43));
    assert_eq!(identify(0x509, false).name, "RTL8106eus");
    // :136 { 0x7cf, 0x541, RTL_GIGA_MAC_VER_46, "RTL8168h/8111h" } -> VER_48 RTL8107e
    assert_eq!(identify(0x541, true).version, MacVersion(46));
    assert_eq!(identify(0x541, false).version, MacVersion(48));
    assert_eq!(identify(0x541, false).name, "RTL8107e");
    // and gmii must NOT disturb a part that is neither
    assert_eq!(identify(0x4c0, true).version, identify(0x4c0, false).version);
}

/// The scan takes the FIRST match, and narrow masks precede family-wide ones that also match.
/// r8169_main.c:147-152: 0x7c8/0x2c8 (VER_34) comes BEFORE 0x7cf/0x2c1 (VER_32) and 0x7c8/0x2c0
/// (VER_33). An XID of 0x2c1 matches BOTH 0x7cf/0x2c1 and 0x7c8/0x2c0 — order decides.
#[test]
fn order_decides_when_two_rows_match() {
    assert_eq!(identify(0x2c1, true).version, MacVersion(32), "the narrower row must win");
    assert_eq!(identify(0x2c0, true).version, MacVersion(33));
    // Prove the ambiguity is real rather than asserted: both rows DO match 0x2c1.
    let matching: Vec<&ChipInfo> = CHIP_INFOS.iter().filter(|c| 0x2c1 & c.mask == c.val).collect();
    assert!(matching.len() >= 2, "expected an ambiguous XID, got {matching:?}");
}

/// r8169_main.c:174 — `{ 0x000, 0x000, RTL_GIGA_MAC_NONE }`. The C scan has no bounds check, so
/// the catch-all is what stops it walking off the table.
#[test]
fn the_table_ends_in_a_catch_all_that_matches_everything() {
    let last = CHIP_INFOS.last().expect("table is not empty");
    assert_eq!((last.mask, last.val), (0x000, 0x000), "the final row must match anything");
    assert_eq!(last.version, MacVersion::NONE);
    // An XID no real row claims still resolves, and resolves to NONE rather than to a wrong chip.
    assert_eq!(identify(0xabc, true).version, MacVersion::NONE);
}

/// The EXTENDED row is a marker, not a chip: identity lives in TX_CONFIG_V2 (r8169_main.c:5652).
/// A caller that treats it as an answer configures a part it has not identified.
#[test]
fn extended_is_a_marker_not_a_chip() {
    assert_eq!(identify(0x7c8, true).version, MacVersion::EXTENDED);
    assert_ne!(MacVersion::EXTENDED, MacVersion::NONE);
}

/// The table was transcribed mechanically; this pins the count so a partial re-extraction cannot
/// land quietly. 56 rows: 54 named + the EXTENDED marker + the catch-all (r8169_main.c:104-174).
#[test]
fn the_whole_table_was_ported_not_a_subset() {
    assert_eq!(CHIP_INFOS.len(), 56);
    let named = CHIP_INFOS.iter().filter(|c| c.name != "(unnamed)").count();
    assert_eq!(named, 54);
}
