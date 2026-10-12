// SPDX-License-Identifier: GPL-2.0-only
//! Page-table-entry and page-directory-entry encoding for Gen8/Gen9, and the private PAT the entries select.
//!
//! Ported from gt/gen8_ppgtt.c (`gen8_pde_encode`, `gen8_pte_encode`), gt/intel_ggtt.c (`gen8_ggtt_pte_encode`/`decode`),
//! gt/intel_gtt.h (the bit and PPAT names) and gt/intel_gtt.c (`bdw_setup_private_ppat`), v7.0-rc6. Only the Gen8/Gen9 encoders
//! are carried: `gen12_pte_encode` / `mtl_ggtt_pte_encode` / the Gen6-7 PTEs are other generations and are not ported.

/// x86 `_PAGE_PWT` / `_PAGE_PCD` / `_PAT` (arch/x86/include/asm/pgtable_types.h:13-18,54-63): the three bits that pick the PAT index.
pub const PAGE_PWT: u64 = 1 << 3; // pgtable_types.h:54
pub const PAGE_PCD: u64 = 1 << 4; // pgtable_types.h:55
pub const PAGE_PAT: u64 = 1 << 7; // pgtable_types.h:63

pub const GEN8_PAGE_PRESENT: u64 = 1 << 0; // intel_gtt.h:152
pub const GEN8_PAGE_RW: u64 = 1 << 1; // intel_gtt.h:153

/// `PPAT_UNCACHED` = PAT index 3 (intel_gtt.h:135), `PPAT_CACHED_PDE` = index 0 (:136), `PPAT_CACHED` = index 4 (:137),
/// `PPAT_DISPLAY_ELLC` = index 2 (:138).
pub const PPAT_UNCACHED: u64 = PAGE_PWT | PAGE_PCD; // intel_gtt.h:135
pub const PPAT_CACHED_PDE: u64 = 0; // intel_gtt.h:136
pub const PPAT_CACHED: u64 = PAGE_PAT; // intel_gtt.h:137
pub const PPAT_DISPLAY_ELLC: u64 = PAGE_PCD; // intel_gtt.h:138

/// `PTE_READ_ONLY` / `PTE_LM` request flags (intel_gtt.h:316-317). `PTE_LM` is local memory: never on Gen9, refused.
pub const PTE_READ_ONLY: u32 = 1 << 0; // intel_gtt.h:316
pub const PTE_LM: u32 = 1 << 1; // intel_gtt.h:317

/// `enum i915_cache_level` (gem/i915_gem_object_types.h:131): the values ARE the pat_index on pre-Gen12 (gen8_ppgtt.c:41-43).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheLevel {
    None = 0,
    Llc = 1,
    L3Llc = 2,
    Wt = 3,
}

/// `gen8_pde_encode` (gen8_ppgtt.c:19): the entry that links a table page into its parent.
pub const fn gen8_pde_encode(addr: u64, level: CacheLevel) -> u64 {
    let pde = addr | GEN8_PAGE_PRESENT | GEN8_PAGE_RW;
    match level {
        CacheLevel::None => pde | PPAT_UNCACHED,
        _ => pde | PPAT_CACHED_PDE,
    }
}

/// `gen8_pte_encode` (gen8_ppgtt.c:32): a leaf entry of the PPGTT.
pub const fn gen8_pte_encode(addr: u64, level: CacheLevel, flags: u32) -> u64 {
    let mut pte = addr | GEN8_PAGE_PRESENT | GEN8_PAGE_RW;
    if flags & PTE_READ_ONLY != 0 {
        pte &= !GEN8_PAGE_RW;
    }
    match level {
        CacheLevel::None => pte | PPAT_UNCACHED,
        CacheLevel::Wt => pte | PPAT_DISPLAY_ELLC,
        _ => pte | PPAT_CACHED,
    }
}

/// `gen8_ggtt_pte_encode` (intel_ggtt.c:281): a GGTT entry carries only the address and PRESENT on Gen8/9 - no cache bits, no RW.
pub const fn gen8_ggtt_pte_encode(addr: u64) -> u64 {
    addr | GEN8_PAGE_PRESENT
}

/// The address bits of a Gen8/Gen9 entry: bits 38:12 (39-bit device addresses, `dma_mask_size` = 39, i915_pci.c:416).
/// (Linux decodes the GGTT with the wider `GEN12_GGTT_PTE_ADDR_MASK` = bits 45:12, intel_gtt.h:100; the KBL planners never
/// emit an address above 39 bits, so the narrower mask is exact for what is written.)
pub const GEN8_PTE_ADDR_MASK: u64 = ((1u64 << 39) - 1) & !0xfff;

/// The PAT index (0..=7) an entry's PWT / PCD / PAT bits select.
pub const fn pat_index_of(entry: u64) -> u8 {
    ((entry & PAGE_PAT != 0) as u8) << 2 | ((entry & PAGE_PCD != 0) as u8) << 1 | (entry & PAGE_PWT != 0) as u8
}

// ---- the private PAT (bdw_setup_private_ppat, intel_gtt.c:611) ----------------------------------------------------------
pub const GEN8_PPAT_AGE_SHIFT: u32 = 4; // intel_gtt.h:141
pub const GEN8_PPAT_LLCELLC: u8 = 2 << 2; // intel_gtt.h:143
pub const GEN8_PPAT_LLC: u8 = 1 << 2; // intel_gtt.h:144
pub const GEN8_PPAT_WB: u8 = 3; // intel_gtt.h:145
pub const GEN8_PPAT_WT: u8 = 2; // intel_gtt.h:146
pub const GEN8_PPAT_WC: u8 = 1; // intel_gtt.h:147
pub const GEN8_PPAT_UC: u8 = 0; // intel_gtt.h:148
pub const GEN8_PPAT_ELLC_OVERRIDE: u8 = 0; // intel_gtt.h:149

const fn age(x: u8) -> u8 {
    x << GEN8_PPAT_AGE_SHIFT // intel_gtt.h:141
}

/// The eight PAT entries for Gen9+ (`GRAPHICS_VER >= 9` picks entry 2 = WB | ELLC_OVERRIDE, intel_gtt.c:623).
pub const KBL_PPAT: [u8; 8] = [
    GEN8_PPAT_WB | GEN8_PPAT_LLC,                              // 0, intel_gtt.c:614: normal objects, no eLLC
    GEN8_PPAT_WC | GEN8_PPAT_LLCELLC,                          // 1, :615
    GEN8_PPAT_WB | GEN8_PPAT_ELLC_OVERRIDE,                    // 2, :624 (gen9+)
    GEN8_PPAT_UC,                                              // 3, :616
    GEN8_PPAT_WB | GEN8_PPAT_LLCELLC | age(0),                 // 4, :617
    GEN8_PPAT_WB | GEN8_PPAT_LLCELLC | age(1),                 // 5, :618
    GEN8_PPAT_WB | GEN8_PPAT_LLCELLC | age(2),                 // 6, :619
    GEN8_PPAT_WB | GEN8_PPAT_LLCELLC | age(3),                 // 7, :620
];

/// `GEN8_PPAT(i, x)` or-ed together: the 64-bit value Linux splits into `GEN8_PRIVATE_PAT_LO/HI`.
pub const fn private_ppat() -> u64 {
    let mut v = 0u64;
    let mut i = 0;
    while i < 8 {
        v |= (KBL_PPAT[i] as u64) << (i * 8); // GEN8_PPAT(i, x), intel_gtt.h:150
        i += 1;
    }
    v
}
