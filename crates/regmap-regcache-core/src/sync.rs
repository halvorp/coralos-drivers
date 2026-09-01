// SPDX-License-Identifier: GPL-2.0-only
//! Dirty tracking plus sync/drop range selection and write ordering.
//!
//! Ported from Linux `drivers/base/regmap/regcache.c:325-378,396-565,603-610,749-885` and
//! `drivers/base/regmap/regcache-rbtree.c:465-540`.
//!
//! Copyright 2011 Wolfson Microelectronics plc.
//! Original author: Dimitris Papastamos.

use core::fmt;

use crate::defaults::{lookup_reg_default, RegDefault};

/// Linux's two dirty/sync-default flags (regcache.c:417-443,603-608).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DirtyState {
    pub cache_dirty: bool,
    pub no_sync_defaults: bool,
}

/// One cached register observation. The caller supplies policy answers; this crate performs no I/O.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CachedRegister {
    pub reg: u32,
    pub value: Option<u32>,
    pub present: bool,
    pub writeable: bool,
    pub volatile: bool,
}

/// One patch write applied before cache data (regcache.c:420-430).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchWrite {
    pub reg: u32,
    pub value: u32,
}

/// One hardware write decision. Patch entries precede cache entries, and register writes preserve
/// the ascending scan/in-order traversal used by Linux (regcache.c:420-435,349-375;
/// regcache-rbtree.c:477-500).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncWrite {
    pub reg: u32,
    pub value: u32,
    pub source: SyncWriteSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncWriteSource {
    Patch,
    Cache,
}

/// Literal names for the two write sources (regcache.c:420-435).
pub const SYNC_WRITE_SOURCE_NAMES: [&str; 2] = ["patch", "cache"];

/// Output capacity or range refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncError {
    RangeReversed { min: u32, max: u32 },
    OutputTooSmall { supplied: usize, required: usize },
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::RangeReversed { min, max } => write!(
                f,
                "regcache sync/drop range refused {min:#x}..={max:#x}: minimum exceeds maximum"
            ),
            Self::OutputTooSmall { supplied, required } => write!(
                f,
                "regcache sync plan refused output capacity {supplied}: {required} writes are required"
            ),
        }
    }
}

/// Mark hardware reset/power loss: dirty and compare cache values with reset defaults
/// (regcache.c:590-610).
pub fn mark_dirty(state: &mut DirtyState) {
    state.cache_dirty = true;
    state.no_sync_defaults = true;
}

/// Complete a full sync. Dirty clears only after cache sync succeeds; the default-suppression flag
/// clears on every exit (regcache.c:432-443).
pub fn finish_full_sync(state: &mut DirtyState, sync_succeeded: bool) {
    if sync_succeeded {
        state.cache_dirty = false;
    }
    state.no_sync_defaults = false;
}

/// Complete a region sync. Linux leaves the global dirty bit set, but always clears
/// `no_sync_defaults` (regcache.c:511-525).
pub fn finish_region_sync(state: &mut DirtyState) {
    state.no_sync_defaults = false;
}

/// Whether one cached value must be written (regcache.c:325-342).
pub fn register_needs_sync(
    writeable: bool,
    no_sync_defaults: bool,
    reg: u32,
    value: u32,
    defaults: &[RegDefault],
) -> bool {
    if !writeable {
        return false; // regcache.c:330-331
    }
    if !no_sync_defaults {
        return true; // regcache.c:333-335
    }
    match lookup_reg_default(defaults, reg) {
        Some(index) => value != defaults[index].def, // regcache.c:337-340
        None => true,
    }
}

fn cache_write_for(
    entry: CachedRegister,
    min: u32,
    max: u32,
    no_sync_defaults: bool,
    defaults: &[RegDefault],
) -> Option<SyncWrite> {
    if entry.reg < min || entry.reg > max || entry.volatile || !entry.writeable || !entry.present {
        return None; // regcache.c:353-359,749-755,789-795
    }
    let value = entry.value?; // regcache.c:357-361: ENOENT is skipped
    register_needs_sync(
        entry.writeable,
        no_sync_defaults,
        entry.reg,
        value,
        defaults,
    )
    .then_some(SyncWrite {
        reg: entry.reg,
        value,
        source: SyncWriteSource::Cache,
    })
}

fn sort_cache_writes_by_register(writes: &mut [SyncWrite]) {
    // Linux's default loop increments `reg`, while RBTREE traverses nodes in order
    // (regcache.c:349-375; regcache-rbtree.c:477-500). Insertion sort keeps this allocation-free.
    for i in 1..writes.len() {
        let write = writes[i];
        let mut j = i;
        while j > 0 && writes[j - 1].reg > write.reg {
            writes[j] = writes[j - 1];
            j -= 1;
        }
        writes[j] = write;
    }
}

/// Build a full-sync write list: when dirty, patches in table order first, then eligible cache
/// registers in ascending register order (regcache.c:349-375,417-438;
/// regcache-rbtree.c:477-500).
///
/// The caller retains all hardware access. This function writes only into `out`.
pub fn plan_full_sync(
    state: DirtyState,
    patches: &[PatchWrite],
    cache: &[CachedRegister],
    defaults: &[RegDefault],
    max_register: u32,
    out: &mut [SyncWrite],
) -> Result<usize, SyncError> {
    if !state.cache_dirty {
        return Ok(0); // regcache.c:417-418
    }
    let cache_count = cache
        .iter()
        .filter_map(|entry| {
            cache_write_for(*entry, 0, max_register, state.no_sync_defaults, defaults)
        })
        .count();
    let required = patches.len() + cache_count;
    if out.len() < required {
        return Err(SyncError::OutputTooSmall {
            supplied: out.len(),
            required,
        });
    }
    let mut used = 0;
    for patch in patches {
        out[used] = SyncWrite {
            reg: patch.reg,
            value: patch.value,
            source: SyncWriteSource::Patch,
        }; // regcache.c:420-429
        used += 1;
    }
    let cache_start = used;
    for entry in cache {
        if let Some(write) =
            cache_write_for(*entry, 0, max_register, state.no_sync_defaults, defaults)
        {
            out[used] = write;
            used += 1;
        }
    }
    sort_cache_writes_by_register(&mut out[cache_start..used]);
    Ok(used)
}

/// Build a region-sync write list. Patches are deliberately absent, and Linux does not clear the
/// global dirty bit after a region sync (regcache.c:489-534).
pub fn plan_region_sync(
    state: DirtyState,
    min: u32,
    max: u32,
    cache: &[CachedRegister],
    defaults: &[RegDefault],
    out: &mut [SyncWrite],
) -> Result<usize, SyncError> {
    if min > max {
        return Err(SyncError::RangeReversed { min, max });
    }
    if !state.cache_dirty {
        return Ok(0); // regcache.c:511-512
    }
    let required = cache
        .iter()
        .filter_map(|entry| cache_write_for(*entry, min, max, state.no_sync_defaults, defaults))
        .count();
    if out.len() < required {
        return Err(SyncError::OutputTooSmall {
            supplied: out.len(),
            required,
        });
    }
    let mut used = 0;
    for entry in cache {
        if let Some(write) = cache_write_for(*entry, min, max, state.no_sync_defaults, defaults) {
            out[used] = write;
            used += 1;
        }
    }
    sort_cache_writes_by_register(&mut out[..used]);
    Ok(used)
}

/// Apply Linux's inclusive drop region to caller-owned presence bits
/// (regcache-flat.c:146-155; regcache-rbtree.c:510-540).
pub fn drop_region(
    min: u32,
    max: u32,
    cache: &[CachedRegister],
    present_out: &mut [bool],
) -> Result<usize, SyncError> {
    if min > max {
        return Err(SyncError::RangeReversed { min, max });
    }
    if present_out.len() < cache.len() {
        return Err(SyncError::OutputTooSmall {
            supplied: present_out.len(),
            required: cache.len(),
        });
    }
    for (slot, entry) in present_out.iter_mut().zip(cache.iter()) {
        *slot = entry.present && !(entry.reg >= min && entry.reg <= max);
    }
    Ok(cache.len())
}
