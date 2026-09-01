// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for FLAT/RBTREE arithmetic. Expected values are Linux literals with FILE and LINE.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use regmap_regcache_core::index::{
    flat_drop_span, flat_entry_count, flat_index, rbtree_block_top, rbtree_index,
    rbtree_range_indices, IndexError, IndexRange,
};

/// internal.h:317-321 and regcache-flat.c:19-22,30-39 — FLAT index is a right shift; allocation
/// includes max_register's slot.
#[test]
fn flat_index_and_entry_count_match_linux_literals() {
    assert_eq!(flat_index(0x24, 2), Ok(9));
    assert_eq!(flat_index(0x27, 2), Ok(9));
    assert_eq!(flat_entry_count(0x24, 2), Ok(10));
    assert_eq!(
        flat_index(0x24, 32),
        Err(IndexError::StrideOrderOutOfRange { order: 32 })
    );
}

/// regcache-flat.c:146-154 — an inclusive 0x08..=0x14 range at stride order 2 becomes bitmap
/// start 2, count 4 (indices 2 through 5).
#[test]
fn flat_drop_converts_inclusive_registers_to_start_and_count() {
    assert_eq!(flat_drop_span(0x08, 0x14, 2), Ok((2, 4)));
    assert_eq!(flat_drop_span(0x10, 0x10, 2), Ok((4, 1)));
    assert_eq!(
        flat_drop_span(0x14, 0x08, 2),
        Err(IndexError::RangeReversed {
            min: 0x14,
            max: 0x08
        })
    );
}

/// regcache-rbtree.c:39-45,244-278 — block top and slot use base plus stride arithmetic.
#[test]
fn rbtree_block_top_and_index_match_linux_literals() {
    assert_eq!(rbtree_block_top(0x20, 5, 4), Ok(0x30));
    assert_eq!(rbtree_index(0x2c, 0x20, 4), Ok(3));
    assert_eq!(rbtree_block_top(0, 0, 4), Err(IndexError::EmptyBlock));
    assert_eq!(
        rbtree_index(0x1c, 0x20, 4),
        Err(IndexError::RegisterBelowBlock {
            reg: 0x1c,
            base: 0x20
        })
    );
    assert_eq!(rbtree_index(0x20, 0x20, 0), Err(IndexError::ZeroStride));
}

/// regcache-rbtree.c:465-500,510-540 — range bounds become half-open block slots. The arithmetic
/// floors an unaligned minimum and includes an unaligned maximum exactly as Linux does.
#[test]
fn rbtree_sync_and_drop_range_indices_match_linux() {
    assert_eq!(
        rbtree_range_indices(0x20, 5, 4, 0x25, 0x2d),
        Ok(Some(IndexRange { start: 1, end: 4 }))
    );
    assert_eq!(
        rbtree_range_indices(0x20, 5, 4, 0, u32::MAX),
        Ok(Some(IndexRange { start: 0, end: 5 }))
    );
    assert_eq!(rbtree_range_indices(0x20, 5, 4, 0, 0x1f), Ok(None));
    assert_eq!(rbtree_range_indices(0x20, 5, 4, 0x31, 0x40), Ok(None));
}
