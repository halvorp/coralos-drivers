// SPDX-License-Identifier: GPL-2.0-only
//! Frozen erase argument/range vectors from Linux `drivers/mmc/core/core.c` and
//! `include/linux/mmc/{mmc.h,sd.h}`. Copyright (C) 2003-2004 Russell King;
//! 2004 Ian Molton; 2005-2008 Pierre Ossman; 2006 Philip Langdale.

use mmc_request_core::erase::*;

#[test]
fn every_mmc_argument_is_pinned_by_count_name_and_literal() {
    let got: Vec<(&str, u32)> = MMC_ERASE_ARGUMENTS.iter().map(|x| (x.name, x.value)).collect();
    assert_eq!(got.len(), 6);
    assert_eq!(got, [
        ("MMC_ERASE_ARG", 0x0000_0000), // include/linux/mmc/mmc.h:437
        ("MMC_SECURE_ERASE_ARG", 0x8000_0000), // include/linux/mmc/mmc.h:438
        ("MMC_TRIM_ARG", 0x0000_0001), // include/linux/mmc/mmc.h:439
        ("MMC_DISCARD_ARG", 0x0000_0003), // include/linux/mmc/mmc.h:440
        ("MMC_SECURE_TRIM1_ARG", 0x8000_0001), // include/linux/mmc/mmc.h:441
        ("MMC_SECURE_TRIM2_ARG", 0x8000_8000), // include/linux/mmc/mmc.h:442
    ]);
    assert_eq!(MMC_SECURE_ARGS, 0x8000_0000); // include/linux/mmc/mmc.h:443
    assert_eq!(MMC_TRIM_OR_DISCARD_ARGS, 0x0000_8003); // include/linux/mmc/mmc.h:444
}

#[test]
fn every_sd_argument_is_pinned_by_count_name_and_literal() {
    let got: Vec<(&str, u32)> = SD_ERASE_ARGUMENTS.iter().map(|x| (x.name, x.value)).collect();
    assert_eq!(got.len(), 2);
    assert_eq!(got, [
        ("SD_ERASE_ARG", 0x0000_0000), // include/linux/mmc/sd.h:101
        ("SD_DISCARD_ARG", 0x0000_0001), // include/linux/mmc/sd.h:102
    ]);
}

#[test]
fn all_public_argument_encoders_have_literal_vectors() {
    assert_eq!(encode_mmc_argument(MmcEraseOperation::Erase), 0x0000_0000);
    assert_eq!(encode_mmc_argument(MmcEraseOperation::SecureErase), 0x8000_0000);
    assert_eq!(encode_mmc_argument(MmcEraseOperation::Trim), 0x0000_0001);
    assert_eq!(encode_mmc_argument(MmcEraseOperation::Discard), 0x0000_0003);
    assert_eq!(encode_mmc_argument(MmcEraseOperation::SecureTrim1), 0x8000_0001);
    assert_eq!(encode_mmc_argument(MmcEraseOperation::SecureTrim2), 0x8000_8000); // include/linux/mmc/mmc.h:437-442
    assert_eq!(encode_sd_argument(SdEraseOperation::Erase), 0x0000_0000);
    assert_eq!(encode_sd_argument(SdEraseOperation::Discard), 0x0000_0001); // include/linux/mmc/sd.h:101-102
}

#[test]
fn trim_classification_matches_linux_discard_exclusion() {
    assert!(!is_trim_argument(0x0000_0000));
    assert!(is_trim_argument(0x0000_0001));
    assert!(!is_trim_argument(0x0000_0003), "MMC discard is deliberately excluded");
    assert!(is_trim_argument(0x8000_0001));
    assert!(is_trim_argument(0x8000_8000)); // drivers/mmc/core/core.c:1519-1522
}

#[test]
fn erase_range_is_inclusive_and_zero_count_is_empty() {
    assert_eq!(validate_erase_range(100, 8, 4, MMC_ERASE_ARG), Ok(Some(107))); // core.c:1834-1841
    assert_eq!(validate_erase_range(100, 0, 4, MMC_ERASE_ARG), Ok(None)); // core.c:1833-1834
}

#[test]
fn secure_erase_alignment_and_range_overflow_are_named() {
    assert_eq!(validate_erase_range(3, 8, 4, MMC_SECURE_ERASE_ARG), Err(EraseRangeError::SecureEraseStartMisaligned { start_sector: 3, erase_group_size: 4 })); // core.c:1825-1828
    assert_eq!(validate_erase_range(4, 6, 4, MMC_SECURE_ERASE_ARG), Err(EraseRangeError::SecureEraseCountMisaligned { sector_count: 6, erase_group_size: 4 }));
    assert_eq!(validate_erase_range(0, 1, 0, MMC_ERASE_ARG), Err(EraseRangeError::ZeroEraseGroupSize { erase_group_size: 0, minimum: 1 })); // core.c:1811-1812
    assert_eq!(validate_erase_range(u64::MAX, 2, 1, MMC_ERASE_ARG), Err(EraseRangeError::RangeEndOverflow { start_sector: u64::MAX, sector_count: 2, maximum_sector: u64::MAX })); // core.c:1834,1836
}
