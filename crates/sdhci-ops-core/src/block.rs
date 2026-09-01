// SPDX-License-Identifier: GPL-2.0-only
//! BLOCK_SIZE and BLOCK_COUNT register encoding.
//!
//! Ported from Linux `sdhci_set_block_info()` in `drivers/mmc/host/sdhci.c:1097-:1115` and
//! `SDHCI_MAKE_BLKSZ` in `drivers/mmc/host/sdhci.h:30-:33`.
//!
//! Original copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved, and the Linux SDHCI
//! authors.

/// SDMA boundary field mask in `SDHCI_MAKE_BLKSZ` (`drivers/mmc/host/sdhci.h:31`).
pub const SDMA_BOUNDARY_MASK: u8 = 0x7;
/// SDMA boundary field position in `SDHCI_MAKE_BLKSZ` (`drivers/mmc/host/sdhci.h:31`).
pub const SDMA_BOUNDARY_SHIFT: u8 = 12;
/// Block-size field mask in `SDHCI_MAKE_BLKSZ` (`drivers/mmc/host/sdhci.h:31`).
pub const BLOCK_SIZE_MASK: u16 = 0x0fff;
/// Smallest SDMA boundary is 4 KiB (`drivers/mmc/host/sdhci.h:350-:354`).
pub const SDMA_BOUNDARY_MIN_BYTES: u32 = 4 * 1024;
/// Linux's default SDMA boundary is 512 KiB (`drivers/mmc/host/sdhci.h:353`).
pub const SDMA_DEFAULT_BOUNDARY_BYTES: u32 = 512 * 1024;
/// Linux's default boundary argument: `ilog2(512 KiB) - 12 = 7` (`sdhci.h:354`).
pub const SDMA_DEFAULT_BOUNDARY_ARG: u8 = 7;
/// Linux rejects block sizes above the 12-bit register field (`drivers/mmc/host/sdhci.c:1089`).
pub const MAX_BLOCK_SIZE: u16 = BLOCK_SIZE_MASK;
/// Legacy block count is a 16-bit register (`drivers/mmc/host/sdhci.c:1090,1114`).
pub const MAX_BLOCK_COUNT: u32 = 65_535;
/// Linux's data-initialization transfer-size ceiling (`drivers/mmc/host/sdhci.c:1088`).
pub const MAX_TRANSFER_BYTES: u64 = 524_288;

/// Why a BLOCK_SIZE/BLOCK_COUNT word was refused instead of silently truncated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    BlockSizeExceedsField { value: u32, max: u32 },
    BlockCountExceedsRegister { value: u32, max: u32 },
    TransferBytesExceedLinuxLimit { value: u64, max: u64 },
    SdmaBoundaryNotPowerOfTwo { value: u32 },
    SdmaBoundaryBelowMinimum { value: u32, min: u32 },
    SdmaBoundaryAboveMaximum { value: u32, max: u32 },
}

/// BLOCK_SIZE and legacy BLOCK_COUNT register words produced by `sdhci_set_block_info()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockRegisters {
    pub block_size: u16,
    pub block_count: u16,
}

/// Encode Linux's SDMA boundary argument from a byte size.
///
/// The valid values are powers of two from 4 KiB through 512 KiB
/// (`drivers/mmc/host/sdhci.h:350-:354`).
pub const fn encode_sdma_boundary(bytes: u32) -> Result<u8, BlockError> {
    if bytes < SDMA_BOUNDARY_MIN_BYTES {
        return Err(BlockError::SdmaBoundaryBelowMinimum {
            value: bytes,
            min: SDMA_BOUNDARY_MIN_BYTES,
        });
    }
    if bytes > SDMA_DEFAULT_BOUNDARY_BYTES {
        return Err(BlockError::SdmaBoundaryAboveMaximum {
            value: bytes,
            max: SDMA_DEFAULT_BOUNDARY_BYTES,
        });
    }
    if !bytes.is_power_of_two() {
        return Err(BlockError::SdmaBoundaryNotPowerOfTwo { value: bytes });
    }
    Ok((bytes.trailing_zeros() - 12) as u8)
}

/// Decode the three-bit SDMA boundary argument back to bytes.
pub const fn decode_sdma_boundary(argument: u8) -> Result<u32, BlockError> {
    if argument > SDMA_BOUNDARY_MASK {
        return Err(BlockError::SdmaBoundaryAboveMaximum {
            value: argument as u32,
            max: SDMA_BOUNDARY_MASK as u32,
        });
    }
    Ok(SDMA_BOUNDARY_MIN_BYTES << argument)
}

/// Encode BLOCK_SIZE, including the SDMA boundary field, plus legacy BLOCK_COUNT.
///
/// Unlike the C macro's masks, this API names and rejects out-of-range values so a caller cannot
/// silently direct SDMA across the wrong boundary or truncate the transfer length.
pub const fn encode_block_registers(
    sdma_boundary_argument: u8,
    block_size: u32,
    block_count: u32,
) -> Result<BlockRegisters, BlockError> {
    if sdma_boundary_argument > SDMA_BOUNDARY_MASK {
        return Err(BlockError::SdmaBoundaryAboveMaximum {
            value: sdma_boundary_argument as u32,
            max: SDMA_BOUNDARY_MASK as u32,
        });
    }
    if block_size > MAX_BLOCK_SIZE as u32 {
        return Err(BlockError::BlockSizeExceedsField {
            value: block_size,
            max: MAX_BLOCK_SIZE as u32,
        });
    }
    if block_count > MAX_BLOCK_COUNT {
        return Err(BlockError::BlockCountExceedsRegister {
            value: block_count,
            max: MAX_BLOCK_COUNT,
        });
    }
    let transfer_bytes = block_size as u64 * block_count as u64;
    if transfer_bytes > MAX_TRANSFER_BYTES {
        return Err(BlockError::TransferBytesExceedLinuxLimit {
            value: transfer_bytes,
            max: MAX_TRANSFER_BYTES,
        });
    }
    Ok(BlockRegisters {
        block_size: ((sdma_boundary_argument as u16) << SDMA_BOUNDARY_SHIFT)
            | block_size as u16,
        block_count: block_count as u16,
    })
}

/// Decode BLOCK_SIZE into `(sdma_boundary_argument, block_size)`.
pub const fn decode_block_size(word: u16) -> (u8, u16) {
    (
        ((word >> SDMA_BOUNDARY_SHIFT) & SDMA_BOUNDARY_MASK as u16) as u8,
        word & BLOCK_SIZE_MASK,
    )
}

/// Decode the legacy BLOCK_COUNT register.
pub const fn decode_block_count(word: u16) -> u32 {
    word as u32
}
