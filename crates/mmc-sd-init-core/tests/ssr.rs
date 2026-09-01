// SPDX-License-Identifier: GPL-2.0-only
//! SSR vectors from Linux `drivers/mmc/core/sd.c` and `include/linux/mmc/sd.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::ssr::*;

#[test]
fn every_ssr_field_is_pinned_by_name_start_and_size() {
    let expected = [
        ("AU_SIZE", 428, 4),
        ("ERASE_SIZE", 408, 16),
        ("ERASE_TIMEOUT", 402, 6),
        ("ERASE_OFFSET", 400, 2),
        ("DISCARD_SUPPORT", 313, 1), // sd.c:291-312
    ];
    assert_eq!(SSR_FIELDS.len(), 5);
    assert_eq!(SSR_FIELDS.map(|x| (x.name, x.start, x.size)), expected);
}

#[test]
fn every_linux_allocation_unit_literal_is_frozen() {
    // sd.c:53-58, all values already divided by Linux's literal 512-byte sector size.
    let expected = [
        0, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 24576, 32768, 49152, 65536, 131072,
    ];
    assert_eq!(SD_AU_SIZE_SECTORS.len(), 16);
    assert_eq!(SD_AU_SIZE_SECTORS, expected);
}

#[test]
fn ssr_extracts_all_fields_and_uses_linux_erase_arithmetic() {
    let mut raw = [0u32; 16];
    // AU=10, ES=4, ET=2, EO=3 at sd.c:291-300.
    raw[2] = 0x0000_a000;
    raw[3] = 0x040b_0000;
    raw[6] = 0x0200_0000; // discard-support bit 313, sd.c:309-312.
    let ssr = decode(&raw, true, 1).unwrap();
    assert_eq!(ssr.au_sectors, 16_384);
    assert_eq!(ssr.erase_timeout_ms, 500);
    assert_eq!(ssr.erase_offset_ms, 3_000);
    assert!(ssr.discard_supported);
    assert_eq!(ssr.erase_arg, 0x0000_0001); // include/linux/mmc/sd.h:102
}

#[test]
fn discard_requires_both_new_scr_and_ssr_support() {
    let mut raw = [0u32; 16];
    raw[6] = 0x0200_0000;
    assert_eq!(decode(&raw, false, 0).unwrap().erase_arg, 0x0000_0000);
    assert_eq!(decode(&raw, false, 1).unwrap().erase_arg, 0x0000_0001);
}

#[test]
fn old_scr_refuses_new_allocation_unit_by_name_and_bound() {
    let mut raw = [0u32; 16];
    raw[2] = 0x0000_a000;
    assert_eq!(
        decode(&raw, false, 0),
        Err(SsrError::InvalidAllocationUnit {
            value: 10,
            maximum_without_spec3: 9
        })
    ); // sd.c:292-305
}
