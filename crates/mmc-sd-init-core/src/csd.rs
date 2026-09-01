// SPDX-License-Identifier: GPL-2.0-only
//! SD CSD v1/v2 capacity decode. The two incompatible formulae remain explicit.
//!
//! Ported from Linux `drivers/mmc/core/sd.c` and its `unstuff_bits` helper in
//! `drivers/mmc/core/mmc_ops.h`. Extraction is reused from `mmc-core-cmd`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_core_cmd::csd::{extract_bits, ExtractError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsdField {
    pub name: &'static str,
    pub start: u8,
    pub size: u8,
}

pub const CSD_CAPACITY_FIELDS: [CsdField; 5] = [
    CsdField {
        name: "CSD_STRUCTURE",
        start: 126,
        size: 2,
    }, // drivers/mmc/core/sd.c:113
    CsdField {
        name: "READ_BL_LEN",
        start: 80,
        size: 4,
    }, // drivers/mmc/core/sd.c:131
    CsdField {
        name: "V1_C_SIZE",
        start: 62,
        size: 12,
    }, // drivers/mmc/core/sd.c:128
    CsdField {
        name: "V1_C_SIZE_MULT",
        start: 47,
        size: 3,
    }, // drivers/mmc/core/sd.c:127
    CsdField {
        name: "V2_C_SIZE",
        start: 48,
        size: 22,
    }, // drivers/mmc/core/sd.c:168-170
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capacity {
    pub csd_structure: u8,
    pub sectors: u64,
    pub bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityError {
    Extract(ExtractError),
    UnrecognisedCsdStructure { value: u8, maximum_supported: u8 },
    CapacityOverflow { c_size: u32, read_blkbits: u8 },
}

impl From<ExtractError> for CapacityError {
    fn from(value: ExtractError) -> Self {
        Self::Extract(value)
    }
}

/// Decode CSD capacity exactly as Linux does, then expose both sectors and bytes.
///
/// CSD v1: `(C_SIZE + 1) << (C_SIZE_MULT + 2)` blocks, each `2^READ_BL_LEN` bytes.
/// CSD v2: `(C_SIZE + 1) << 10` sectors, always 512 bytes each.
pub fn decode_capacity(raw: &[u32; 4]) -> Result<Capacity, CapacityError> {
    let structure = extract_bits(raw, 126, 2)? as u8; // drivers/mmc/core/sd.c:113
    match structure {
        0 => {
            let c_size = extract_bits(raw, 62, 12)?; // drivers/mmc/core/sd.c:128
            let c_size_mult = extract_bits(raw, 47, 3)?; // drivers/mmc/core/sd.c:127
            let read_blkbits = extract_bits(raw, 80, 4)? as u8; // drivers/mmc/core/sd.c:131
            let blocks = u64::from(c_size + 1).checked_shl(c_size_mult + 2).ok_or(
                CapacityError::CapacityOverflow {
                    c_size,
                    read_blkbits,
                },
            )?;
            let bytes = blocks.checked_shl(u32::from(read_blkbits)).ok_or(
                CapacityError::CapacityOverflow {
                    c_size,
                    read_blkbits,
                },
            )?;
            Ok(Capacity {
                csd_structure: structure,
                sectors: bytes / 512,
                bytes,
            })
            // drivers/mmc/core/sd.c:127-131; block.c:2672-2674
        }
        1 => {
            let c_size = extract_bits(raw, 48, 22)?; // drivers/mmc/core/sd.c:168-170
            let sectors = u64::from(c_size + 1) << 10; // drivers/mmc/core/sd.c:172-180
            Ok(Capacity {
                csd_structure: structure,
                sectors,
                bytes: sectors * 512,
            })
        }
        value => Err(CapacityError::UnrecognisedCsdStructure {
            value,
            maximum_supported: 1,
        }), // drivers/mmc/core/sd.c:193-196
    }
}
