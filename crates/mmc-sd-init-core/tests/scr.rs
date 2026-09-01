// SPDX-License-Identifier: GPL-2.0-only
//! SCR vectors from Linux `drivers/mmc/core/sd.c` and `include/linux/mmc/card.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::scr::*;

#[test]
fn every_scr_field_is_pinned_by_name_start_and_size() {
    let expected = [
        ("SCR_STRUCTURE", 60, 4),
        ("SD_SPEC", 56, 4),
        ("DATA_STAT_AFTER_ERASE", 55, 1),
        ("SD_BUS_WIDTHS", 48, 4),
        ("SD_SPEC3", 47, 1),
        ("SD_SPEC4", 42, 1),
        ("SD_SPECX", 38, 4),
        ("CMD_SUPPORT", 32, 4), // sd.c:216-242
    ];
    assert_eq!(SCR_FIELDS.len(), 8);
    assert_eq!(SCR_FIELDS.map(|x| (x.name, x.start, x.size)), expected);
}

#[test]
fn scr_protocol_constants_are_frozen() {
    let expected_versions = [("VER_0", 0), ("VER_1", 1), ("VER_2", 2)]; // sd.h:71-73
    let actual_versions = [
        ("VER_0", SCR_SPEC_VER_0),
        ("VER_1", SCR_SPEC_VER_1),
        ("VER_2", SCR_SPEC_VER_2),
    ];
    assert_eq!(actual_versions.len(), 3);
    assert_eq!(actual_versions, expected_versions);
    let expected_caps = [
        ("BUS_WIDTH_1", 0x01),
        ("BUS_WIDTH_4", 0x04),
        ("CMD20", 0x01),
        ("CMD23", 0x02),
        ("CMD48", 0x04),
        ("CMD58", 0x08),
        // include/linux/mmc/card.h:136-142
    ];
    let actual_caps = [
        ("BUS_WIDTH_1", SD_SCR_BUS_WIDTH_1),
        ("BUS_WIDTH_4", SD_SCR_BUS_WIDTH_4),
        ("CMD20", SD_SCR_CMD20_SUPPORT),
        ("CMD23", SD_SCR_CMD23_SUPPORT),
        ("CMD48", SD_SCR_CMD48_SUPPORT),
        ("CMD58", SD_SCR_CMD58_SUPPORT),
    ];
    assert_eq!(actual_caps.len(), 6);
    assert_eq!(actual_caps, expected_caps);
}

#[test]
fn version_four_scr_extracts_all_linux_fields() {
    // Literal SCR: structure=0, SD_SPEC=2, erased=1, widths=1+4, SPEC3=1, SPEC4=1,
    // SPECX=2 and all four command-support bits. Field positions are sd.c:216-242.
    let scr = decode(&[0x0285_848f, 0]).unwrap();
    assert_eq!(scr.sda_vsn, 2);
    assert!(scr.sda_spec3);
    assert!(scr.sda_spec4);
    assert_eq!(scr.sda_specx, 2);
    assert_eq!(scr.bus_widths, 5);
    assert_eq!(scr.cmds, 15);
    assert_eq!(scr.erased_byte, 0xff);
}

#[test]
fn scr_refusals_name_structure_and_missing_widths() {
    assert_eq!(
        decode(&[0x1000_0000, 0]),
        Err(ScrError::UnrecognisedScrStructure {
            value: 1,
            expected: 0
        })
    );
    assert_eq!(
        decode(&[0x0201_0000, 0]),
        Err(ScrError::MissingMandatoryBusWidths {
            offered: 1,
            required: 5
        })
    ); // sd.c:244-249
}
