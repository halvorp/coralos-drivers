// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for dirty state, sync selection/order, and drop ranges.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original author: Dimitris Papastamos.

use regmap_regcache_core::defaults::RegDefault;
use regmap_regcache_core::sync::{
    drop_region, finish_full_sync, finish_region_sync, mark_dirty, plan_full_sync,
    plan_region_sync, register_needs_sync, CachedRegister, DirtyState, PatchWrite, SyncError,
    SyncWrite, SyncWriteSource, SYNC_WRITE_SOURCE_NAMES,
};

const EMPTY_WRITE: SyncWrite = SyncWrite {
    reg: 0,
    value: 0,
    source: SyncWriteSource::Cache,
};

fn entry(
    reg: u32,
    value: Option<u32>,
    present: bool,
    writeable: bool,
    volatile: bool,
) -> CachedRegister {
    CachedRegister {
        reg,
        value,
        present,
        writeable,
        volatile,
    }
}

/// regcache.c:603-608,432-443,511-525. Full success alone clears dirty; every full/region tail
/// clears no_sync_defaults; region sync deliberately leaves dirty set.
#[test]
fn dirty_tracking_matches_full_failure_success_and_region_paths() {
    let mut state = DirtyState::default();
    mark_dirty(&mut state);
    assert_eq!(
        state,
        DirtyState {
            cache_dirty: true,
            no_sync_defaults: true
        }
    );
    finish_full_sync(&mut state, false);
    assert_eq!(
        state,
        DirtyState {
            cache_dirty: true,
            no_sync_defaults: false
        }
    );
    state.no_sync_defaults = true;
    finish_region_sync(&mut state);
    assert_eq!(
        state,
        DirtyState {
            cache_dirty: true,
            no_sync_defaults: false
        }
    );
    state.no_sync_defaults = true;
    finish_full_sync(&mut state, true);
    assert_eq!(
        state,
        DirtyState {
            cache_dirty: false,
            no_sync_defaults: false
        }
    );
}

/// regcache.c:325-342. Unwriteable never syncs; absent reset knowledge syncs even defaults; after
/// reset, an exact known default is skipped while changed/unknown values sync.
#[test]
fn per_register_sync_decision_matches_linux() {
    let defaults = [RegDefault { reg: 4, def: 0x55 }];
    assert!(!register_needs_sync(false, false, 4, 0x66, &defaults));
    assert!(register_needs_sync(true, false, 4, 0x55, &defaults));
    assert!(!register_needs_sync(true, true, 4, 0x55, &defaults));
    assert!(register_needs_sync(true, true, 4, 0x56, &defaults));
    assert!(register_needs_sync(true, true, 8, 0x55, &defaults));
}

/// regcache.c:417-435. Patches are emitted FIRST in patch-table order, then cache registers in
/// ascending scan order. Volatile, unwriteable, absent, missing, default, and over-max entries skip.
#[test]
fn full_sync_writes_patch_first_then_only_eligible_cache_registers_in_order() {
    let state = DirtyState {
        cache_dirty: true,
        no_sync_defaults: true,
    };
    let patches = [
        PatchWrite {
            reg: 0x30,
            value: 0xa0,
        },
        PatchWrite {
            reg: 0x04,
            value: 0xa1,
        },
    ];
    let defaults = [RegDefault {
        reg: 0x08,
        def: 0x20,
    }];
    // Intentionally not sorted: planning, rather than caller convenience, must enforce Linux's
    // ascending default-loop/RBTREE traversal order (regcache.c:349-375; rbtree.c:477-500).
    let cache = [
        entry(0x18, Some(0x60), true, true, false),
        entry(0x04, Some(0x11), true, true, true),
        entry(0x08, Some(0x20), true, true, false),
        entry(0x0c, Some(0x30), true, false, false),
        entry(0x10, Some(0x40), false, true, false),
        entry(0x14, None, true, true, false),
        entry(0x00, Some(0x10), true, true, false),
        entry(0x1c, Some(0x70), true, true, false),
    ];
    let mut out = [EMPTY_WRITE; 8];
    let used = plan_full_sync(state, &patches, &cache, &defaults, 0x18, &mut out).unwrap();
    assert_eq!(used, 4);
    assert_eq!(
        &out[..used],
        &[
            SyncWrite {
                reg: 0x30,
                value: 0xa0,
                source: SyncWriteSource::Patch
            },
            SyncWrite {
                reg: 0x04,
                value: 0xa1,
                source: SyncWriteSource::Patch
            },
            SyncWrite {
                reg: 0x00,
                value: 0x10,
                source: SyncWriteSource::Cache
            },
            SyncWrite {
                reg: 0x18,
                value: 0x60,
                source: SyncWriteSource::Cache
            },
        ]
    );
}

/// regcache.c:333-335,417-418. Clean means no writes; without known-reset semantics, even a value
/// equal to a listed default is written.
#[test]
fn clean_sync_is_empty_and_ordinary_dirty_sync_writes_defaults() {
    let defaults = [RegDefault { reg: 4, def: 0x55 }];
    let cache = [entry(4, Some(0x55), true, true, false)];
    let mut out = [EMPTY_WRITE; 1];
    assert_eq!(
        plan_full_sync(DirtyState::default(), &[], &cache, &defaults, 8, &mut out),
        Ok(0)
    );
    assert_eq!(
        plan_full_sync(
            DirtyState {
                cache_dirty: true,
                no_sync_defaults: false
            },
            &[],
            &cache,
            &defaults,
            8,
            &mut out
        ),
        Ok(1)
    );
    assert_eq!(
        out[0],
        SyncWrite {
            reg: 4,
            value: 0x55,
            source: SyncWriteSource::Cache
        }
    );
}

/// regcache.c:489-534. Region bounds are inclusive; patches are not part of region sync; write
/// order remains ascending cache order.
#[test]
fn region_sync_is_inclusive_omits_patches_and_preserves_register_order() {
    let cache = [
        entry(0x0c, Some(0x13), true, true, false),
        entry(0x08, Some(0x12), true, true, false),
        entry(0x00, Some(0x10), true, true, false),
        entry(0x04, Some(0x11), true, true, false),
    ];
    let mut out = [EMPTY_WRITE; 4];
    let used = plan_region_sync(
        DirtyState {
            cache_dirty: true,
            no_sync_defaults: false,
        },
        0x04,
        0x08,
        &cache,
        &[],
        &mut out,
    )
    .unwrap();
    assert_eq!(used, 2);
    assert_eq!(
        &out[..used],
        &[
            SyncWrite {
                reg: 0x04,
                value: 0x11,
                source: SyncWriteSource::Cache
            },
            SyncWrite {
                reg: 0x08,
                value: 0x12,
                source: SyncWriteSource::Cache
            },
        ]
    );
}

/// regcache-flat.c:146-155 and regcache-rbtree.c:510-540. Drop is inclusive and clears presence,
/// not values outside the requested range.
#[test]
fn drop_region_clears_only_the_inclusive_range() {
    let cache = [
        entry(0, Some(1), true, true, false),
        entry(4, Some(2), true, true, false),
        entry(8, Some(3), false, true, false),
        entry(12, Some(4), true, true, false),
    ];
    let mut present = [false; 4];
    assert_eq!(drop_region(4, 8, &cache, &mut present), Ok(4));
    assert_eq!(present, [true, false, false, true]);
    assert_eq!(
        drop_region(8, 4, &cache, &mut present),
        Err(SyncError::RangeReversed { min: 8, max: 4 })
    );
}

/// regcache.c:420-435. Both source-family members are driven through real planning above; names
/// and count are frozen independently here.
#[test]
fn both_sync_write_sources_are_pinned_by_count_and_name() {
    let expected = ["patch", "cache"];
    assert_eq!(SYNC_WRITE_SOURCE_NAMES.len(), 2);
    assert_eq!(SYNC_WRITE_SOURCE_NAMES, expected);
}

/// Safe no-allocation API names insufficient output capacity and its exact required bound.
#[test]
fn sync_output_refusal_names_capacity_and_required_count() {
    let cache = [entry(0, Some(1), true, true, false)];
    assert_eq!(
        plan_full_sync(
            DirtyState {
                cache_dirty: true,
                no_sync_defaults: false
            },
            &[PatchWrite { reg: 4, value: 2 }],
            &cache,
            &[],
            4,
            &mut [EMPTY_WRITE; 1],
        ),
        Err(SyncError::OutputTooSmall {
            supplied: 1,
            required: 2
        })
    );
}
