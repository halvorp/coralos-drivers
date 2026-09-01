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
