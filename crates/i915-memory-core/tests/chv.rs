// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview (8086:22B0, Gen8, the Z83) rows of i915-memory-core, ruling D8: the GGTT size (`chv_get_total_gtt_size`), stolen
//! sizing and its reserved part (`chv_stolen_size`, `chv_get_stolen_reserved`), 16 fences, the CHV private PAT, and the 3-level
//! 32-bit PPGTT with its four preallocated page directories (`gen8_ppgtt_create` with `vm->top = 2`). Every expected value is a
//! Linux literal (linux-ref v7.0-rc6) worked out by hand, never read back from the table under test.
mod common;
use common::*;
use i915_memory_core::fence::{plan_fence_of, Tiling};
use i915_memory_core::probe::{chv_gtt_table_bytes, chv_stolen_reserved, chv_stolen_size, decode_stolen_for, GgttGeometry, Stolen};
use i915_memory_core::pte::{private_ppat_for, CHV_PPAT, CHV_PPAT_SNOOP};
use i915_memory_core::regs::CHV_NUM_FENCES;
use i915_memory_core::*;

const MIB: u64 = 1 << 20;

fn chv() -> i915_devinfo_core::PlatformDescriptor {
    i915_devinfo_core::lookup(0x22b0).expect("CHV row")
}

fn gen7() -> i915_devinfo_core::PlatformDescriptor {
    let mut d = chv();
    d.info.graphics_ver = 7;
    d
}

// ---- GGTT size ------------------------------------------------------------------------------------------------------------

#[test]
fn chv_ggtt_size_reads_the_snb_ggms_field() {
    // chv_get_total_gtt_size (intel_ggtt.c:1169-1178): GGMS = (ctl >> 8) & 3 (i915_drm.h:50-51), size = 1 << (20 + GGMS)
    assert_eq!(chv_gtt_table_bytes(1 << 8), Ok(2 * MIB));
    assert_eq!(chv_gtt_table_bytes(2 << 8), Ok(4 * MIB));
    assert_eq!(chv_gtt_table_bytes(3 << 8), Ok(8 * MIB));
    assert_eq!(chv_gtt_table_bytes(0), Err(MemError::GgttAbsent { gmch_ctl: 0 }));
    // the BDW field (bits 7:6) is not Cherryview's: GGMS 3 there and 0 in bits 9:8 is "no GTT"
    assert_eq!(chv_gtt_table_bytes(3 << 6), Err(MemError::GgttAbsent { gmch_ctl: 0xc0 }));
    // bits above the 2-bit field are masked: 0xfc00 | GGMS 1
    assert_eq!(chv_gtt_table_bytes(0xfc00 | (1 << 8)), Ok(2 * MIB));
}

#[test]
fn probe_for_dispatches_by_row() {
    // Cherryview: total = table / 8 * 4096 (intel_ggtt.c:1284); 16 fences (intel_ggtt_fencing.c:855-861)
    let g = GgttGeometry::probe_for(&chv(), 3 << 8, 16 * MIB, 256 * MIB).unwrap();
    assert_eq!((g.total, g.entries, g.mappable_end, g.pte_base, g.fences), (4 << 30, 1 << 20, 256 * MIB, 0x80_0000, 16));
    assert_eq!(GgttGeometry::probe_for(&chv(), 1 << 8, 16 * MIB, 256 * MIB).unwrap().total, 1 << 30);
    assert_eq!(GgttGeometry::probe_for(&chv(), 3 << 6, 16 * MIB, 256 * MIB), Err(MemError::GgttAbsent { gmch_ctl: 0xc0 }));
    // Gen8+ GTTMMADR is 16 MiB on Cherryview too (gen6_gttmmadr_size, intel_ggtt.c:1187)
    assert_eq!(GgttGeometry::probe_for(&chv(), 3 << 8, 4 * MIB, 256 * MIB), Err(MemError::Bar0Size { got: 4 * MIB, want: 16 * MIB }));
    // Gen9 takes the BDW field and 32 fences
    let k = GgttGeometry::probe_for(&kbl(), 3 << 6, 16 * MIB, 256 * MIB).unwrap();
    assert_eq!((k.total, k.fences), (4 << 30, 32));
    assert_eq!(GgttGeometry::probe_for(&gen7(), 3 << 8, 16 * MIB, 256 * MIB), Err(MemError::UnsupportedGeneration { gen: 7 }));
    assert_eq!(CHV_NUM_FENCES, 16); // intel_ggtt_fencing.c:861
}

#[test]
fn chv_fence_ids_stop_at_sixteen() {
    assert!(plan_fence_of(CHV_NUM_FENCES, 15, 0, 4096, Tiling::X, 128).is_ok());
    assert_eq!(plan_fence_of(CHV_NUM_FENCES, 16, 0, 4096, Tiling::X, 128).err(), Some(MemError::BadFence { what: "fence id", value: 16 }));
}

// ---- stolen ---------------------------------------------------------------------------------------------------------------

#[test]
fn chv_stolen_size_is_the_three_piece_table() {
    // chv_stolen_size (early-quirks.c:426-445): GMS = (ctl >> 3) & 0x1f (i915_drm.h:52-53)
    let gms = |g: u16| chv_stolen_size(g << 3);
    assert_eq!(gms(0x00), 0);
    assert_eq!(gms(0x01), 32 * MIB); // 0x0..0x10: 32MB increments from 0
    assert_eq!(gms(0x10), 512 * MIB);
    assert_eq!(gms(0x11), 8 * MIB); // 0x11..0x16: 4MB increments from 8MB
    assert_eq!(gms(0x16), 28 * MIB);
    assert_eq!(gms(0x17), 36 * MIB); // 0x17..0x1d: 4MB increments from 36MB
    assert_eq!(gms(0x1d), 60 * MIB);
    // the BDW GMS field (bits 15:8) is not read: bits 15:8 set, GMS 2 in bits 7:3
    assert_eq!(chv_stolen_size(0xff00 | (2 << 3)), 64 * MIB);
}

#[test]
fn chv_stolen_reserved_size_family_every_member() {
    // chv_get_stolen_reserved (i915_gem_stolen.c:352-383), GEN8_STOLEN_RESERVED_* = 0/1/2/3 << 7 (i915_reg.h:139-143)
    assert_eq!(chv_stolen_reserved(0x7f00_0001), Some((0x7f00_0000, MIB))); // 1M is the ZERO encoding: pinned by what it selects
    assert_eq!(chv_stolen_reserved(0x7f00_0081), Some((0x7f00_0000, 2 * MIB)));
    assert_eq!(chv_stolen_reserved(0x7f00_0101), Some((0x7f00_0000, 4 * MIB)));
    assert_eq!(chv_stolen_reserved(0x7f00_0181), Some((0x7f00_0000, 8 * MIB)));
    assert_eq!(chv_stolen_reserved(0x7f00_0180), None, "GEN6_STOLEN_RESERVED_ENABLE (bit 0) clear: nothing reserved, :362-363");
}

#[test]
fn decode_stolen_for_cherryview() {
    // GMS 2 = 64 MiB at BSM 0x7b80_0000 (gen3_stolen_base, early-quirks.c:339): top 0x7f80_0000
    let ctl = 2 << 3;
    let bsm = 0x7b80_0001;
    let s = decode_stolen_for(&chv(), ctl, bsm, 0x7f00_0181).unwrap();
    assert_eq!(s, Stolen { base: 0x7b80_0000, size: 64 * MIB, reserved: Some((0x7f00_0000, 8 * MIB)), usable: 56 * MIB });
    assert_eq!(decode_stolen_for(&chv(), ctl, bsm, 0).unwrap().usable, 64 * MIB);
    // reserved_base == stolen_top is "no reserved stolen" (init_reserved_stolen, i915_gem_stolen.c:503-504)
    assert_eq!(decode_stolen_for(&chv(), ctl, bsm, 0x7f80_0001).unwrap().reserved, None);
    // a reservation that runs past the top, and one at base 0 (:506-524): refused by name
    assert_eq!(
        decode_stolen_for(&chv(), ctl, bsm, 0x7f40_0181),
        Err(MemError::StolenReservedOutside { reserved_base: 0x7f40_0000, base: 0x7b80_0000, top: 0x7f80_0000 })
    );
    assert_eq!(decode_stolen_for(&chv(), ctl, bsm, 0x0000_0001).unwrap_err().name(), "STOLEN-RESERVED-OUTSIDE");
    // `!reserved_base` (:506) on its own: stolen at BSM 0, so a reservation at 0 is inside the range and only that rule refuses it
    assert_eq!(
        decode_stolen_for(&chv(), ctl, 0, 0x0000_0001),
        Err(MemError::StolenReservedOutside { reserved_base: 0, base: 0, top: 64 * MIB })
    );
    // Gen9 keeps its arm (gen9_stolen_size: GMS in bits 15:8)
    assert_eq!(decode_stolen_for(&kbl(), 2 << 8, bsm, 0).unwrap().size, 64 * MIB);
    assert_eq!(decode_stolen_for(&gen7(), ctl, bsm, 0), Err(MemError::UnsupportedGeneration { gen: 7 }));
}

// ---- private PAT ----------------------------------------------------------------------------------------------------------

#[test]
fn chv_private_ppat_is_snoop_on_wb_entries() {
    // chv_setup_private_ppat (intel_gtt.c:655-662): SNOOP on 0 and 4..7, 0 on 1..3; CHV_PPAT_SNOOP = bit 6 (intel_gtt.h:140)
    assert_eq!(CHV_PPAT_SNOOP, 0x40);
    assert_eq!(CHV_PPAT, [0x40, 0x00, 0x00, 0x00, 0x40, 0x40, 0x40, 0x40]);
    assert_eq!(private_ppat_for(&chv()), Ok(0x4040_4040_0000_0040));
    assert_eq!(private_ppat_for(&kbl()), Ok(0x3b2b_1b0b_0003_0907), "Gen9 non-LP keeps bdw_setup_private_ppat");
    assert_eq!(private_ppat_for(&gen7()), Err(MemError::UnsupportedGeneration { gen: 7 }));
}

// ---- the 3-level 32-bit PPGTT ---------------------------------------------------------------------------------------------

#[test]
fn layout_is_chosen_by_ppgtt_size() {
    // chv_info.__runtime.ppgtt_size = 32 (i915_pci.c:462); i915_vm_is_4lvl = (total - 1) >> 32 (intel_gtt.h:428-431)
    assert_eq!(PpgttLayout::of(&chv()), Ok(PpgttLayout::Legacy32));
    assert_eq!((PpgttLayout::Legacy32.top(), PpgttLayout::Legacy32.bits()), (2, 32)); // vm->top = 2, gen8_ppgtt.c:1014
    assert_eq!(PpgttLayout::of(&kbl()), Ok(PpgttLayout::Full48));
    assert_eq!((PpgttLayout::Full48.top(), PpgttLayout::Full48.bits()), (3, 48));
    assert_eq!((GEN8_3LVL_PDPES, PPGTT_LEGACY_BITS), (4, 32)); // intel_gtt.h:133, i915_pci.c:462
}

// the four page directories the caller supplies (PpgttInit::pd), one per PDP register
const PD: [u64; 4] = [0x100000, 0x101000, 0x102000, 0x103000];

fn chv_init() -> PpgttInit {
    PpgttInit { pml4: PML4, scratch: S, pd: PD }
}

fn chv_space() -> (Gtt, UnbindPlan, Pool) {
    let (g, p) = Gtt::new_ppgtt(&chv(), &chv_init()).unwrap();
    (g, p, Pool::new(0x104000))
}

#[test]
fn creation_preallocates_four_page_directories() {
    let (g, plan, _) = chv_space();
    assert_eq!(g.total(), 1 << 32);
    assert_eq!(g.ppgtt_layout(), Some(PpgttLayout::Legacy32));
    // gen8_init_scratch with vm->top = 2 makes scratch[0..=2] only (gen8_ppgtt.c:862): scratch[3] encodes nothing
    assert_eq!(g.scratch_chain(), [0x11000 | 0x19, 0x12000 | 0x1b, 0x13000 | 0x1b, 0]);
    // a 3-level base is the top table, four scratch pages and the four PDs (gen8_preallocate_top_level_pdp, gen8_ppgtt.c:901-916)
    assert_eq!((PpgttLayout::Legacy32.base_pages(), PpgttLayout::Full48.base_pages()), (9, 5));
    assert_eq!(g.table_count(), 5, "the top PDP and its four PDs");
    assert_eq!(
        plan.ops(),
        [
            Op::FillTable { table: S[1], entry: 0x11019 }, // fill_px(scratch[1], scratch[0]->encode), :877
            Op::FillTable { table: S[2], entry: 0x1201b },
            Op::FillTable { table: PML4, entry: 0x1301b }, // the top PDP points at scratch[2] (gen8_alloc_top_pd, :948)
            Op::FillTable { table: PD[0], entry: 0x1201b }, // fill_px(pde, scratch[1]->encode), :914
            Op::TableEntry { table: PML4, index: 0, value: PD[0] | 0x3 }, // set_pd_entry: pde(LLC) = addr | P | RW (intel_ppgtt.c:109)
            Op::FillTable { table: PD[1], entry: 0x1201b },
            Op::TableEntry { table: PML4, index: 1, value: PD[1] | 0x3 },
            Op::FillTable { table: PD[2], entry: 0x1201b },
            Op::TableEntry { table: PML4, index: 2, value: PD[2] | 0x3 },
            Op::FillTable { table: PD[3], entry: 0x1201b },
            Op::TableEntry { table: PML4, index: 3, value: PD[3] | 0x3 },
        ]
    );
    // the context names the four PDs (init_ppgtt_regs ASSIGN_CTX_PDP(3..0), intel_lrc.c:889-903)
    assert_eq!(g.ppgtt_roots(), Some(PpgttRoots::Pdp(PD)));
}

#[test]
fn a_4_level_space_names_its_pml4() {
    let (g, _) = Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S, pd: PD }).unwrap();
    assert_eq!(g.ppgtt_roots(), Some(PpgttRoots::Pml4(PML4)), "a 4-level space never links pd");
    assert_eq!(g.table_count(), 1);
    assert_eq!(g.ppgtt_layout(), Some(PpgttLayout::Full48));
    let (gg, _) = Gtt::new_ggtt(&kbl(), &geom_4g(), 0x5000).unwrap();
    assert_eq!((gg.ppgtt_roots(), gg.ppgtt_layout()), (None, None));
}

#[test]
fn creation_refuses_bad_page_directories() {
    // each PD is validated like every other page: aligned, inside the DMA mask (39 bits, i915_pci.c:460), aliasing nothing
    let with = |pd: [u64; 4]| Gtt::new_ppgtt(&chv(), &PpgttInit { pml4: PML4, scratch: S, pd }).err();
    assert_eq!(with([PD[0], PD[1], PD[2], PML4]), Some(MemError::DuplicatePage { addr: PML4 }));
    assert_eq!(with([PD[0], PD[1], PD[2], S[3]]), Some(MemError::DuplicatePage { addr: S[3] }));
    assert_eq!(with([PD[0], PD[1], PD[0], PD[3]]), Some(MemError::DuplicatePage { addr: PD[0] }));
    assert_eq!(with([PD[0], PD[1], PD[2], PD[3] + 8]).map(|e| e.name()), Some("MISALIGNED"));
    assert_eq!(with([PD[0], PD[1], PD[2], 1 << 39]), Some(MemError::DeviceAddrBeyondMask { addr: 1 << 39, bits: 39 }));
    assert_eq!(Gtt::new_ppgtt(&gen7(), &chv_init()).err(), Some(MemError::UnsupportedGeneration { gen: 7 }));
    // a 4-level space ignores pd entirely (zeros are not validated)
    assert!(Gtt::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S, pd: [0; 4] }).is_ok());
}

fn one_page_at(pages: &[u64], at: u64) -> BindRequest<'_> {
    BindRequest { pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place: Placement::with_mode(Mode::Fixed(at)) }
}

#[test]
fn a_bind_makes_only_the_page_table() {
    let (mut g, _, mut pool) = chv_space();
    let pages = [0x40_0000u64];
    let plan = g.plan_bind(&mut pool, &one_page_at(&pages, 0xC000_0000)).unwrap();
    // the PD for 3..4 GiB exists; only a PT is asked for (level 0, base 0xC000_0000) and linked at PD[3] entry 0
    assert_eq!(pool.asked, [TablePageRequest { level: 0, base_va: 0xC000_0000 }]);
    assert_eq!(
        plan.ops(),
        [
            Op::FillTable { table: 0x104000, entry: 0x11019 },
            Op::TableEntry { table: PD[3], index: 0, value: 0x104003 },
            Op::PteRun { table: 0x104000, index: 0, count: 1, page_offset: 0 },
        ]
    );
    // walk: no PML4 level; PDP index 3, PD index 1, PT index 1 for 0xC020_1000
    let w = g.walk(0xC020_1000).unwrap();
    assert_eq!(w.index, [0, 3, 1, 1]);
    assert_eq!(w.table, [None, Some(PML4), Some(PD[3]), None]);
    assert_eq!(g.walk(0xC000_0000).unwrap().table, [None, Some(PML4), Some(PD[3]), Some(0x104000)]);
}

#[test]
fn the_32_bit_limit_is_named() {
    let (mut g, _, mut pool) = chv_space();
    let pages = [0x40_0000u64, 0x40_1000];
    assert!(g.plan_bind(&mut pool, &one_page_at(&pages[..1], (1 << 32) - 4096)).is_ok());
    assert_eq!(g.plan_bind(&mut pool, &one_page_at(&pages[..1], 1 << 32)).err(), Some(MemError::Beyond32Bit { va: 1 << 32 }));
    assert_eq!(g.plan_bind(&mut pool, &one_page_at(&pages, (1 << 32) - 4096)).err(), Some(MemError::Beyond32Bit { va: 1 << 32 }));
    assert_eq!(g.walk(1 << 32).err(), Some(MemError::Beyond32Bit { va: 1 << 32 }));
    assert_eq!(g.check_access((1 << 32) - 4096, 8192).err(), Some(MemError::Beyond32Bit { va: 1 << 32 }));
    assert_eq!(MemError::Beyond32Bit { va: 0 }.name(), "VA-BEYOND-32BIT");
}
