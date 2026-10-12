// SPDX-License-Identifier: GPL-2.0-only
//! PTE / PDE / PPAT encodings against values worked out BY HAND from the Linux macros (intel_gtt.h:135-150, gen8_ppgtt.c:19-58,
//! intel_ggtt.c:281, intel_gtt.c:611-626), never from this crate's output.
use i915_memory_core::pte::*;
use i915_memory_core::CacheLevel;

#[test]
fn the_bit_names_have_linux_values() {
    // intel_gtt.h:152-153, x86 pgtable_types.h: PWT = bit 3, PCD = bit 4, PAT = bit 7
    assert_eq!(GEN8_PAGE_PRESENT, 0x1);
    assert_eq!(GEN8_PAGE_RW, 0x2);
    assert_eq!(PAGE_PWT, 0x8);
    assert_eq!(PAGE_PCD, 0x10);
    assert_eq!(PAGE_PAT, 0x80);
    assert_eq!(PPAT_UNCACHED, 0x18); // PWT | PCD
    assert_eq!(PPAT_CACHED_PDE, 0x0);
    assert_eq!(PPAT_CACHED, 0x80);
    assert_eq!(PPAT_DISPLAY_ELLC, 0x10);
    assert_eq!(PTE_READ_ONLY, 1);
    assert_eq!(PTE_LM, 2);
    assert_eq!(CacheLevel::None as u32, 0); // gem/i915_gem_object_types.h:131-
    assert_eq!(CacheLevel::Llc as u32, 1);
    assert_eq!(CacheLevel::L3Llc as u32, 2);
    assert_eq!(CacheLevel::Wt as u32, 3);
}

#[test]
fn ppgtt_pte_for_every_cache_level_and_the_read_only_flag() {
    let a = 0x12_3456_7000u64;
    // gen8_pte_encode: addr | PRESENT | RW, then RO clears RW, then the PPAT bits by level (gen8_ppgtt.c:36-55)
    assert_eq!(gen8_pte_encode(a, CacheLevel::None, 0), a | 0x1 | 0x2 | 0x18);
    assert_eq!(gen8_pte_encode(a, CacheLevel::Wt, 0), a | 0x1 | 0x2 | 0x10);
    assert_eq!(gen8_pte_encode(a, CacheLevel::Llc, 0), a | 0x1 | 0x2 | 0x80);
    assert_eq!(gen8_pte_encode(a, CacheLevel::L3Llc, 0), a | 0x1 | 0x2 | 0x80);
    assert_eq!(gen8_pte_encode(a, CacheLevel::None, PTE_READ_ONLY), a | 0x1 | 0x18);
    assert_eq!(gen8_pte_encode(a, CacheLevel::Wt, PTE_READ_ONLY), a | 0x1 | 0x10);
    assert_eq!(gen8_pte_encode(a, CacheLevel::Llc, PTE_READ_ONLY), a | 0x1 | 0x80);
    assert_eq!(gen8_pte_encode(0x1000, CacheLevel::None, 0), 0x101b);
    assert_eq!(gen8_pte_encode(0x1000, CacheLevel::Llc, 0), 0x1083);
    assert_eq!(gen8_pte_encode(0x1000, CacheLevel::Wt, 0), 0x1013);
}

#[test]
fn pde_is_cached_pde_unless_the_level_is_none() {
    // gen8_pde_encode (gen8_ppgtt.c:19-30)
    assert_eq!(gen8_pde_encode(0x2000, CacheLevel::None), 0x2000 | 0x3 | 0x18);
    assert_eq!(gen8_pde_encode(0x2000, CacheLevel::Llc), 0x2003); // __set_pd_entry passes I915_CACHE_LLC (intel_ppgtt.c:109)
    assert_eq!(gen8_pde_encode(0x2000, CacheLevel::L3Llc), 0x2003);
    assert_eq!(gen8_pde_encode(0x2000, CacheLevel::Wt), 0x2003);
}

#[test]
fn ggtt_pte_is_address_plus_present_and_nothing_else() {
    // gen8_ggtt_pte_encode (intel_ggtt.c:281-290) with PTE_LM = 0
    assert_eq!(gen8_ggtt_pte_encode(0x5000), 0x5001);
    assert_eq!(gen8_ggtt_pte_encode(0x7f_ffff_f000), 0x7f_ffff_f001);
    assert_eq!(gen8_ggtt_pte_encode(0), 0x1);
}

#[test]
fn private_ppat_is_the_gen9_table() {
    // bdw_setup_private_ppat (intel_gtt.c:611-631), GRAPHICS_VER >= 9: entry 2 = WB | ELLC_OVERRIDE. Bytes worked out by hand:
    // 0: WB|LLC = 3|4 = 0x07   1: WC|LLCELLC = 1|8 = 0x09   2: WB|0 = 0x03   3: UC = 0x00
    // 4: WB|LLCELLC|AGE0 = 0x0b   5: |AGE1 = 0x1b   6: |AGE2 = 0x2b   7: |AGE3 = 0x3b
    assert_eq!(KBL_PPAT, [0x07, 0x09, 0x03, 0x00, 0x0b, 0x1b, 0x2b, 0x3b]);
    assert_eq!(private_ppat(), 0x3b2b_1b0b_0003_0907);
    assert_eq!(private_ppat() as u32, 0x0003_0907); // GEN8_PRIVATE_PAT_LO
    assert_eq!((private_ppat() >> 32) as u32, 0x3b2b_1b0b); // GEN8_PRIVATE_PAT_HI
    // the component names (intel_gtt.h:141-149)
    assert_eq!(GEN8_PPAT_AGE_SHIFT, 4);
    assert_eq!(GEN8_PPAT_LLCELLC, 0x8);
    assert_eq!(GEN8_PPAT_LLC, 0x4);
    assert_eq!((GEN8_PPAT_WB, GEN8_PPAT_WT, GEN8_PPAT_WC, GEN8_PPAT_UC, GEN8_PPAT_ELLC_OVERRIDE), (3, 2, 1, 0, 0));
}

#[test]
fn the_pte_bits_select_the_pat_entry_linux_means_them_to() {
    assert_eq!(pat_index_of(0), 0);
    assert_eq!(pat_index_of(PAGE_PWT), 1);
    assert_eq!(pat_index_of(PAGE_PCD), 2);
    assert_eq!(pat_index_of(PAGE_PWT | PAGE_PCD), 3);
    assert_eq!(pat_index_of(PAGE_PAT), 4);
    assert_eq!(pat_index_of(PAGE_PAT | PAGE_PWT | PAGE_PCD), 7);
    // gen8_pte_encode's three outcomes land on PAT entries 3 (UC), 2 (WB eLLC override), 4 (WB LLCeLLC age 0)...
    let a = 0x1000;
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pte_encode(a, CacheLevel::None, 0)) as usize], 0x00);
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pte_encode(a, CacheLevel::Wt, 0)) as usize], 0x03);
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pte_encode(a, CacheLevel::Llc, 0)) as usize], 0x0b);
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pte_encode(a, CacheLevel::L3Llc, 0)) as usize], 0x0b);
    // ...and a cached PDE lands on entry 0 (WB | LLC)
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pde_encode(a, CacheLevel::Llc)) as usize], 0x07);
    assert_eq!(KBL_PPAT[pat_index_of(gen8_pde_encode(a, CacheLevel::None)) as usize], 0x00);
}
