// SPDX-License-Identifier: GPL-2.0-only
//! Clause 22 register numbers and BMCR/BMSR/ADVERTISE/LPA fields.
//!
//! Ported from Linux `include/uapi/linux/mii.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

/// A frozen named register literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegisterDef {
    pub name: &'static str,
    pub address: u8,
}

/// Generic MII registers used by `drivers/net/mii.c`.
pub const REGISTERS: [RegisterDef; 7] = [
    RegisterDef { name: "BMCR", address: 0x00 }, // include/uapi/linux/mii.h:16
    RegisterDef { name: "BMSR", address: 0x01 }, // include/uapi/linux/mii.h:17
    RegisterDef { name: "ADVERTISE", address: 0x04 }, // include/uapi/linux/mii.h:20
    RegisterDef { name: "LPA", address: 0x05 }, // include/uapi/linux/mii.h:21
    RegisterDef { name: "CTRL1000", address: 0x09 }, // include/uapi/linux/mii.h:23
    RegisterDef { name: "STAT1000", address: 0x0a }, // include/uapi/linux/mii.h:24
    RegisterDef { name: "ESTATUS", address: 0x0f }, // include/uapi/linux/mii.h:27
];

pub const MII_BMCR: u8 = 0x00; // include/uapi/linux/mii.h:16
pub const MII_BMSR: u8 = 0x01; // include/uapi/linux/mii.h:17
pub const MII_ADVERTISE: u8 = 0x04; // include/uapi/linux/mii.h:20
pub const MII_LPA: u8 = 0x05; // include/uapi/linux/mii.h:21
pub const MII_CTRL1000: u8 = 0x09; // include/uapi/linux/mii.h:23
pub const MII_STAT1000: u8 = 0x0a; // include/uapi/linux/mii.h:24
pub const MII_ESTATUS: u8 = 0x0f; // include/uapi/linux/mii.h:27

pub const BMCR_SPEED1000: u16 = 0x0040; // include/uapi/linux/mii.h:42
pub const BMCR_CTST: u16 = 0x0080; // include/uapi/linux/mii.h:43
pub const BMCR_FULLDPLX: u16 = 0x0100; // include/uapi/linux/mii.h:44
pub const BMCR_ANRESTART: u16 = 0x0200; // include/uapi/linux/mii.h:45
pub const BMCR_ISOLATE: u16 = 0x0400; // include/uapi/linux/mii.h:46
pub const BMCR_PDOWN: u16 = 0x0800; // include/uapi/linux/mii.h:47
pub const BMCR_ANENABLE: u16 = 0x1000; // include/uapi/linux/mii.h:48
pub const BMCR_SPEED100: u16 = 0x2000; // include/uapi/linux/mii.h:49
pub const BMCR_LOOPBACK: u16 = 0x4000; // include/uapi/linux/mii.h:50
pub const BMCR_RESET: u16 = 0x8000; // include/uapi/linux/mii.h:51
pub const BMCR_SPEED10: u16 = 0x0000; // include/uapi/linux/mii.h:52

pub const BMSR_ERCAP: u16 = 0x0001; // include/uapi/linux/mii.h:55
pub const BMSR_JCD: u16 = 0x0002; // include/uapi/linux/mii.h:56
pub const BMSR_LSTATUS: u16 = 0x0004; // include/uapi/linux/mii.h:57
pub const BMSR_ANEGCAPABLE: u16 = 0x0008; // include/uapi/linux/mii.h:58
pub const BMSR_RFAULT: u16 = 0x0010; // include/uapi/linux/mii.h:59
pub const BMSR_ANEGCOMPLETE: u16 = 0x0020; // include/uapi/linux/mii.h:60
pub const BMSR_ESTATEN: u16 = 0x0100; // include/uapi/linux/mii.h:62
pub const BMSR_100HALF2: u16 = 0x0200; // include/uapi/linux/mii.h:63
pub const BMSR_100FULL2: u16 = 0x0400; // include/uapi/linux/mii.h:64
pub const BMSR_10HALF: u16 = 0x0800; // include/uapi/linux/mii.h:65
pub const BMSR_10FULL: u16 = 0x1000; // include/uapi/linux/mii.h:66
pub const BMSR_100HALF: u16 = 0x2000; // include/uapi/linux/mii.h:67
pub const BMSR_100FULL: u16 = 0x4000; // include/uapi/linux/mii.h:68
pub const BMSR_100BASE4: u16 = 0x8000; // include/uapi/linux/mii.h:69

pub const ADVERTISE_SLCT: u16 = 0x001f; // include/uapi/linux/mii.h:72
pub const ADVERTISE_CSMA: u16 = 0x0001; // include/uapi/linux/mii.h:73
pub const ADVERTISE_10HALF: u16 = 0x0020; // include/uapi/linux/mii.h:74
pub const ADVERTISE_10FULL: u16 = 0x0040; // include/uapi/linux/mii.h:76
pub const ADVERTISE_100HALF: u16 = 0x0080; // include/uapi/linux/mii.h:78
pub const ADVERTISE_100FULL: u16 = 0x0100; // include/uapi/linux/mii.h:80
pub const ADVERTISE_100BASE4: u16 = 0x0200; // include/uapi/linux/mii.h:82
pub const ADVERTISE_PAUSE_CAP: u16 = 0x0400; // include/uapi/linux/mii.h:83
pub const ADVERTISE_PAUSE_ASYM: u16 = 0x0800; // include/uapi/linux/mii.h:84
pub const ADVERTISE_RFAULT: u16 = 0x2000; // include/uapi/linux/mii.h:86
pub const ADVERTISE_LPACK: u16 = 0x4000; // include/uapi/linux/mii.h:87
pub const ADVERTISE_NPAGE: u16 = 0x8000; // include/uapi/linux/mii.h:88
pub const ADVERTISE_FULL: u16 = 0x0141; // include/uapi/linux/mii.h:90-91
pub const ADVERTISE_ALL: u16 = 0x01e0; // include/uapi/linux/mii.h:92-94

pub const LPA_10HALF: u16 = 0x0020; // include/uapi/linux/mii.h:97
pub const LPA_10FULL: u16 = 0x0040; // include/uapi/linux/mii.h:99
pub const LPA_100HALF: u16 = 0x0080; // include/uapi/linux/mii.h:101
pub const LPA_100FULL: u16 = 0x0100; // include/uapi/linux/mii.h:103
pub const LPA_100BASE4: u16 = 0x0200; // include/uapi/linux/mii.h:105
pub const LPA_PAUSE_CAP: u16 = 0x0400; // include/uapi/linux/mii.h:106
pub const LPA_PAUSE_ASYM: u16 = 0x0800; // include/uapi/linux/mii.h:107
pub const LPA_RFAULT: u16 = 0x2000; // include/uapi/linux/mii.h:109
pub const LPA_LPACK: u16 = 0x4000; // include/uapi/linux/mii.h:110
pub const LPA_NPAGE: u16 = 0x8000; // include/uapi/linux/mii.h:111
pub const LPA_DUPLEX: u16 = 0x0140; // include/uapi/linux/mii.h:113
pub const LPA_100: u16 = 0x0380; // include/uapi/linux/mii.h:114

pub const ESTATUS_1000_TFULL: u16 = 0x2000; // include/uapi/linux/mii.h:126
pub const ESTATUS_1000_THALF: u16 = 0x1000; // include/uapi/linux/mii.h:127
pub const ADVERTISE_1000FULL: u16 = 0x0200; // include/uapi/linux/mii.h:152
pub const ADVERTISE_1000HALF: u16 = 0x0100; // include/uapi/linux/mii.h:153
pub const LPA_1000FULL: u16 = 0x0800; // include/uapi/linux/mii.h:163
pub const LPA_1000HALF: u16 = 0x0400; // include/uapi/linux/mii.h:164

/// Decoded BMCR fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bmcr {
    pub reset: bool,
    pub loopback: bool,
    pub speed100: bool,
    pub autoneg_enable: bool,
    pub power_down: bool,
    pub isolate: bool,
    pub autoneg_restart: bool,
    pub full_duplex: bool,
    pub collision_test: bool,
    pub speed1000: bool,
}

/// Decode the BMCR fields used by Linux `mii.c`.
pub const fn decode_bmcr(word: u16) -> Bmcr {
    Bmcr {
        reset: word & BMCR_RESET != 0,
        loopback: word & BMCR_LOOPBACK != 0,
        speed100: word & BMCR_SPEED100 != 0,
        autoneg_enable: word & BMCR_ANENABLE != 0,
        power_down: word & BMCR_PDOWN != 0,
        isolate: word & BMCR_ISOLATE != 0,
        autoneg_restart: word & BMCR_ANRESTART != 0,
        full_duplex: word & BMCR_FULLDPLX != 0,
        collision_test: word & BMCR_CTST != 0,
        speed1000: word & BMCR_SPEED1000 != 0,
    }
}

/// Encode only the BMCR fields represented by [`Bmcr`].
pub const fn encode_bmcr(value: Bmcr) -> u16 {
    (if value.reset { BMCR_RESET } else { 0 })
        | (if value.loopback { BMCR_LOOPBACK } else { 0 })
        | (if value.speed100 { BMCR_SPEED100 } else { 0 })
        | (if value.autoneg_enable { BMCR_ANENABLE } else { 0 })
        | (if value.power_down { BMCR_PDOWN } else { 0 })
        | (if value.isolate { BMCR_ISOLATE } else { 0 })
        | (if value.autoneg_restart { BMCR_ANRESTART } else { 0 })
        | (if value.full_duplex { BMCR_FULLDPLX } else { 0 })
        | (if value.collision_test { BMCR_CTST } else { 0 })
        | (if value.speed1000 { BMCR_SPEED1000 } else { 0 })
}

/// Decoded BMSR fields used by the MII library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bmsr {
    pub extended_register_capable: bool,
    pub jabber_detected: bool,
    pub link_up: bool,
    pub autoneg_capable: bool,
    pub remote_fault: bool,
    pub autoneg_complete: bool,
    pub extended_status: bool,
    pub mbps100_t2_half: bool,
    pub mbps100_t2_full: bool,
    pub mbps10_half: bool,
    pub mbps10_full: bool,
    pub mbps100_half: bool,
    pub mbps100_full: bool,
    pub mbps100_base4: bool,
}

/// Decode the BMSR status fields used by Linux `mii.c`.
pub const fn decode_bmsr(word: u16) -> Bmsr {
    Bmsr {
        extended_register_capable: word & BMSR_ERCAP != 0,
        jabber_detected: word & BMSR_JCD != 0,
        link_up: word & BMSR_LSTATUS != 0,
        autoneg_capable: word & BMSR_ANEGCAPABLE != 0,
        remote_fault: word & BMSR_RFAULT != 0,
        autoneg_complete: word & BMSR_ANEGCOMPLETE != 0,
        extended_status: word & BMSR_ESTATEN != 0,
        mbps100_t2_half: word & BMSR_100HALF2 != 0,
        mbps100_t2_full: word & BMSR_100FULL2 != 0,
        mbps10_half: word & BMSR_10HALF != 0,
        mbps10_full: word & BMSR_10FULL != 0,
        mbps100_half: word & BMSR_100HALF != 0,
        mbps100_full: word & BMSR_100FULL != 0,
        mbps100_base4: word & BMSR_100BASE4 != 0,
    }
}

/// Encode only the BMSR fields represented by [`Bmsr`].
pub const fn encode_bmsr(value: Bmsr) -> u16 {
    (if value.extended_register_capable { BMSR_ERCAP } else { 0 })
        | (if value.jabber_detected { BMSR_JCD } else { 0 })
        | (if value.link_up { BMSR_LSTATUS } else { 0 })
        | (if value.autoneg_capable { BMSR_ANEGCAPABLE } else { 0 })
        | (if value.remote_fault { BMSR_RFAULT } else { 0 })
        | (if value.autoneg_complete { BMSR_ANEGCOMPLETE } else { 0 })
        | (if value.extended_status { BMSR_ESTATEN } else { 0 })
        | (if value.mbps100_t2_half { BMSR_100HALF2 } else { 0 })
        | (if value.mbps100_t2_full { BMSR_100FULL2 } else { 0 })
        | (if value.mbps10_half { BMSR_10HALF } else { 0 })
        | (if value.mbps10_full { BMSR_10FULL } else { 0 })
        | (if value.mbps100_half { BMSR_100HALF } else { 0 })
        | (if value.mbps100_full { BMSR_100FULL } else { 0 })
        | (if value.mbps100_base4 { BMSR_100BASE4 } else { 0 })
}

/// Decoded technology and pause fields common to ADVERTISE and LPA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbilityWord {
    pub selector: u8,
    pub mbps10_half: bool,
    pub mbps10_full: bool,
    pub mbps100_half: bool,
    pub mbps100_full: bool,
    pub mbps100_base4: bool,
    pub pause: bool,
    pub asym_pause: bool,
    pub remote_fault: bool,
    pub acknowledge: bool,
    pub next_page: bool,
}

/// Decode an ADVERTISE or LPA register word.
pub const fn decode_ability(word: u16) -> AbilityWord {
    AbilityWord {
        selector: (word & ADVERTISE_SLCT) as u8,
        mbps10_half: word & ADVERTISE_10HALF != 0,
        mbps10_full: word & ADVERTISE_10FULL != 0,
        mbps100_half: word & ADVERTISE_100HALF != 0,
        mbps100_full: word & ADVERTISE_100FULL != 0,
        mbps100_base4: word & ADVERTISE_100BASE4 != 0,
        pause: word & ADVERTISE_PAUSE_CAP != 0,
        asym_pause: word & ADVERTISE_PAUSE_ASYM != 0,
        remote_fault: word & ADVERTISE_RFAULT != 0,
        acknowledge: word & ADVERTISE_LPACK != 0,
        next_page: word & ADVERTISE_NPAGE != 0,
    }
}

/// Encode an ADVERTISE/LPA ability value; selector values outside the five-bit field are refused.
pub const fn encode_ability(value: AbilityWord) -> Result<u16, AbilityEncodeError> {
    if value.selector > ADVERTISE_SLCT as u8 {
        return Err(AbilityEncodeError::SelectorOutOfRange {
            selector: value.selector,
            maximum: ADVERTISE_SLCT as u8,
        });
    }
    Ok(value.selector as u16
        | (if value.mbps10_half { ADVERTISE_10HALF } else { 0 })
        | (if value.mbps10_full { ADVERTISE_10FULL } else { 0 })
        | (if value.mbps100_half { ADVERTISE_100HALF } else { 0 })
        | (if value.mbps100_full { ADVERTISE_100FULL } else { 0 })
        | (if value.mbps100_base4 { ADVERTISE_100BASE4 } else { 0 })
        | (if value.pause { ADVERTISE_PAUSE_CAP } else { 0 })
        | (if value.asym_pause { ADVERTISE_PAUSE_ASYM } else { 0 })
        | (if value.remote_fault { ADVERTISE_RFAULT } else { 0 })
        | (if value.acknowledge { ADVERTISE_LPACK } else { 0 })
        | (if value.next_page { ADVERTISE_NPAGE } else { 0 }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbilityEncodeError {
    SelectorOutOfRange { selector: u8, maximum: u8 },
}
