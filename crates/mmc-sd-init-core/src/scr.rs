// SPDX-License-Identifier: GPL-2.0-only
//! SD Configuration Register (SCR) extraction and validation.
//!
//! Ported from Linux `drivers/mmc/core/sd.c`, with SCR literals from
//! `include/linux/mmc/card.h` and `include/linux/mmc/sd.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_core_cmd::csd::{extract_bits, ExtractError};

pub const SCR_SPEC_VER_0: u8 = 0; // include/linux/mmc/sd.h:71
pub const SCR_SPEC_VER_1: u8 = 1; // include/linux/mmc/sd.h:72
pub const SCR_SPEC_VER_2: u8 = 2; // include/linux/mmc/sd.h:73
pub const SD_SCR_BUS_WIDTH_1: u8 = 1 << 0; // include/linux/mmc/card.h:136
pub const SD_SCR_BUS_WIDTH_4: u8 = 1 << 2; // include/linux/mmc/card.h:137
pub const SD_SCR_CMD20_SUPPORT: u8 = 1 << 0; // include/linux/mmc/card.h:139
pub const SD_SCR_CMD23_SUPPORT: u8 = 1 << 1; // include/linux/mmc/card.h:140
pub const SD_SCR_CMD48_SUPPORT: u8 = 1 << 2; // include/linux/mmc/card.h:141
pub const SD_SCR_CMD58_SUPPORT: u8 = 1 << 3; // include/linux/mmc/card.h:142

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrField {
    pub name: &'static str,
    pub start: u8,
    pub size: u8,
}

pub const SCR_FIELDS: [ScrField; 8] = [
    ScrField {
        name: "SCR_STRUCTURE",
        start: 60,
        size: 4,
    }, // drivers/mmc/core/sd.c:216
    ScrField {
        name: "SD_SPEC",
        start: 56,
        size: 4,
    }, // drivers/mmc/core/sd.c:223
    ScrField {
        name: "DATA_STAT_AFTER_ERASE",
        start: 55,
        size: 1,
    }, // drivers/mmc/core/sd.c:234
    ScrField {
        name: "SD_BUS_WIDTHS",
        start: 48,
        size: 4,
    }, // drivers/mmc/core/sd.c:224
    ScrField {
        name: "SD_SPEC3",
        start: 47,
        size: 1,
    }, // drivers/mmc/core/sd.c:225-227
    ScrField {
        name: "SD_SPEC4",
        start: 42,
        size: 1,
    }, // drivers/mmc/core/sd.c:229-230
    ScrField {
        name: "SD_SPECX",
        start: 38,
        size: 4,
    }, // drivers/mmc/core/sd.c:229-231
    ScrField {
        name: "CMD_SUPPORT",
        start: 32,
        size: 4,
    }, // drivers/mmc/core/sd.c:239-242
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scr {
    pub sda_vsn: u8,
    pub sda_spec3: bool,
    pub sda_spec4: bool,
    pub sda_specx: u8,
    pub bus_widths: u8,
    pub cmds: u8,
    pub erased_byte: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrError {
    Extract(ExtractError),
    UnrecognisedScrStructure { value: u8, expected: u8 },
    MissingMandatoryBusWidths { offered: u8, required: u8 },
}

impl From<ExtractError> for ScrError {
    fn from(value: ExtractError) -> Self {
        Self::Extract(value)
    }
}

/// Decode Linux's two host-order SCR words without duplicating R2 extraction.
pub fn decode(raw_scr: &[u32; 2]) -> Result<Scr, ScrError> {
    let resp = [0, 0, raw_scr[0], raw_scr[1]]; // drivers/mmc/core/sd.c:211-215
    let structure = extract_bits(&resp, 60, 4)? as u8;
    if structure != 0 {
        return Err(ScrError::UnrecognisedScrStructure {
            value: structure,
            expected: 0,
        });
        // drivers/mmc/core/sd.c:216-221
    }
    let sda_vsn = extract_bits(&resp, 56, 4)? as u8; // drivers/mmc/core/sd.c:223
    let bus_widths = extract_bits(&resp, 48, 4)? as u8; // drivers/mmc/core/sd.c:224
    let sda_spec3 = sda_vsn == SCR_SPEC_VER_2 && extract_bits(&resp, 47, 1)? != 0;
    // drivers/mmc/core/sd.c:225-227
    let sda_spec4 = sda_spec3 && extract_bits(&resp, 42, 1)? != 0; // drivers/mmc/core/sd.c:229-230
    let sda_specx = if sda_spec3 {
        extract_bits(&resp, 38, 4)? as u8
    } else {
        0
    };
    // drivers/mmc/core/sd.c:229-232
    let erased_byte = if extract_bits(&resp, 55, 1)? != 0 {
        0xff
    } else {
        0x00
    };
    // drivers/mmc/core/sd.c:234-237
    let cmds = if sda_spec4 {
        extract_bits(&resp, 32, 4)? as u8
    } else if sda_spec3 {
        extract_bits(&resp, 32, 2)? as u8
    } else {
        0
    }; // drivers/mmc/core/sd.c:239-242
    let required = SD_SCR_BUS_WIDTH_1 | SD_SCR_BUS_WIDTH_4;
    if bus_widths & required != required {
        return Err(ScrError::MissingMandatoryBusWidths {
            offered: bus_widths,
            required,
        });
        // drivers/mmc/core/sd.c:244-249
    }
    Ok(Scr {
        sda_vsn,
        sda_spec3,
        sda_spec4,
        sda_specx,
        bus_widths,
        cmds,
        erased_byte,
    })
}
