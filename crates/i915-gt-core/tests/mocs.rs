// SPDX-License-Identifier: GPL-2.0-only
//! The Gen9 MOCS table and its programming plan against Linux. Entry values are what GCC evaluates from the macro block of
//! gt/intel_mocs.c:34-80 (LE_*, L3_*), written out literally.
use i915_gt_core::engine::engine_info;
use i915_gt_core::mocs::*;
use i915_gt_core::GtError;

const RCS0: u32 = 0;
const BCS0: u32 = 1;
const VCS0: u32 = 10;
const VCS1: u32 = 11;
const VECS0: u32 = 18;

#[test]
fn skl_table_entries_are_linux_literals() {
    let t = SKL_MOCS_TABLE;
    // I915_MOCS_UNCACHED: LE_1_UC | LE_TC_2_LLC_ELLC, L3_1_UC (intel_mocs.c:131-133)
    assert_eq!((t.entry_control(0), t.entry_l3cc(0)), (0x9, 0x10));
    // I915_MOCS_PTE: LE_0_PAGETABLE | LE_TC_0_PAGETABLE | LE_LRUM(3), L3_3_WB (:134-136)
    assert_eq!((t.entry_control(1), t.entry_l3cc(1)), (0x30, 0x30));
    // I915_MOCS_CACHED: LE_3_WB | LE_TC_2_LLC_ELLC | LE_LRUM(3), L3_3_WB (:140-142)
    assert_eq!((t.entry_control(2), t.entry_l3cc(2)), (0x3b, 0x30));
    // mocs:63: LE_3_WB | LE_TC_1_LLC | LE_LRUM(3), L3_1_UC (:152-154)
    assert_eq!((t.entry_control(63), t.entry_l3cc(63)), (0x37, 0x10));
}

#[test]
fn unused_entries_fall_back_to_the_pte_entry() {
    let t = SKL_MOCS_TABLE;
    assert_eq!(t.unused_entries_index, I915_MOCS_PTE);
    for i in (3..63).chain([64, 100]) {
        assert_eq!((t.entry_control(i), t.entry_l3cc(i)), (0x30, 0x30), "index {i}");
    }
}

#[test]
fn uapi_indexes_are_distinct_and_pinned() {
    assert_eq!((I915_MOCS_UNCACHED, I915_MOCS_PTE, I915_MOCS_CACHED), (0, 1, 2));
    assert_eq!(GEN9_NUM_MOCS_ENTRIES, 64);
    assert_eq!(SKL_MOCS_TABLE.size, 64);
    assert_eq!(SKL_MOCS_TABLE.n_entries, 64);
    assert_eq!(SKL_MOCS_TABLE.entries.len(), 4, "only the four named indexes are used");
}

#[test]
fn field_encoders_are_linux_shifts() {
    assert_eq!((le_cacheability(3), le_tgt_cache(2), le_lrum(3)), (3, 8, 0x30));
    assert_eq!((l3_esc(1), l3_scc(7), l3_cacheability(3)), (1, 0xe, 0x30));
    assert_eq!((LE_0_PAGETABLE, LE_1_UC, LE_3_WB), (0, 1, 3));
    assert_eq!((LE_TC_0_PAGETABLE, LE_TC_1_LLC, LE_TC_2_LLC_ELLC), (0, 4, 8));
    assert_eq!((L3_1_UC, L3_3_WB), (0x10, 0x30));
}

#[test]
fn l3cc_words_pair_two_entries_low_then_high() {
    let t = SKL_MOCS_TABLE;
    assert_eq!(t.l3cc_word(0), 0x0030_0010, "entry 0 low (L3_1_UC), entry 1 high (L3_3_WB)");
    assert_eq!(t.l3cc_word(1), 0x0030_0030, "entry 2 (CACHED) and entry 3 (unused -> PTE)");
    assert_eq!(t.l3cc_word(15), 0x0030_0030);
    assert_eq!(t.l3cc_word(31), 0x0010_0030, "entry 62 (unused -> PTE) low, entry 63 (L3_1_UC) high");
}

#[test]
fn control_writes_cover_all_64_registers_of_the_engine_block() {
    let rcs = engine_info(9, RCS0).unwrap();
    let w: Vec<_> = engine_control_writes(&SKL_MOCS_TABLE, &rcs).unwrap().collect();
    assert_eq!(w.len(), 64);
    assert_eq!((w[0].reg.offset(), w[0].value), (0xc800, 0x9));
    assert_eq!((w[1].reg.offset(), w[1].value), (0xc804, 0x30));
    assert_eq!((w[2].reg.offset(), w[2].value), (0xc808, 0x3b));
    assert_eq!((w[3].reg.offset(), w[3].value), (0xc80c, 0x30), "unused index written with the PTE control value");
    assert_eq!((w[63].reg.offset(), w[63].value), (0xc8fc, 0x37));
}

#[test]
fn each_engine_has_its_own_mocs_block() {
    // __GEN9_RCS0_MOCS0 0xc800, VCS0 0xc900, VCS1 0xca00, VECS0 0xcb00, BCS0 0xcc00 (intel_gt_regs.h:1021-1031)
    for (id, base) in [(RCS0, 0xc800u32), (VCS0, 0xc900), (VCS1, 0xca00), (VECS0, 0xcb00), (BCS0, 0xcc00)] {
        assert_eq!(mocs_offset(id).unwrap(), base, "engine {id}");
        let e = engine_info(9, id).unwrap();
        let w: Vec<_> = engine_control_writes(&SKL_MOCS_TABLE, &e).unwrap().collect();
        assert_eq!(w[0].reg.offset(), base);
        assert_eq!(w[63].reg.offset(), base + 63 * 4);
    }
    assert_eq!(mocs_offset(2), Err(GtError::NoSuchEngine { id: 2 }));
}

#[test]
fn only_the_render_engine_programs_the_l3cc_words() {
    assert!(engine_programs_l3cc(&engine_info(9, RCS0).unwrap()));
    for id in [BCS0, VCS0, VCS1, VECS0] {
        assert!(!engine_programs_l3cc(&engine_info(9, id).unwrap()), "engine {id}");
    }
    let w: Vec<_> = l3cc_writes(&SKL_MOCS_TABLE).unwrap().collect();
    assert_eq!(w.len(), 32, "(n_entries + 1) / 2 words");
    assert_eq!((w[0].reg.offset(), w[0].value), (0xb020, 0x0030_0010));
    assert_eq!((w[31].reg.offset(), w[31].value), (0xb09c, 0x0010_0030));
}

#[test]
fn skip_caching_bits_in_an_l3cc_value_refuse_the_whole_table() {
    // WaDisableSkipCaching:skl,bxt,kbl,glk (intel_mocs.c:514-523): L3_ESC(1) | L3_SCC(7) = 0xf must be clear
    static BAD: [(u8, MocsEntry); 2] = [
        (0, MocsEntry { control_value: 0x9, l3cc_value: 0x10 }),
        (1, MocsEntry { control_value: 0x30, l3cc_value: 0x30 | 0x2 }),
    ];
    let t = MocsTable { name: "test", size: 2, n_entries: 64, entries: &BAD, unused_entries_index: 1 };
    assert_eq!(t.validate_gen9(), Err(GtError::MocsSkipCaching { index: 1, l3cc: 0x32 }));
    let rcs = engine_info(9, RCS0).unwrap();
    assert_eq!(engine_control_writes(&t, &rcs).unwrap_err().name(), "MOCS-SKIP-CACHING");
    assert_eq!(l3cc_writes(&t).unwrap_err().name(), "MOCS-SKIP-CACHING");
    // each of the four bits individually
    for bit in [0x1u16, 0x2, 0x4, 0x8] {
        static E: [(u8, MocsEntry); 1] = [(0, MocsEntry { control_value: 0, l3cc_value: 0 })];
        let mut e = E;
        e[0].1.l3cc_value = bit;
        // leak a copy so the slice is 'static for this probe
        let leaked: &'static [(u8, MocsEntry)] = Box::leak(Box::new(e));
        let t = MocsTable { name: "test", size: 1, n_entries: 64, entries: leaked, unused_entries_index: 0 };
        assert!(matches!(t.validate_gen9(), Err(GtError::MocsSkipCaching { .. })), "bit {bit:#x}");
    }
    assert_eq!(SKL_MOCS_TABLE.validate_gen9(), Ok(()));
}

#[test]
fn a_table_larger_than_the_hardware_is_refused() {
    static E: [(u8, MocsEntry); 1] = [(0, MocsEntry { control_value: 0, l3cc_value: 0 })];
    let t = MocsTable { name: "test", size: 65, n_entries: 64, entries: &E, unused_entries_index: 0 };
    assert_eq!(t.validate_gen9(), Err(GtError::MocsTableTooLarge { size: 65, n_entries: 64 }));
}

// ---- get_mocs_settings (intel_mocs.c:454-534): the table a platform gets, and the programs intel_mocs_init / intel_mocs_init_engine run ----

use i915_devinfo_core::lookup;

#[test]
fn kaby_lake_and_skylake_get_the_skl_table_with_per_engine_control_registers() {
    // KBL is IS_GEN9_BC (i915_drv.h:543: GRAPHICS_VER == 9 && !IS_GEN9_LP), so get_mocs_settings picks skl_mocs_table (intel_mocs.c:497-500);
    // has_global_mocs is a GEN12_FEATURES flag (i915_pci.c:635), so Gen9 sets HAS_ENGINE_MOCS, not HAS_GLOBAL_MOCS (:525-530), and
    // has_l3cc() is unconditionally true (:439-442).
    for devid in [0x591c_u16 /* KBL GT2 Amber Lake-Y, the Pixelbook Go */, 0x5916, 0x1916 /* SKL GT2 */] {
        let s = mocs_settings(&lookup(devid).unwrap()).unwrap().expect("a Gen9 part has a MOCS table");
        assert_eq!(s.table.name, "skl", "{devid:#x}");
        assert_eq!((s.engine_mocs, s.global_mocs, s.render_l3cc), (true, false, true), "{devid:#x}");
        assert_eq!(s.table.unused_entries_index, I915_MOCS_PTE, "table->unused_entries_index = I915_MOCS_PTE (:461)");
    }
}

#[test]
fn a_gen8_part_gets_no_mocs_program_as_linux_returns_no_flags() {
    // Cherryview: none of the branches match and GRAPHICS_VER < 9 does not even warn (intel_mocs.c:505-509): flags = 0, nothing programmed.
    assert!(mocs_settings(&lookup(0x22b0).unwrap()).unwrap().is_none());
}

#[test]
fn broxton_table_entries_are_linux_literals() {
    // broxton_mocs_table (intel_mocs.c:157-163): GEN9_MOCS_ENTRIES + I915_MOCS_CACHED = LE_1_UC | LE_TC_2_LLC_ELLC | LE_LRUM(3), L3_3_WB
    let t = BXT_MOCS_TABLE;
    assert_eq!(t.name, "bxt");
    assert_eq!((t.size, t.n_entries, t.unused_entries_index), (3, 64, I915_MOCS_PTE));
    assert_eq!((t.entry_control(0), t.entry_l3cc(0)), (0x9, 0x10));
    assert_eq!((t.entry_control(1), t.entry_l3cc(1)), (0x30, 0x30));
    assert_eq!((t.entry_control(2), t.entry_l3cc(2)), (0x39, 0x30));
    assert_eq!((t.entry_control(63), t.entry_l3cc(63)), (0x30, 0x30), "no entry 63 on Broxton: the PTE entry");
    assert_eq!(t.validate_gen9(), Ok(()));
}

#[test]
fn the_render_engine_program_is_its_64_control_words_then_the_32_l3cc_words() {
    // intel_mocs_init_engine (intel_mocs.c:631-649): HAS_ENGINE_MOCS -> init_mocs_table(engine) (:644-645), then for RENDER_CLASS
    // init_l3cc_table (:647-648).
    let s = mocs_settings(&lookup(0x591c).unwrap()).unwrap().unwrap();
    let p = engine_init_writes(&s, &engine_info(9, RCS0).unwrap()).unwrap();
    assert_eq!((p.control_entries(), p.l3cc_words()), (64, 32));
    let w: Vec<_> = p.collect();
    assert_eq!(w.len(), 96);
    assert_eq!((w[0].reg.offset(), w[0].value), (0xc800, 0x9), "I915_MOCS_UNCACHED (:131-133)");
    assert_eq!((w[1].reg.offset(), w[1].value), (0xc804, 0x30), "I915_MOCS_PTE (:134-136)");
    assert_eq!((w[2].reg.offset(), w[2].value), (0xc808, 0x3b), "I915_MOCS_CACHED (:140-142)");
    assert_eq!((w[40].reg.offset(), w[40].value), (0xc8a0, 0x30), "unused -> unused_entries_index (:542-548)");
    assert_eq!((w[63].reg.offset(), w[63].value), (0xc8fc, 0x37), "mocs:63 (:152-154)");
    assert_eq!((w[64].reg.offset(), w[64].value), (0xb020, 0x0030_0010), "l3cc_combine(entry 0, entry 1) (:602-613)");
    assert_eq!((w[65].reg.offset(), w[65].value), (0xb024, 0x0030_0030));
    assert_eq!((w[95].reg.offset(), w[95].value), (0xb09c, 0x0010_0030), "entry 62 low, entry 63 (L3_1_UC) high");
}

#[test]
fn a_non_render_engine_program_is_its_control_words_only() {
    let s = mocs_settings(&lookup(0x591c).unwrap()).unwrap().unwrap();
    for (id, base) in [(BCS0, 0xcc00u32), (VCS0, 0xc900), (VECS0, 0xcb00)] {
        let p = engine_init_writes(&s, &engine_info(9, id).unwrap()).unwrap();
        assert_eq!((p.control_entries(), p.l3cc_words()), (64, 0), "engine {id}");
        let w: Vec<_> = p.collect();
        assert_eq!(w.len(), 64);
        assert!(w.iter().all(|w| (base..base + 0x100).contains(&w.reg.offset())), "engine {id}");
    }
}

#[test]
fn the_gt_program_on_gen9_is_the_l3cc_words_only() {
    // intel_mocs_init (intel_mocs.c:666-685): HAS_GLOBAL_MOCS is not set on Gen9, so only init_l3cc_table runs (:683-684).
    let s = mocs_settings(&lookup(0x591c).unwrap()).unwrap().unwrap();
    let p = gt_init_writes(&s).unwrap();
    assert_eq!((p.control_entries(), p.l3cc_words()), (0, 32));
    let w: Vec<_> = p.collect();
    assert_eq!(w.len(), 32);
    assert_eq!((w[0].reg.offset(), w[31].reg.offset()), (0xb020, 0xb09c));
}

#[test]
fn global_mocs_settings_are_refused_by_name_not_written_per_engine() {
    // a platform with has_global_mocs (Gen12+) writes GEN12_GLOBAL_MOCS, which this port does not carry
    let s = MocsSettings { table: SKL_MOCS_TABLE, engine_mocs: false, global_mocs: true, render_l3cc: true };
    assert_eq!(gt_init_writes(&s).unwrap_err().name(), "MOCS-GLOBAL-NOT-CARRIED");
    let p = engine_init_writes(&s, &engine_info(9, BCS0).unwrap()).unwrap();
    assert_eq!(p.count(), 0, "no per-engine table when the MOCS are global (intel_mocs.c:643-645)");
}
