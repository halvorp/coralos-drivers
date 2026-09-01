// SPDX-License-Identifier: GPL-2.0-only
//! Cache-only/bypass decisions, dirty tracking, and synchronization ordering.
//!
//! Ported from Linux `drivers/base/regmap/regmap.c` lines 1665-1685, 1947-1958, 2825-2852,
//! 2898-2913 and `drivers/base/regmap/regcache.c` lines 280-475, 568-631.
//!
//! Copyright 2011 Wolfson Microelectronics plc. Original authors: Mark Brown and Dimitris Papastamos.

use core::fmt;

/// Named operation refusals corresponding to Linux's `-EBUSY` and WARNed contradictory modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheError {
    ReadRefusedCacheOnlyMiss,
    CacheOnlyRefusedWhileBypassing,
    BypassRefusedWhileCacheOnly,
}

impl fmt::Display for CacheError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadRefusedCacheOnlyMiss => f.write_str("regmap read refused: cache-only mode had no cached value and hardware access is forbidden"),
            Self::CacheOnlyRefusedWhileBypassing => f.write_str("cache-only mode refused: cache bypass is already enabled"),
            Self::BypassRefusedWhileCacheOnly => f.write_str("cache bypass refused: cache-only mode is already enabled"),
        }
    }
}

/// Cache mode and dirty flags represented without storage or I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CacheState {
    pub cache_only: bool,
    pub cache_bypass: bool,
    pub dirty: bool,
    pub no_sync_defaults: bool,
}

/// Read source selected by Linux `_regmap_read` (regmap.c:2825-2852).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadAction {
    ReturnCached(u32),
    ReadHardware { populate_cache: bool },
}

/// Write effects selected by Linux `_regmap_write` (regmap.c:1947-1958).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteAction {
    pub update_cache: bool,
    pub write_hardware: bool,
    pub mark_dirty: bool,
}

/// Ordered sync phases from Linux `regcache_sync` (regcache.c:386-475).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncStep {
    ApplyPatchBypassed,
    SyncCache,
    RestoreBypass,
    ClearNoSyncDefaults,
    ResyncPageSelectors,
    CompleteAsync,
}

/// Frozen names for every ordered sync phase in Linux `regcache_sync` (regcache.c:411-471).
pub const SYNC_STEP_NAMES: [&str; 6] = [
    "apply-patch-bypassed",
    "sync-cache",
    "restore-bypass",
    "clear-no-sync-defaults",
    "resync-page-selectors",
    "complete-async",
];

/// Full dirty-sync order. The first two phases are omitted when clean (regcache.c:417-438), while
/// bypass restoration, flag clearing, paging repair, and async completion still occur.
pub fn sync_plan(dirty: bool) -> ([Option<SyncStep>; 6], usize) {
    if dirty {
        ([
            Some(SyncStep::ApplyPatchBypassed),
            Some(SyncStep::SyncCache),
            Some(SyncStep::RestoreBypass),
            Some(SyncStep::ClearNoSyncDefaults),
            Some(SyncStep::ResyncPageSelectors),
            Some(SyncStep::CompleteAsync),
        ], 6)
    } else {
        ([
            Some(SyncStep::RestoreBypass),
            Some(SyncStep::ClearNoSyncDefaults),
            Some(SyncStep::ResyncPageSelectors),
            Some(SyncStep::CompleteAsync),
            None,
            None,
        ], 4)
    }
}

/// Sync one cached register only when it is nonvolatile, writeable, present, and not a skippable
/// known default (regcache.c:341-382). This pins both sides of Linux's volatile sync exclusion.
pub fn sync_register(
    is_volatile: bool,
    writeable: bool,
    cached_value: Option<u32>,
    no_sync_defaults: bool,
    known_default: Option<u32>,
) -> Option<u32> {
    if is_volatile || !writeable {
        return None;
    }
    let value = cached_value?;
    register_needs_sync(writeable, no_sync_defaults, value, known_default).then_some(value)
}

/// Select a read source. Bypass ignores cache; a miss in cache-only refuses hardware access
/// (regmap.c:2831-2851).
pub fn read_action(state: CacheState, cached: Option<u32>) -> Result<ReadAction, CacheError> {
    if !state.cache_bypass {
        if let Some(value) = cached { return Ok(ReadAction::ReturnCached(value)); }
    }
    if state.cache_only { return Err(CacheError::ReadRefusedCacheOnlyMiss); }
    Ok(ReadAction::ReadHardware { populate_cache: !state.cache_bypass })
}

/// Select write effects. Bypass writes hardware only; cache-only writes cache, marks dirty, and
/// suppresses hardware (regmap.c:1947-1958; raw equivalent at 1665-1685).
pub fn write_action(state: CacheState) -> WriteAction {
    if state.cache_bypass {
        WriteAction { update_cache: false, write_hardware: true, mark_dirty: false }
    } else if state.cache_only {
        WriteAction { update_cache: true, write_hardware: false, mark_dirty: true }
    } else {
        WriteAction { update_cache: true, write_hardware: true, mark_dirty: false }
    }
}

/// Enter/leave cache-only mode, naming Linux's warned contradictory state (regcache.c:579-587).
pub fn set_cache_only(state: &mut CacheState, enable: bool) -> Result<(), CacheError> {
    if enable && state.cache_bypass { return Err(CacheError::CacheOnlyRefusedWhileBypassing); }
    state.cache_only = enable;
    Ok(())
}

/// Enter/leave bypass mode, naming Linux's warned contradictory state (regcache.c:623-630).
pub fn set_cache_bypass(state: &mut CacheState, enable: bool) -> Result<(), CacheError> {
    if enable && state.cache_only { return Err(CacheError::BypassRefusedWhileCacheOnly); }
    state.cache_bypass = enable;
    Ok(())
}

/// Linux `regcache_mark_dirty`: dirty plus force syncing defaults (regcache.c:590-610).
pub fn mark_dirty(state: &mut CacheState) {
    state.dirty = true;
    state.no_sync_defaults = true;
}

/// Finish sync. Dirty clears only on success; `no_sync_defaults` always clears (regcache.c:436-444).
pub fn finish_sync(state: &mut CacheState, sync_succeeded: bool) {
    if sync_succeeded { state.dirty = false; }
    state.no_sync_defaults = false;
}

/// Decide whether a cache value needs hardware sync (regcache.c:323-339).
pub fn register_needs_sync(
    writeable: bool,
    no_sync_defaults: bool,
    value: u32,
    known_default: Option<u32>,
) -> bool {
    if !writeable { return false; }
    if !no_sync_defaults { return true; }
    known_default != Some(value)
}

/// Temporary state used by `regmap_read_bypassed`: force bypass and disable cache-only
/// (regmap.c:2898-2913). Returning the original enables exact restoration by the caller.
pub fn begin_bypassed_read(state: CacheState) -> (CacheState, CacheState) {
    (CacheState { cache_bypass: true, cache_only: false, ..state }, state)
}
