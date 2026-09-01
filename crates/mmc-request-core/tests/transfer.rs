// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for transfer bounds from Linux `drivers/mmc/core/core.c`.
//! Copyright (C) 2003-2004 Russell King; 2005-2008 Pierre Ossman.

use mmc_request_core::transfer::*;

const LIMITS: TransferLimits = TransferLimits {
    max_block_size: 512,
    max_block_count: 8,
    max_request_size: 4096,
};

#[test]
fn valid_transfer_returns_the_literal_byte_count() {
    assert_eq!(validate_transfer(512, 8, 4096, LIMITS), Ok(4096)); // drivers/mmc/core/core.c:312-320
    assert_eq!(validate_transfer(1, 1, 1, LIMITS), Ok(1));
}

#[test]
fn each_linux_host_bound_is_a_named_refusal() {
    assert_eq!(validate_transfer(513, 1, 513, LIMITS), Err(TransferError::BlockSizeExceedsMaximum { block_size: 513, maximum: 512 })); // core.c:312
    assert_eq!(validate_transfer(512, 9, 4608, LIMITS), Err(TransferError::BlockCountExceedsMaximum { block_count: 9, maximum: 8 })); // core.c:313
    let size_limited = TransferLimits { max_block_size: 512, max_block_count: 8, max_request_size: 2048 };
    assert_eq!(validate_transfer(512, 8, 4096, size_limited), Err(TransferError::TransferSizeExceedsMaximum { transfer_size: 4096, maximum: 2048 })); // core.c:314
}

#[test]
fn arithmetic_overflow_and_scatterlist_mismatch_are_named() {
    let huge = TransferLimits { max_block_size: u32::MAX, max_block_count: u32::MAX, max_request_size: u32::MAX };
    assert_eq!(validate_transfer(u32::MAX, 2, 8_589_934_590, huge), Err(TransferError::TransferSizeOverflow { block_size: u32::MAX, block_count: 2, maximum: u32::MAX }));
    assert_eq!(validate_transfer(512, 8, 3584, LIMITS), Err(TransferError::ScatterlistSizeMismatch { scatterlist_size: 3584, transfer_size: 4096 })); // core.c:319-320
}
