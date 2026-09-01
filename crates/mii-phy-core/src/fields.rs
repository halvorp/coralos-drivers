// SPDX-License-Identifier: GPL-2.0-only
//! Frozen named field tables for the four core MII words.
//!
//! Ported from Linux `include/uapi/linux/mii.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

/// One named register field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FieldDef {
    pub name: &'static str,
    pub mask: u16,
}

/// BMCR fields used by `drivers/net/mii.c`.
pub const BMCR_FIELDS: [FieldDef; 10] = [
    FieldDef { name: "RESET", mask: 0x8000 }, // include/uapi/linux/mii.h:51
    FieldDef { name: "LOOPBACK", mask: 0x4000 }, // include/uapi/linux/mii.h:50
    FieldDef { name: "SPEED100", mask: 0x2000 }, // include/uapi/linux/mii.h:49
    FieldDef { name: "ANENABLE", mask: 0x1000 }, // include/uapi/linux/mii.h:48
    FieldDef { name: "PDOWN", mask: 0x0800 }, // include/uapi/linux/mii.h:47
    FieldDef { name: "ISOLATE", mask: 0x0400 }, // include/uapi/linux/mii.h:46
    FieldDef { name: "ANRESTART", mask: 0x0200 }, // include/uapi/linux/mii.h:45
    FieldDef { name: "FULLDPLX", mask: 0x0100 }, // include/uapi/linux/mii.h:44
    FieldDef { name: "CTST", mask: 0x0080 }, // include/uapi/linux/mii.h:43
    FieldDef { name: "SPEED1000", mask: 0x0040 }, // include/uapi/linux/mii.h:42
];

/// BMSR fields decoded or consulted by this crate.
pub const BMSR_FIELDS: [FieldDef; 14] = [
    FieldDef { name: "ERCAP", mask: 0x0001 }, // include/uapi/linux/mii.h:55
    FieldDef { name: "JCD", mask: 0x0002 }, // include/uapi/linux/mii.h:56
    FieldDef { name: "LSTATUS", mask: 0x0004 }, // include/uapi/linux/mii.h:57
    FieldDef { name: "ANEGCAPABLE", mask: 0x0008 }, // include/uapi/linux/mii.h:58
    FieldDef { name: "RFAULT", mask: 0x0010 }, // include/uapi/linux/mii.h:59
    FieldDef { name: "ANEGCOMPLETE", mask: 0x0020 }, // include/uapi/linux/mii.h:60
    FieldDef { name: "ESTATEN", mask: 0x0100 }, // include/uapi/linux/mii.h:62
    FieldDef { name: "100HALF2", mask: 0x0200 }, // include/uapi/linux/mii.h:63
    FieldDef { name: "100FULL2", mask: 0x0400 }, // include/uapi/linux/mii.h:64
    FieldDef { name: "10HALF", mask: 0x0800 }, // include/uapi/linux/mii.h:65
    FieldDef { name: "10FULL", mask: 0x1000 }, // include/uapi/linux/mii.h:66
    FieldDef { name: "100HALF", mask: 0x2000 }, // include/uapi/linux/mii.h:67
    FieldDef { name: "100FULL", mask: 0x4000 }, // include/uapi/linux/mii.h:68
    FieldDef { name: "100BASE4", mask: 0x8000 }, // include/uapi/linux/mii.h:69
];

/// ADVERTISE technology, pause, selector and acknowledgement fields.
pub const ADVERTISE_FIELDS: [FieldDef; 11] = [
    FieldDef { name: "SLCT", mask: 0x001f }, // include/uapi/linux/mii.h:72
    FieldDef { name: "10HALF", mask: 0x0020 }, // include/uapi/linux/mii.h:74
    FieldDef { name: "10FULL", mask: 0x0040 }, // include/uapi/linux/mii.h:76
    FieldDef { name: "100HALF", mask: 0x0080 }, // include/uapi/linux/mii.h:78
    FieldDef { name: "100FULL", mask: 0x0100 }, // include/uapi/linux/mii.h:80
    FieldDef { name: "100BASE4", mask: 0x0200 }, // include/uapi/linux/mii.h:82
    FieldDef { name: "PAUSE_CAP", mask: 0x0400 }, // include/uapi/linux/mii.h:83
    FieldDef { name: "PAUSE_ASYM", mask: 0x0800 }, // include/uapi/linux/mii.h:84
    FieldDef { name: "RFAULT", mask: 0x2000 }, // include/uapi/linux/mii.h:86
    FieldDef { name: "LPACK", mask: 0x4000 }, // include/uapi/linux/mii.h:87
    FieldDef { name: "NPAGE", mask: 0x8000 }, // include/uapi/linux/mii.h:88
];

/// LPA aliases of the same nine wire positions.
pub const LPA_FIELDS: [FieldDef; 11] = [
    FieldDef { name: "SLCT", mask: 0x001f }, // include/uapi/linux/mii.h:96
    FieldDef { name: "10HALF", mask: 0x0020 }, // include/uapi/linux/mii.h:97
    FieldDef { name: "10FULL", mask: 0x0040 }, // include/uapi/linux/mii.h:99
    FieldDef { name: "100HALF", mask: 0x0080 }, // include/uapi/linux/mii.h:101
    FieldDef { name: "100FULL", mask: 0x0100 }, // include/uapi/linux/mii.h:103
    FieldDef { name: "100BASE4", mask: 0x0200 }, // include/uapi/linux/mii.h:105
    FieldDef { name: "PAUSE_CAP", mask: 0x0400 }, // include/uapi/linux/mii.h:106
    FieldDef { name: "PAUSE_ASYM", mask: 0x0800 }, // include/uapi/linux/mii.h:107
    FieldDef { name: "RFAULT", mask: 0x2000 }, // include/uapi/linux/mii.h:109
    FieldDef { name: "LPACK", mask: 0x4000 }, // include/uapi/linux/mii.h:110
    FieldDef { name: "NPAGE", mask: 0x8000 }, // include/uapi/linux/mii.h:111
];
