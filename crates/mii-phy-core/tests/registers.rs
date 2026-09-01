// SPDX-License-Identifier: GPL-2.0-only
//! Register vectors ported from Linux `include/uapi/linux/mii.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

use mii_phy_core::registers::*;

#[test]
fn seven_mii_c_registers_are_frozen_by_count_name_and_literal() {
    // include/uapi/linux/mii.h:16-27; only registers used by drivers/net/mii.c are in this layer.
    assert_eq!(REGISTERS.len(), 7);
    let expected = [
        ("BMCR", 0x00),
        ("BMSR", 0x01),
        ("ADVERTISE", 0x04),
        ("LPA", 0x05),
        ("CTRL1000", 0x09),
        ("STAT1000", 0x0a),
        ("ESTATUS", 0x0f),
    ];
    for (got, (name, address)) in REGISTERS.iter().zip(expected) {
        assert_eq!(got.name, name);
        assert_eq!(got.address, address, "{name}");
    }
    assert_eq!(MII_BMCR, 0x00);
    assert_eq!(MII_BMSR, 0x01);
    assert_eq!(MII_ADVERTISE, 0x04);
    assert_eq!(MII_LPA, 0x05);
    assert_eq!(MII_CTRL1000, 0x09);
    assert_eq!(MII_STAT1000, 0x0a);
    assert_eq!(MII_ESTATUS, 0x0f);
}

#[test]
fn every_bmsr_constant_is_pinned_to_its_linux_literal() {
    // include/uapi/linux/mii.h:55-69. Keep this list independent of BMSR_FIELDS: a table built
    // from a mistyped public constant would otherwise only prove that the typo equals itself.
    let got = [
        ("ERCAP", BMSR_ERCAP),
        ("JCD", BMSR_JCD),
        ("LSTATUS", BMSR_LSTATUS),
        ("ANEGCAPABLE", BMSR_ANEGCAPABLE),
        ("RFAULT", BMSR_RFAULT),
        ("ANEGCOMPLETE", BMSR_ANEGCOMPLETE),
        ("ESTATEN", BMSR_ESTATEN),
        ("100HALF2", BMSR_100HALF2),
        ("100FULL2", BMSR_100FULL2),
        ("10HALF", BMSR_10HALF),
        ("10FULL", BMSR_10FULL),
        ("100HALF", BMSR_100HALF),
        ("100FULL", BMSR_100FULL),
        ("100BASE4", BMSR_100BASE4),
    ];
    let expected = [
        ("ERCAP", 0x0001),
        ("JCD", 0x0002),
        ("LSTATUS", 0x0004),
        ("ANEGCAPABLE", 0x0008),
        ("RFAULT", 0x0010),
        ("ANEGCOMPLETE", 0x0020),
        ("ESTATEN", 0x0100),
        ("100HALF2", 0x0200),
        ("100FULL2", 0x0400),
        ("10HALF", 0x0800),
        ("10FULL", 0x1000),
        ("100HALF", 0x2000),
        ("100FULL", 0x4000),
        ("100BASE4", 0x8000),
    ];
    assert_eq!(got.len(), 14);
    assert_eq!(got, expected);
}

#[test]
fn every_advertise_and_lpa_constant_is_pinned_to_its_linux_literal() {
    // include/uapi/linux/mii.h:72-114,152-164. These are direct constant checks, not values
    // copied from the production field tables.
    let advertise = [
        ("SLCT", ADVERTISE_SLCT),
        ("CSMA", ADVERTISE_CSMA),
        ("10HALF", ADVERTISE_10HALF),
        ("10FULL", ADVERTISE_10FULL),
        ("100HALF", ADVERTISE_100HALF),
        ("100FULL", ADVERTISE_100FULL),
        ("100BASE4", ADVERTISE_100BASE4),
        ("PAUSE_CAP", ADVERTISE_PAUSE_CAP),
        ("PAUSE_ASYM", ADVERTISE_PAUSE_ASYM),
        ("RFAULT", ADVERTISE_RFAULT),
        ("LPACK", ADVERTISE_LPACK),
        ("NPAGE", ADVERTISE_NPAGE),
        ("FULL", ADVERTISE_FULL),
        ("ALL", ADVERTISE_ALL),
        ("1000FULL", ADVERTISE_1000FULL),
        ("1000HALF", ADVERTISE_1000HALF),
    ];
    let expected_advertise = [
        ("SLCT", 0x001f),
        ("CSMA", 0x0001),
        ("10HALF", 0x0020),
        ("10FULL", 0x0040),
        ("100HALF", 0x0080),
        ("100FULL", 0x0100),
        ("100BASE4", 0x0200),
        ("PAUSE_CAP", 0x0400),
        ("PAUSE_ASYM", 0x0800),
        ("RFAULT", 0x2000),
        ("LPACK", 0x4000),
        ("NPAGE", 0x8000),
        ("FULL", 0x0141),
        ("ALL", 0x01e0),
        ("1000FULL", 0x0200),
        ("1000HALF", 0x0100),
    ];
    assert_eq!(advertise.len(), 16);
    assert_eq!(advertise, expected_advertise);

    let lpa = [
        ("10HALF", LPA_10HALF),
        ("10FULL", LPA_10FULL),
        ("100HALF", LPA_100HALF),
        ("100FULL", LPA_100FULL),
        ("100BASE4", LPA_100BASE4),
        ("PAUSE_CAP", LPA_PAUSE_CAP),
        ("PAUSE_ASYM", LPA_PAUSE_ASYM),
        ("RFAULT", LPA_RFAULT),
        ("LPACK", LPA_LPACK),
        ("NPAGE", LPA_NPAGE),
        ("DUPLEX", LPA_DUPLEX),
        ("100", LPA_100),
        ("1000FULL", LPA_1000FULL),
        ("1000HALF", LPA_1000HALF),
    ];
    let expected_lpa = [
        ("10HALF", 0x0020),
        ("10FULL", 0x0040),
        ("100HALF", 0x0080),
        ("100FULL", 0x0100),
        ("100BASE4", 0x0200),
        ("PAUSE_CAP", 0x0400),
        ("PAUSE_ASYM", 0x0800),
        ("RFAULT", 0x2000),
        ("LPACK", 0x4000),
        ("NPAGE", 0x8000),
        ("DUPLEX", 0x0140),
        ("100", 0x0380),
        ("1000FULL", 0x0800),
        ("1000HALF", 0x0400),
    ];
    assert_eq!(lpa.len(), 14);
    assert_eq!(lpa, expected_lpa);
}

#[test]
fn bmcr_decodes_and_encodes_linux_literals() {
    // include/uapi/linux/mii.h:42-52.
    let value = decode_bmcr(0xffff);
    assert_eq!(value, Bmcr {
        reset: true,
        loopback: true,
        speed100: true,
        autoneg_enable: true,
        power_down: true,
        isolate: true,
        autoneg_restart: true,
        full_duplex: true,
        collision_test: true,
        speed1000: true,
    });
    assert_eq!(encode_bmcr(value), 0xffc0);
    assert_eq!(BMCR_SPEED10, 0x0000);
}

#[test]
fn bmsr_decodes_and_encodes_linux_literals() {
    // include/uapi/linux/mii.h:57-69. Capability bits not represented by Bmsr are ignored.
    let value = decode_bmsr(0xffff);
    assert_eq!(value, Bmsr {
        extended_register_capable: true,
        jabber_detected: true,
        link_up: true,
        autoneg_capable: true,
        remote_fault: true,
        autoneg_complete: true,
        extended_status: true,
        mbps100_t2_half: true,
        mbps100_t2_full: true,
        mbps10_half: true,
        mbps10_full: true,
        mbps100_half: true,
        mbps100_full: true,
        mbps100_base4: true,
    });
    assert_eq!(encode_bmsr(value), 0xff3f);
}

#[test]
fn a_real_bmsr_word_decodes_to_the_exact_supported_mode_set() {
    // drivers/net/phy/mxl-86110.c:178 supplies a real BMSR reset word, 0x7949.
    // Capability positions are include/uapi/linux/mii.h:58,65-68.
    let bmsr = decode_bmsr(0x7949);
    let got: std::vec::Vec<&str> = [
        ("Autoneg", bmsr.autoneg_capable),
        ("10baseT_Half", bmsr.mbps10_half),
        ("10baseT_Full", bmsr.mbps10_full),
        ("100baseT_Half", bmsr.mbps100_half),
        ("100baseT_Full", bmsr.mbps100_full),
        ("100baseT2_Half", bmsr.mbps100_t2_half),
        ("100baseT2_Full", bmsr.mbps100_t2_full),
        ("100baseT4", bmsr.mbps100_base4),
    ]
    .into_iter()
    .filter_map(|(name, supported)| supported.then_some(name))
    .collect();
    let expected = std::vec![
        "Autoneg",
        "10baseT_Half",
        "10baseT_Full",
        "100baseT_Half",
        "100baseT_Full",
    ];
    assert_eq!(got.len(), 5);
    assert_eq!(got, expected);
}

#[test]
fn advertise_and_lpa_decode_and_encode_the_same_wire_positions() {
    // include/uapi/linux/mii.h:72-88 and :96-111.
    let word = 0xefe1;
    let value = decode_ability(word);
    assert_eq!(value, AbilityWord {
        selector: 1,
        mbps10_half: true,
        mbps10_full: true,
        mbps100_half: true,
        mbps100_full: true,
        mbps100_base4: true,
        pause: true,
        asym_pause: true,
        remote_fault: true,
        acknowledge: true,
        next_page: true,
    });
    assert_eq!(encode_ability(value), Ok(0xefe1));
}

#[test]
fn selector_overflow_is_named_and_not_silently_truncated() {
    let value = AbilityWord {
        selector: 0x20,
        mbps10_half: false,
        mbps10_full: false,
        mbps100_half: false,
        mbps100_full: false,
        mbps100_base4: false,
        pause: false,
        asym_pause: false,
        remote_fault: false,
        acknowledge: false,
        next_page: false,
    };
    assert_eq!(encode_ability(value), Err(AbilityEncodeError::SelectorOutOfRange {
        selector: 0x20,
        maximum: 0x1f, // include/uapi/linux/mii.h:72, ADVERTISE_SLCT
    }));
}
