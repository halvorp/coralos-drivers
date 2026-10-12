// SPDX-License-Identifier: GPL-2.0-only
//! MOCS (Memory Object Control State): the Gen9 table Kaby Lake uses and the plan that programs it (gt/intel_mocs.c).
//! Kaby Lake is `IS_GEN9_BC` (intel_mocs.c:497), so its table is `skl_mocs_table` (:138-155); it has per-ENGINE control
//! registers (`has_global_mocs` is false before Icelake, :444-447) and the render-engine L3CC words `LNCFCMOCS(i)` (:439-442).
use crate::engine::{EngineClass, EngineInfo};
use crate::error::GtError;
use crate::effect::WritePlan;
use i915_devinfo_core::engine::{BCS0, RCS0, VCS0, VCS1, VECS0};
use i915_gt_regs_core as r;
use i915_common_core::Reg;

// ---- field encoders (intel_mocs.c:34-54) ----------------------------------------------------------------------------------

/// `_LE_CACHEABILITY(value)` (intel_mocs.c:34).
pub const fn le_cacheability(v: u32) -> u32 {
    v << 0
}
/// `_LE_TGT_CACHE(value)` (intel_mocs.c:35).
pub const fn le_tgt_cache(v: u32) -> u32 {
    v << 2
}
/// `LE_LRUM(value)` (intel_mocs.c:36).
pub const fn le_lrum(v: u32) -> u32 {
    v << 4
}
/// `L3_ESC(value)` (intel_mocs.c:50): bit 0 - an "enable skip caching" bit `WaDisableSkipCaching` forbids on Gen9.
pub const fn l3_esc(v: u32) -> u32 {
    v << 0
}
/// `L3_SCC(value)` (intel_mocs.c:51): bits 3:1 - the "skip caching control" field `WaDisableSkipCaching` forbids on Gen9.
pub const fn l3_scc(v: u32) -> u32 {
    v << 1
}
/// `_L3_CACHEABILITY(value)` (intel_mocs.c:52).
pub const fn l3_cacheability(v: u32) -> u32 {
    v << 4
}

pub const LE_0_PAGETABLE: u32 = le_cacheability(0); // intel_mocs.c:65
pub const LE_1_UC: u32 = le_cacheability(1); // intel_mocs.c:66
pub const LE_3_WB: u32 = le_cacheability(3); // intel_mocs.c:68
pub const LE_TC_0_PAGETABLE: u32 = le_tgt_cache(0); // intel_mocs.c:71
pub const LE_TC_1_LLC: u32 = le_tgt_cache(1); // intel_mocs.c:72
pub const LE_TC_2_LLC_ELLC: u32 = le_tgt_cache(2); // intel_mocs.c:73
pub const L3_1_UC: u32 = l3_cacheability(1); // intel_mocs.c:78
pub const L3_3_WB: u32 = l3_cacheability(3); // intel_mocs.c:80

/// `GEN9_NUM_MOCS_ENTRIES` (intel_mocs.c:57): entries 63-64 are reserved but configured.
pub const GEN9_NUM_MOCS_ENTRIES: u32 = 64; // intel_mocs.c:57

/// `enum i915_mocs_table_index` (include/uapi/drm/i915_drm.h:131-155): the entries userspace may rely on.
pub const I915_MOCS_UNCACHED: u8 = 0; // i915_drm.h:142
pub const I915_MOCS_PTE: u8 = 1; // i915_drm.h:148
pub const I915_MOCS_CACHED: u8 = 2; // i915_drm.h:154

/// `struct drm_i915_mocs_entry` (intel_mocs.c:18-22) without `used` (a sparse table lists only used entries).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MocsEntry {
    pub control_value: u32,
    pub l3cc_value: u16,
}

/// `struct drm_i915_mocs_table` (intel_mocs.c:24-31) over a sparse entry list.
#[derive(Clone, Copy, Debug)]
pub struct MocsTable {
    /// `ARRAY_SIZE(table)`: one past the highest index the designated initialisers name.
    pub size: u32,
    /// Registers programmed per table.
    pub n_entries: u32,
    pub entries: &'static [(u8, MocsEntry)],
    pub unused_entries_index: u8,
}

/// `GEN9_MOCS_ENTRIES` + the rest of `skl_mocs_table` (intel_mocs.c:130-155).
static SKL_ENTRIES: [(u8, MocsEntry); 4] = [
    (I915_MOCS_UNCACHED, MocsEntry { control_value: LE_1_UC | LE_TC_2_LLC_ELLC, l3cc_value: L3_1_UC as u16 }), // intel_mocs.c:131-133
    (I915_MOCS_PTE, MocsEntry { control_value: LE_0_PAGETABLE | LE_TC_0_PAGETABLE | le_lrum(3), l3cc_value: L3_3_WB as u16 }), // intel_mocs.c:134-136
    (I915_MOCS_CACHED, MocsEntry { control_value: LE_3_WB | LE_TC_2_LLC_ELLC | le_lrum(3), l3cc_value: L3_3_WB as u16 }), // intel_mocs.c:140-142
    (63, MocsEntry { control_value: LE_3_WB | LE_TC_1_LLC | le_lrum(3), l3cc_value: L3_1_UC as u16 }), // intel_mocs.c:152-154
];

/// The table `get_mocs_settings` picks for a Gen9 big-core part (`IS_GEN9_BC`, intel_mocs.c:497-500), with
/// `unused_entries_index = I915_MOCS_PTE` (:461).
pub const KBL_MOCS_TABLE: MocsTable = MocsTable { size: 64, n_entries: GEN9_NUM_MOCS_ENTRIES, entries: &SKL_ENTRIES, unused_entries_index: I915_MOCS_PTE };

impl MocsTable {
    fn used(&self, index: u32) -> Option<&MocsEntry> {
        if index >= self.size {
            return None;
        }
        self.entries.iter().find(|(i, _)| *i as u32 == index).map(|(_, e)| e)
    }
    fn unused(&self) -> &MocsEntry {
        // The unused-entries slot is always a populated entry of the table (Linux indexes the array directly).
        self.used(self.unused_entries_index as u32).unwrap_or(&self.entries[0].1)
    }
    /// `get_entry_control` (intel_mocs.c:542-548): the entry's control value, or the unused-slot's when the index is not used.
    pub fn entry_control(&self, index: u32) -> u32 {
        self.used(index).unwrap_or_else(|| self.unused()).control_value
    }
    /// `get_entry_l3cc` (intel_mocs.c:594-600).
    pub fn entry_l3cc(&self, index: u32) -> u16 {
        self.used(index).unwrap_or_else(|| self.unused()).l3cc_value
    }
    /// `l3cc_combine(get_entry_l3cc(2i), get_entry_l3cc(2i+1))` (intel_mocs.c:602-605, 607-613): word `i` of LNCFCMOCS.
    pub fn l3cc_word(&self, i: u32) -> u32 {
        (self.entry_l3cc(2 * i) as u32) | ((self.entry_l3cc(2 * i + 1) as u32) << 16)
    }
    /// The pre-flight of `get_mocs_settings` (intel_mocs.c:509-523): the table fits, and on Gen9 no entry sets the skip-caching
    /// bits (`WaDisableSkipCaching:skl,bxt,kbl,glk`). Linux returns "no MOCS" (flags = 0) on a violation; this refuses by name.
    pub fn validate_gen9(&self) -> Result<(), GtError> {
        if self.size > self.n_entries {
            return Err(GtError::MocsTableTooLarge { size: self.size, n_entries: self.n_entries });
        }
        for (i, e) in self.entries.iter() {
            if (e.l3cc_value as u32) & (l3_esc(1) | l3_scc(0x7)) != 0 {
                return Err(GtError::MocsSkipCaching { index: *i as u32, l3cc: e.l3cc_value });
            }
        }
        Ok(())
    }
}

/// `mocs_offset` (intel_mocs.c:568-582): the first control register of an engine (`__GEN9_*_MOCS0`).
pub fn mocs_offset(engine_id: u32) -> Result<u32, GtError> {
    match engine_id {
        x if x == RCS0 => Ok(r::GEN9_RCS0_MOCS0),
        x if x == VCS0 => Ok(r::GEN9_VCS0_MOCS0),
        x if x == VCS1 => Ok(r::GEN9_VCS1_MOCS0),
        x if x == VECS0 => Ok(r::GEN9_VECS0_MOCS0),
        x if x == BCS0 => Ok(r::GEN9_BCS0_MOCS0),
        _ => Err(GtError::NoSuchEngine { id: engine_id }),
    }
}

/// The writes of `__init_mocs_table` (intel_mocs.c:555-566): `n_entries` words from `base`, one per index, each the entry's
/// control value or the unused-slot's.
#[derive(Clone, Copy, Debug)]
pub struct ControlWrites {
    table: MocsTable,
    base: u32,
    next: u32,
}

impl Iterator for ControlWrites {
    type Item = WritePlan;
    fn next(&mut self) -> Option<WritePlan> {
        if self.next >= self.table.n_entries {
            return None;
        }
        let w = WritePlan { reg: Reg::new(self.base + self.next * 4), value: self.table.entry_control(self.next) };
        self.next += 1;
        Some(w)
    }
}

/// `init_mocs_table(engine, table)` (intel_mocs.c:584-589): validates the table first (refusing as Linux's flags=0 does).
pub fn engine_control_writes(table: &MocsTable, engine: &EngineInfo) -> Result<ControlWrites, GtError> {
    table.validate_gen9()?;
    Ok(ControlWrites { table: *table, base: mocs_offset(engine.id)?, next: 0 })
}

/// The writes of `init_l3cc_table` (intel_mocs.c:615-629) for a pre-12.55 part: `(n_entries + 1) / 2` words
/// `GEN9_LNCFCMOCS(i)`.
#[derive(Clone, Copy, Debug)]
pub struct L3ccWrites {
    table: MocsTable,
    next: u32,
}

impl Iterator for L3ccWrites {
    type Item = WritePlan;
    fn next(&mut self) -> Option<WritePlan> {
        if self.next >= self.table.n_entries.div_ceil(2) {
            return None;
        }
        let w = WritePlan { reg: r::gen9_lncfcmocs(self.next), value: self.table.l3cc_word(self.next) };
        self.next += 1;
        Some(w)
    }
}

pub fn l3cc_writes(table: &MocsTable) -> Result<L3ccWrites, GtError> {
    table.validate_gen9()?;
    Ok(L3ccWrites { table: *table, next: 0 })
}

/// `intel_mocs_init_engine` (intel_mocs.c:631-651): which of the two programs an engine runs on Kaby Lake - its own control
/// table always (`HAS_ENGINE_MOCS`), plus the L3CC words when it is the render engine (`HAS_RENDER_L3CC`).
pub fn engine_programs_l3cc(engine: &EngineInfo) -> bool {
    engine.class == EngineClass::Render
}
