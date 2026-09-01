// SPDX-License-Identifier: GPL-2.0-only
//! Erase/trim/discard argument encoding and range checks from Linux
//! `drivers/mmc/core/core.c`, with literals from `include/linux/mmc/mmc.h` and
//! `include/linux/mmc/sd.h`.
//!
//! Copyright (C) 2003-2004 Russell King; SD support Copyright (C) 2004 Ian
//! Molton; Copyright (C) 2005-2008 Pierre Ossman; MMCv4 support Copyright (C)
//! 2006 Philip Langdale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EraseArgumentDef { pub name: &'static str, pub value: u32 }

pub const MMC_ERASE_ARG: u32 = 0x0000_0000; // include/linux/mmc/mmc.h:437
pub const MMC_SECURE_ERASE_ARG: u32 = 0x8000_0000; // include/linux/mmc/mmc.h:438
pub const MMC_TRIM_ARG: u32 = 0x0000_0001; // include/linux/mmc/mmc.h:439
pub const MMC_DISCARD_ARG: u32 = 0x0000_0003; // include/linux/mmc/mmc.h:440
pub const MMC_SECURE_TRIM1_ARG: u32 = 0x8000_0001; // include/linux/mmc/mmc.h:441
pub const MMC_SECURE_TRIM2_ARG: u32 = 0x8000_8000; // include/linux/mmc/mmc.h:442
pub const MMC_SECURE_ARGS: u32 = 0x8000_0000; // include/linux/mmc/mmc.h:443
pub const MMC_TRIM_OR_DISCARD_ARGS: u32 = 0x0000_8003; // include/linux/mmc/mmc.h:444
pub const SD_ERASE_ARG: u32 = 0x0000_0000; // include/linux/mmc/sd.h:101
pub const SD_DISCARD_ARG: u32 = 0x0000_0001; // include/linux/mmc/sd.h:102

pub const MMC_ERASE_ARGUMENTS: [EraseArgumentDef; 6] = [
    EraseArgumentDef { name: "MMC_ERASE_ARG", value: MMC_ERASE_ARG }, // include/linux/mmc/mmc.h:437
    EraseArgumentDef { name: "MMC_SECURE_ERASE_ARG", value: MMC_SECURE_ERASE_ARG }, // include/linux/mmc/mmc.h:438
    EraseArgumentDef { name: "MMC_TRIM_ARG", value: MMC_TRIM_ARG }, // include/linux/mmc/mmc.h:439
    EraseArgumentDef { name: "MMC_DISCARD_ARG", value: MMC_DISCARD_ARG }, // include/linux/mmc/mmc.h:440
    EraseArgumentDef { name: "MMC_SECURE_TRIM1_ARG", value: MMC_SECURE_TRIM1_ARG }, // include/linux/mmc/mmc.h:441
    EraseArgumentDef { name: "MMC_SECURE_TRIM2_ARG", value: MMC_SECURE_TRIM2_ARG }, // include/linux/mmc/mmc.h:442
];
pub const SD_ERASE_ARGUMENTS: [EraseArgumentDef; 2] = [
    EraseArgumentDef { name: "SD_ERASE_ARG", value: SD_ERASE_ARG }, // include/linux/mmc/sd.h:101
    EraseArgumentDef { name: "SD_DISCARD_ARG", value: SD_DISCARD_ARG }, // include/linux/mmc/sd.h:102
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcEraseOperation { Erase, SecureErase, Trim, Discard, SecureTrim1, SecureTrim2 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdEraseOperation { Erase, Discard }

/// Encode a JEDEC eMMC CMD38 operation without reproducing response decoding.
pub fn encode_mmc_argument(operation: MmcEraseOperation) -> u32 {
    match operation {
        MmcEraseOperation::Erase => MMC_ERASE_ARG,
        MmcEraseOperation::SecureErase => MMC_SECURE_ERASE_ARG,
        MmcEraseOperation::Trim => MMC_TRIM_ARG,
        MmcEraseOperation::Discard => MMC_DISCARD_ARG,
        MmcEraseOperation::SecureTrim1 => MMC_SECURE_TRIM1_ARG,
        MmcEraseOperation::SecureTrim2 => MMC_SECURE_TRIM2_ARG,
    }
}

/// Encode an SD erase/discard argument.
pub fn encode_sd_argument(operation: SdEraseOperation) -> u32 {
    match operation { SdEraseOperation::Erase => SD_ERASE_ARG, SdEraseOperation::Discard => SD_DISCARD_ARG }
}

/// Linux core.c:1519-1522 deliberately excludes DISCARD from trim arguments.
pub fn is_trim_argument(argument: u32) -> bool {
    argument & MMC_TRIM_OR_DISCARD_ARGS != 0 && argument != MMC_DISCARD_ARG
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraseRangeError {
    ZeroEraseGroupSize { erase_group_size: u32, minimum: u32 },
    SecureEraseStartMisaligned { start_sector: u64, erase_group_size: u32 },
    SecureEraseCountMisaligned { sector_count: u32, erase_group_size: u32 },
    RangeEndOverflow { start_sector: u64, sector_count: u32, maximum_sector: u64 },
}

/// Validate the range arithmetic and secure-erase alignment used at
/// core.c:1825-1839. The returned end is inclusive, as passed to CMD36.
pub fn validate_erase_range(start_sector: u64, sector_count: u32,
                            erase_group_size: u32, argument: u32)
                            -> Result<Option<u64>, EraseRangeError> {
    if erase_group_size == 0 {
        return Err(EraseRangeError::ZeroEraseGroupSize { erase_group_size, minimum: 1 });
    }
    if argument == MMC_SECURE_ERASE_ARG {
        if start_sector % u64::from(erase_group_size) != 0 {
            return Err(EraseRangeError::SecureEraseStartMisaligned { start_sector, erase_group_size });
        }
        if sector_count % erase_group_size != 0 {
            return Err(EraseRangeError::SecureEraseCountMisaligned { sector_count, erase_group_size });
        }
    }
    if sector_count == 0 { return Ok(None); }
    let inclusive_delta = u64::from(sector_count - 1);
    match start_sector.checked_add(inclusive_delta) {
        Some(end) => Ok(Some(end)),
        None => Err(EraseRangeError::RangeEndOverflow {
            start_sector, sector_count, maximum_sector: u64::MAX,
        }),
    }
}
