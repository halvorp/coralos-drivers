// SPDX-License-Identifier: GPL-2.0-only
//! Frozen field expectation lists from Linux `include/uapi/linux/mii.h`.
//! Copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.

use mii_phy_core::fields::*;

fn assert_fields(got: &[FieldDef], expected: &[(&str, u16)]) {
    assert_eq!(got.len(), expected.len());
    for (field, &(name, mask)) in got.iter().zip(expected) {
        assert_eq!(field.name, name);
        assert_eq!(field.mask, mask, "{name}");
    }
}

#[test]
fn bmcr_field_count_names_and_masks_are_pinned() {
    // include/uapi/linux/mii.h:42-52.
    assert_fields(&BMCR_FIELDS, &[
        ("RESET", 0x8000),
        ("LOOPBACK", 0x4000),
        ("SPEED100", 0x2000),
        ("ANENABLE", 0x1000),
        ("PDOWN", 0x0800),
        ("ISOLATE", 0x0400),
        ("ANRESTART", 0x0200),
        ("FULLDPLX", 0x0100),
        ("CTST", 0x0080),
        ("SPEED1000", 0x0040),
    ]);
}

#[test]
fn bmsr_field_count_names_and_masks_are_pinned() {
    // include/uapi/linux/mii.h:57-69.
    assert_fields(&BMSR_FIELDS, &[
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
    ]);
}

#[test]
fn advertise_field_count_names_and_masks_are_pinned() {
    // include/uapi/linux/mii.h:72-88.
    assert_fields(&ADVERTISE_FIELDS, &[
        ("SLCT", 0x001f),
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
    ]);
}

#[test]
fn lpa_field_count_names_and_masks_are_pinned() {
    // include/uapi/linux/mii.h:96-111.
    assert_fields(&LPA_FIELDS, &[
        ("SLCT", 0x001f),
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
    ]);
}
