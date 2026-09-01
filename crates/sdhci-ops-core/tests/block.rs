// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for Linux `SDHCI_MAKE_BLKSZ` and block count setup. Ported from
//! `drivers/mmc/host/sdhci.c:1085-:1115` and `drivers/mmc/host/sdhci.h:30-:33,350-:354`.
//! Original copyright (C) 2005-2008 Pierre Ossman and Linux SDHCI authors.

use sdhci_ops_core::block::{
    decode_block_count, decode_block_size, decode_sdma_boundary, encode_block_registers,
    encode_sdma_boundary, BlockError, BlockRegisters, SDMA_DEFAULT_BOUNDARY_ARG,
    SDMA_DEFAULT_BOUNDARY_BYTES,
};

/// sdhci.h:31 — `((dma & 0x7) << 12) | (blksz & 0xFFF)`. Linux initializes the boundary argument
/// to 7 at sdhci.c:4087, from 512 KiB at sdhci.h:353-:354.
#[test]
fn emmc_block_words_pin_default_sdma_boundary_by_value_and_round_trip() {
    let registers = encode_block_registers(7, 512, 8).unwrap();
    assert_eq!(registers, BlockRegisters { block_size: 0x7200, block_count: 0x0008 });
    assert_eq!(decode_block_size(0x7200), (7, 512));
    assert_eq!(decode_block_count(0x0008), 8);
    assert_eq!(SDMA_DEFAULT_BOUNDARY_ARG, 7);
    assert_eq!(SDMA_DEFAULT_BOUNDARY_BYTES, 524_288);
    assert_eq!(encode_sdma_boundary(524_288), Ok(7));
    assert_eq!(decode_sdma_boundary(7), Ok(524_288));
}

/// sdhci.h:350-:354 defines eight valid 4K..512K powers of two. This expected list is literal and
/// not generated from production values: both each encoded value and its decode are pinned.
#[test]
fn all_eight_sdma_boundaries_are_pinned_and_round_trip() {
    let expected = [
        (4_096, 0),
        (8_192, 1),
        (16_384, 2),
        (32_768, 3),
        (65_536, 4),
        (131_072, 5),
        (262_144, 6),
        (524_288, 7),
    ];
    assert_eq!(expected.len(), 8);
    for (bytes, argument) in expected {
        assert_eq!(encode_sdma_boundary(bytes), Ok(argument));
        assert_eq!(decode_sdma_boundary(argument), Ok(bytes));
    }
}

#[test]
fn out_of_range_fields_name_the_value_and_bound_instead_of_masking() {
    assert_eq!(
        encode_block_registers(7, 4096, 1),
        Err(BlockError::BlockSizeExceedsField { value: 4096, max: 4095 })
    );
    assert_eq!(
        encode_block_registers(7, 512, 65_536),
        Err(BlockError::BlockCountExceedsRegister { value: 65_536, max: 65_535 })
    );
    assert_eq!(
        encode_block_registers(7, 512, 1_025),
        Err(BlockError::TransferBytesExceedLinuxLimit {
            value: 524_800,
            max: 524_288,
        })
    ); // sdhci.c:1088
    assert_eq!(
        encode_block_registers(8, 512, 1),
        Err(BlockError::SdmaBoundaryAboveMaximum { value: 8, max: 7 })
    );
    assert_eq!(
        encode_sdma_boundary(6_144),
        Err(BlockError::SdmaBoundaryNotPowerOfTwo { value: 6_144 })
    );
    assert_eq!(
        encode_sdma_boundary(2_048),
        Err(BlockError::SdmaBoundaryBelowMinimum { value: 2_048, min: 4_096 })
    );
}
