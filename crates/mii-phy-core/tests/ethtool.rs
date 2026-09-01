// SPDX-License-Identifier: GPL-2.0-only
//! Vectors from Linux `include/linux/mii.h` and `include/uapi/linux/ethtool.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

use mii_phy_core::ethtool::*;

#[test]
fn every_public_advertised_bit_is_pinned_to_its_linux_literal() {
    // include/uapi/linux/ethtool.h:1964-1978,2149-2163. The macros are 1UL shifted by the
    // independently listed bit indices; write the resulting masks literally here.
    let got = [
        ("10baseT_Half", ADVERTISED_10BASE_T_HALF),
        ("10baseT_Full", ADVERTISED_10BASE_T_FULL),
        ("100baseT_Half", ADVERTISED_100BASE_T_HALF),
        ("100baseT_Full", ADVERTISED_100BASE_T_FULL),
        ("1000baseT_Half", ADVERTISED_1000BASE_T_HALF),
        ("1000baseT_Full", ADVERTISED_1000BASE_T_FULL),
        ("Autoneg", ADVERTISED_AUTONEG),
        ("TP", ADVERTISED_TP),
        ("MII", ADVERTISED_MII),
        ("Pause", ADVERTISED_PAUSE),
        ("Asym_Pause", ADVERTISED_ASYM_PAUSE),
    ];
    let expected = [
        ("10baseT_Half", 0x0001),
        ("10baseT_Full", 0x0002),
        ("100baseT_Half", 0x0004),
        ("100baseT_Full", 0x0008),
        ("1000baseT_Half", 0x0010),
        ("1000baseT_Full", 0x0020),
        ("Autoneg", 0x0040),
        ("TP", 0x0080),
        ("MII", 0x0200),
        ("Pause", 0x2000),
        ("Asym_Pause", 0x4000),
    ];
    assert_eq!(got.len(), 11);
    assert_eq!(got, expected);
}

#[test]
fn eleven_mii_link_modes_are_frozen_by_count_name_and_literal() {
    // include/uapi/linux/ethtool.h:1964-1978. Literal masks are 1 << the listed bit index.
    assert_eq!(MII_LINK_MODES.len(), 11);
    let expected = [
        ("10baseT_Half", 0x0001),
        ("10baseT_Full", 0x0002),
        ("100baseT_Half", 0x0004),
        ("100baseT_Full", 0x0008),
        ("1000baseT_Half", 0x0010),
        ("1000baseT_Full", 0x0020),
        ("Autoneg", 0x0040),
        ("TP", 0x0080),
        ("MII", 0x0200),
        ("Pause", 0x2000),
        ("Asym_Pause", 0x4000),
    ];
    for (got, (name, mask)) in MII_LINK_MODES.iter().zip(expected) {
        assert_eq!(got.name, name);
        assert_eq!(got.mask, mask, "{name}");
    }
}

#[test]
fn ethtool_advertisement_converts_to_mii_advertise() {
    // include/linux/mii.h:119-131; MII literals from include/uapi/linux/mii.h:74-84.
    assert_eq!(ethtool_adv_to_mii_adv(0x600f), 0x0de0);
    assert_eq!(ethtool_adv_to_mii_adv(0x0030), 0x0000, "gigabit lives in CTRL1000");
}

#[test]
fn mii_advertise_converts_to_ethtool_advertisement() {
    // include/linux/mii.h:170-182.
    assert_eq!(mii_adv_to_ethtool_adv(0x0de0), 0x600f);
    assert_eq!(mii_adv_to_ethtool_adv(0x4001), 0x0000, "selector and LPACK are not media");
}

#[test]
fn ethtool_gigabit_modes_convert_to_ctrl1000_and_back() {
    // include/linux/mii.h:202-207 and :245-250.
    assert_eq!(ethtool_adv_to_mii_ctrl1000(0x0030), 0x0300);
    assert_eq!(mii_ctrl1000_to_ethtool_adv(0x0300), 0x0030);
}

#[test]
fn lpa_conversion_adds_autoneg_acknowledgement() {
    // include/linux/mii.h:265-268; LPA_LPACK is 0x4000 at include/uapi/linux/mii.h:110.
    assert_eq!(mii_lpa_to_ethtool_lpa(0x4160), 0x004b);
    assert_eq!(mii_lpa_to_ethtool_lpa(0x0160), 0x000b);
}

#[test]
fn stat1000_conversion_uses_partner_bit_positions() {
    // include/linux/mii.h:283-288; STAT1000 literals at include/uapi/linux/mii.h:163-164.
    assert_eq!(mii_stat1000_to_ethtool_lpa(0x0c00), 0x0030);
    assert_eq!(mii_stat1000_to_ethtool_lpa(0x0300), 0x0000,
        "CTRL1000 positions must not be decoded as STAT1000 positions");
}
