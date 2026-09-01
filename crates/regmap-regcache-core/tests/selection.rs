// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for cache-type selection. Expected values are Linux literals with FILE and LINE.
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.

use regmap_regcache_core::selection::{
    select_cache_type, validate_selection, CacheBackend, CacheType, SelectionError,
    CACHE_TYPE_NAMES, SELECTABLE_CACHE_NAMES,
};

/// include/linux/regmap.h:69-75. Literal count, names, discriminants, and behavior pin the
/// zero-valued NONE member as distinct from every selectable sibling.
#[test]
fn all_five_cache_types_are_pinned_by_count_name_value_and_selection() {
    let expected_names = ["none", "rbtree", "flat", "maple", "flat-sparse"];
    assert_eq!(CACHE_TYPE_NAMES.len(), 5);
    assert_eq!(CACHE_TYPE_NAMES, expected_names);
    assert_eq!(CacheType::None as u8, 0);
    assert_eq!(CacheType::Rbtree as u8, 1);
    assert_eq!(CacheType::Flat as u8, 2);
    assert_eq!(CacheType::Maple as u8, 3);
    assert_eq!(CacheType::FlatSparse as u8, 4);
    assert_ne!(CacheType::None as u8, CacheType::Rbtree as u8);
    assert_eq!(select_cache_type(0), Ok(None));
    assert_eq!(select_cache_type(1), Ok(Some(CacheBackend::Rbtree)));
    assert_eq!(select_cache_type(2), Ok(Some(CacheBackend::Flat)));
    assert_eq!(select_cache_type(3), Ok(Some(CacheBackend::Maple)));
    assert_eq!(select_cache_type(4), Ok(Some(CacheBackend::FlatSparse)));
    assert_eq!(
        select_cache_type(5),
        Err(SelectionError::CacheTypeUnmatched { cache_type: 5 })
    );
}

/// regcache.c:18-23. Linux's implementation table is FLAT_S, RBTREE, MAPLE, FLAT.
#[test]
fn all_four_selectable_backends_are_pinned_in_linux_table_order() {
    let expected = ["flat-sparse", "rbtree", "maple", "flat"];
    assert_eq!(SELECTABLE_CACHE_NAMES.len(), 4);
    assert_eq!(SELECTABLE_CACHE_NAMES, expected);
}

/// regcache.c:139-171. NONE bypasses validation; other types enforce paired defaults and stride.
#[test]
fn initialization_validation_follows_linux_order_and_names_refusals() {
    assert_eq!(validate_selection(0, 0, &[3], 0), Ok(None));
    assert_eq!(
        validate_selection(2, 4, &[0], 0),
        Err(SelectionError::DefaultsWithoutCount)
    );
    assert_eq!(
        validate_selection(2, 4, &[], 1),
        Err(SelectionError::DefaultsCountWithoutTable)
    );
    assert_eq!(
        validate_selection(2, 0, &[], 0),
        Err(SelectionError::ZeroRegisterStride)
    );
    assert_eq!(
        validate_selection(2, 4, &[0, 6], 2),
        Err(SelectionError::DefaultRegisterMisaligned { reg: 6, stride: 4 })
    );
    assert_eq!(
        validate_selection(2, 4, &[0, 8], 2),
        Ok(Some(CacheBackend::Flat))
    );
}
