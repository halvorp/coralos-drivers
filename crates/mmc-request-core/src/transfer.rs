// SPDX-License-Identifier: GPL-2.0-only
//! Block-count and transfer-size checks from Linux
//! `drivers/mmc/core/core.c` (`mmc_mrq_prep`).
//!
//! Copyright (C) 2003-2004 Russell King; SD support Copyright (C) 2004 Ian
//! Molton; Copyright (C) 2005-2008 Pierre Ossman; MMCv4 support Copyright (C)
//! 2006 Philip Langdale.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferLimits {
    pub max_block_size: u32,
    pub max_block_count: u32,
    pub max_request_size: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferError {
    BlockSizeExceedsMaximum { block_size: u32, maximum: u32 },
    BlockCountExceedsMaximum { block_count: u32, maximum: u32 },
    TransferSizeOverflow { block_size: u32, block_count: u32, maximum: u32 },
    TransferSizeExceedsMaximum { transfer_size: u64, maximum: u32 },
    ScatterlistSizeMismatch { scatterlist_size: u64, transfer_size: u64 },
}

/// Validate the same three host bounds and exact scatterlist total as Linux
/// core.c:312-320. Multiplication is widened so a wrapped request is refused.
pub fn validate_transfer(block_size: u32, block_count: u32,
                         scatterlist_size: u64, limits: TransferLimits)
                         -> Result<u64, TransferError> {
    if block_size > limits.max_block_size {
        return Err(TransferError::BlockSizeExceedsMaximum {
            block_size, maximum: limits.max_block_size,
        });
    }
    if block_count > limits.max_block_count {
        return Err(TransferError::BlockCountExceedsMaximum {
            block_count, maximum: limits.max_block_count,
        });
    }
    let transfer_size = u64::from(block_size) * u64::from(block_count);
    if transfer_size > u64::from(u32::MAX) {
        return Err(TransferError::TransferSizeOverflow {
            block_size, block_count, maximum: u32::MAX,
        });
    }
    if transfer_size > u64::from(limits.max_request_size) {
        return Err(TransferError::TransferSizeExceedsMaximum {
            transfer_size, maximum: limits.max_request_size,
        });
    }
    if scatterlist_size != transfer_size {
        return Err(TransferError::ScatterlistSizeMismatch {
            scatterlist_size, transfer_size,
        });
    }
    Ok(transfer_size)
}
