// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux regmap cache state and sync ordering.
//!
//! Ported from `drivers/base/regmap/regmap.c` and `drivers/base/regmap/regcache.c`.
//! Copyright 2011 Wolfson Microelectronics plc. Original authors: Mark Brown and Dimitris Papastamos.

use regmap_cache_core::cache::{
    begin_bypassed_read, finish_sync, mark_dirty, read_action, register_needs_sync,
    set_cache_bypass, set_cache_only, sync_plan, sync_register, write_action, CacheError, CacheState,
    ReadAction, SyncStep, WriteAction, SYNC_STEP_NAMES,
};

/// regcache.c:411-471. Names and count are literal, never generated from production.
#[test]
fn all_six_dirty_sync_phases_are_pinned_by_count_name_and_order() {
    let expected_names = [
        "apply-patch-bypassed",
        "sync-cache",
        "restore-bypass",
        "clear-no-sync-defaults",
        "resync-page-selectors",
        "complete-async",
    ];
    assert_eq!(SYNC_STEP_NAMES.len(), 6);
    assert_eq!(SYNC_STEP_NAMES, expected_names);
    let expected_steps = [
        Some(SyncStep::ApplyPatchBypassed),
        Some(SyncStep::SyncCache),
        Some(SyncStep::RestoreBypass),
        Some(SyncStep::ClearNoSyncDefaults),
        Some(SyncStep::ResyncPageSelectors),
        Some(SyncStep::CompleteAsync),
    ];
    assert_eq!(sync_plan(true), (expected_steps, 6));
}

/// regcache.c:417-475: clean skips patch/cache writes but still restores state, repairs selectors,
/// unlocks/completes async in the same tail order.
#[test]
fn clean_sync_keeps_the_nonwriting_tail_in_order() {
    let (steps, count) = sync_plan(false);
    assert_eq!(count, 4);
    assert_eq!(&steps[..4], &[
        Some(SyncStep::RestoreBypass),
        Some(SyncStep::ClearNoSyncDefaults),
        Some(SyncStep::ResyncPageSelectors),
        Some(SyncStep::CompleteAsync),
    ]);
}

/// regmap.c:2831-2851: cache hit first, bypass skips it, cache-only miss refuses hardware.
#[test]
fn read_semantics_pin_cache_bypass_and_cache_only() {
    let normal = CacheState::default();
    assert_eq!(read_action(normal, Some(0x55)), Ok(ReadAction::ReturnCached(0x55)));
    assert_eq!(read_action(normal, None), Ok(ReadAction::ReadHardware { populate_cache: true }));
    assert_eq!(read_action(CacheState { cache_bypass: true, ..normal }, Some(0x55)), Ok(ReadAction::ReadHardware { populate_cache: false }));
    assert_eq!(read_action(CacheState { cache_only: true, ..normal }, None), Err(CacheError::ReadRefusedCacheOnlyMiss));
    assert_eq!(read_action(CacheState { cache_only: true, ..normal }, Some(0x55)), Ok(ReadAction::ReturnCached(0x55)));
}

/// regmap.c:1947-1958: normal updates both, only updates cache+dirty, bypass updates hardware only.
#[test]
fn write_semantics_pin_all_three_modes() {
    assert_eq!(write_action(CacheState::default()), WriteAction { update_cache: true, write_hardware: true, mark_dirty: false });
    assert_eq!(write_action(CacheState { cache_only: true, ..CacheState::default() }), WriteAction { update_cache: true, write_hardware: false, mark_dirty: true });
    assert_eq!(write_action(CacheState { cache_bypass: true, ..CacheState::default() }), WriteAction { update_cache: false, write_hardware: true, mark_dirty: false });
}

/// regcache.c:579-630 warns against contradictory modes; safe port names each refusal.
#[test]
fn contradictory_cache_modes_are_named_refusals() {
    let mut bypass = CacheState { cache_bypass: true, ..CacheState::default() };
    assert_eq!(set_cache_only(&mut bypass, true), Err(CacheError::CacheOnlyRefusedWhileBypassing));
    assert!(!bypass.cache_only);
    assert_eq!(set_cache_bypass(&mut bypass, false), Ok(()));
    assert!(!bypass.cache_bypass);
    assert_eq!(set_cache_only(&mut bypass, true), Ok(()));
    assert!(bypass.cache_only);
    assert_eq!(set_cache_bypass(&mut bypass, true), Err(CacheError::BypassRefusedWhileCacheOnly));
    assert!(!bypass.cache_bypass);
    assert_eq!(set_cache_only(&mut bypass, false), Ok(()));
}

/// regcache.c:590-610,436-444: reset dirties and forces defaults; successful sync alone clears dirty,
/// while every sync attempt clears no_sync_defaults.
#[test]
fn dirty_tracking_matches_linux_success_and_failure_paths() {
    let mut state = CacheState::default();
    mark_dirty(&mut state);
    assert!(state.dirty);
    assert!(state.no_sync_defaults);
    finish_sync(&mut state, false);
    assert!(state.dirty, "failed sync remains dirty");
    assert!(!state.no_sync_defaults, "sync tail always clears this flag");
    state.no_sync_defaults = true;
    finish_sync(&mut state, true);
    assert!(!state.dirty);
    assert!(!state.no_sync_defaults);
}

/// regcache.c:323-339: unwritable never syncs; absent reset knowledge syncs everything; after a
/// known reset, a known hardware default is skipped and every other value is written.
#[test]
fn per_register_sync_predicate_is_pinned_both_ways() {
    assert!(!register_needs_sync(false, false, 7, None));
    assert!(register_needs_sync(true, false, 7, Some(7)));
    assert!(!register_needs_sync(true, true, 7, Some(7)));
    assert!(register_needs_sync(true, true, 8, Some(7)));
    assert!(register_needs_sync(true, true, 7, None));
}

/// regcache.c:341-382. Assert BOTH directions of volatile sync policy: volatile is skipped, normal
/// is emitted. Also pin writeability, cache absence, and reset-default suppression.
#[test]
fn sync_register_excludes_volatile_and_includes_normal() {
    assert_eq!(sync_register(true, true, Some(0x55), false, None), None);
    assert_eq!(sync_register(false, true, Some(0x55), false, None), Some(0x55));
    assert_eq!(sync_register(false, false, Some(0x55), false, None), None);
    assert_eq!(sync_register(false, true, None, false, None), None);
    assert_eq!(sync_register(false, true, Some(0x55), true, Some(0x55)), None);
    assert_eq!(sync_register(false, true, Some(0x56), true, Some(0x55)), Some(0x56));
}

/// regmap.c:2898-2913: bypassed read temporarily forces bypass=true and cache_only=false, then the
/// saved state permits exact restoration.
#[test]
fn bypassed_read_state_is_temporary_and_restorable() {
    let original = CacheState { cache_only: true, cache_bypass: false, dirty: true, no_sync_defaults: true };
    let (temporary, saved) = begin_bypassed_read(original);
    assert_eq!(temporary, CacheState { cache_only: false, cache_bypass: true, dirty: true, no_sync_defaults: true });
    assert_eq!(saved, original);
}
