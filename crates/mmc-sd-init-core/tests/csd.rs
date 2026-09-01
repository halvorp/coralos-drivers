// SPDX-License-Identifier: GPL-2.0-only
//! Frozen SD CSD capacity vectors from Linux `drivers/mmc/core/sd.c`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::csd::*;

#[test]
fn every_capacity_field_is_pinned_by_name_start_and_size() {
    let expected = [
        ("CSD_STRUCTURE", 126, 2), // sd.c:113
        ("READ_BL_LEN", 80, 4),    // sd.c:131
        ("V1_C_SIZE", 62, 12),     // sd.c:128
        ("V1_C_SIZE_MULT", 47, 3), // sd.c:127
        ("V2_C_SIZE", 48, 22),     // sd.c:168-170
    ];
    assert_eq!(CSD_CAPACITY_FIELDS.len(), 5);
    assert_eq!(
        CSD_CAPACITY_FIELDS.map(|x| (x.name, x.start, x.size)),
        expected
    );
}

#[test]
fn real_spec_style_csd_v1_uses_block_length_and_multiplier() {
    // Physical Layer Simplified Specification CSD v1 register example, represented in Linux R2
    // word order. Linux formula: (3867 + 1) << (7 + 2) blocks, block length 2^9 bytes.
    let raw = [0x0026_0032, 0x5f59_83c6, 0xdbdf_ff92, 0x8040_00aa];
    let capacity = decode_capacity(&raw).unwrap();
    assert_eq!(capacity.csd_structure, 0);
    assert_eq!(capacity.sectors, 1_980_416);
    assert_eq!(capacity.bytes, 1_013_972_992);
}

#[test]
fn real_spec_style_csd_v2_uses_fixed_512_kib_units() {
    // Physical Layer Simplified Specification CSD v2 register example, represented in Linux R2
    // word order. Linux formula: (15159 + 1) << 10 sectors, exactly 512 bytes per sector.
    let raw = [0x400e_0032, 0x5b59_0000, 0x3b37_7f80, 0x0a40_0000];
    let capacity = decode_capacity(&raw).unwrap();
    assert_eq!(capacity.csd_structure, 1);
    assert_eq!(capacity.sectors, 15_523_840);
    assert_eq!(capacity.bytes, 7_948_206_080);
}

#[test]
fn unsupported_csd_version_names_what_refused_and_the_bound() {
    let raw = [0x8000_0000, 0, 0, 0];
    assert_eq!(
        decode_capacity(&raw),
        Err(CapacityError::UnrecognisedCsdStructure {
            value: 2,
            maximum_supported: 1
        })
    );
}
