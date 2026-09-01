// SPDX-License-Identifier: GPL-2.0-only
//! SD Status Register (SSR) allocation-unit, erase, and discard extraction.
//!
//! Ported from Linux `drivers/mmc/core/sd.c` and `include/linux/mmc/sd.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_core_cmd::csd::{extract_bits, ExtractError};

pub const SD_ERASE_ARG: u32 = 0x0000_0000; // include/linux/mmc/sd.h:101
pub const SD_DISCARD_ARG: u32 = 0x0000_0001; // include/linux/mmc/sd.h:102

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsrField {
    pub name: &'static str,
    pub start: u16,
    pub size: u8,
}

pub const SSR_FIELDS: [SsrField; 5] = [
    SsrField {
        name: "AU_SIZE",
        start: 428,
        size: 4,
    }, // drivers/mmc/core/sd.c:291
    SsrField {
        name: "ERASE_SIZE",
        start: 408,
        size: 16,
    }, // drivers/mmc/core/sd.c:295
    SsrField {
        name: "ERASE_TIMEOUT",
        start: 402,
        size: 6,
    }, // drivers/mmc/core/sd.c:296
    SsrField {
        name: "ERASE_OFFSET",
        start: 400,
        size: 2,
    }, // drivers/mmc/core/sd.c:298
    SsrField {
        name: "DISCARD_SUPPORT",
        start: 313,
        size: 1,
    }, // drivers/mmc/core/sd.c:309-312
];

pub const SD_AU_SIZE_SECTORS: [u32; 16] = [
    0, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 24576, 32768, 49152, 65536, 131072,
]; // drivers/mmc/core/sd.c:53-58

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ssr {
    pub au_sectors: u32,
    pub erase_timeout_ms: u32,
    pub erase_offset_ms: u32,
    pub discard_supported: bool,
    pub erase_arg: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SsrError {
    Extract(ExtractError),
    InvalidAllocationUnit {
        value: u8,
        maximum_without_spec3: u8,
    },
}

impl From<ExtractError> for SsrError {
    fn from(value: ExtractError) -> Self {
        Self::Extract(value)
    }
}

/// Decode the fields Linux consumes from the 512-bit host-order SSR.
pub fn decode(raw_ssr: &[u32; 16], sda_spec3: bool, sda_specx: u8) -> Result<Ssr, SsrError> {
    let au = extract_bits(
        &[raw_ssr[0], raw_ssr[1], raw_ssr[2], raw_ssr[3]],
        44, // Linux writes this as `428 - 384` (drivers/mmc/core/sd.c:291)
        4,
    )? as u8; // drivers/mmc/core/sd.c:287-294
    if au > 9 && !sda_spec3 {
        return Err(SsrError::InvalidAllocationUnit {
            value: au,
            maximum_without_spec3: 9,
        });
        // drivers/mmc/core/sd.c:292-305
    }
    let mut erase_timeout_ms = 0;
    let mut erase_offset_ms = 0;
    if au != 0 {
        let first = [raw_ssr[0], raw_ssr[1], raw_ssr[2], raw_ssr[3]];
        let erase_size = extract_bits(&first, 24, 16)?; // `408 - 384`, sd.c:295
        let erase_time = extract_bits(&first, 18, 6)?; // `402 - 384`, sd.c:296
        if erase_size != 0 && erase_time != 0 {
            erase_timeout_ms = erase_time * 1000 / erase_size;
            erase_offset_ms = extract_bits(&first, 16, 2)? * 1000; // `400 - 384`, sd.c:298
                                                                   // drivers/mmc/core/sd.c:295-301
        }
    }
    let discard_supported = extract_bits(&[0, 0, 0, raw_ssr[6]], 25, 1)? != 0;
    // Linux writes the position as `313 - 288`, drivers/mmc/core/sd.c:312
    // drivers/mmc/core/sd.c:309-312
    let erase_arg = if sda_specx != 0 && discard_supported {
        SD_DISCARD_ARG
    } else {
        SD_ERASE_ARG
    }; // drivers/mmc/core/sd.c:313-314
    Ok(Ssr {
        au_sectors: SD_AU_SIZE_SECTORS[usize::from(au)],
        erase_timeout_ms,
        erase_offset_ms,
        discard_supported,
        erase_arg,
    })
}
