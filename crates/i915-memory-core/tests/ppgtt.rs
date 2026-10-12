// SPDX-License-Identifier: GPL-2.0-only
//! The 4-level 48-bit PPGTT: creation (scratch chain), the PML4 walk at every level boundary, the 48-bit limit, stale mappings,
//! guard/scratch presence, table reuse and the table-page source contract.
mod common;
use common::*;
use i915_memory_core::*;

type P = Gtt<128, 1024>;

fn init() -> (P, UnbindPlan) {
    P::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: S }).unwrap()
}

fn fixed(pages: &[u64], at: u64) -> BindRequest<'_> {
    BindRequest {
        pages,
        size: pages.len() as u64 * 4096,
        alignment: 0,
        guard: 0,
        cache: CacheLevel::Llc,
        flags: 0,
        place: Placement::with_mode(Mode::Fixed(at)),
    }
}

fn tw(device_addr: u64, byte_offset: u16, value: u64) -> Write {
    Write { target: Target::Table { device_addr, byte_offset }, value }
}

#[test]
fn creation_builds_the_scratch_chain_gen8_init_scratch_style() {
    let (g, plan) = init();
    // scratch[0]->encode = pte(S0, NONE, RO)  = S0 | P | PWT|PCD (RW cleared)  (gen8_ppgtt.c:856, has_read_only on Gen9)
    // scratch[i]->encode = pde(Si, NONE)      = Si | P | RW | PWT|PCD          (gen8_ppgtt.c:878)
    assert_eq!(g.scratch_chain(), [0x11000 | 0x19, 0x12000 | 0x1b, 0x13000 | 0x1b, 0x14000 | 0x1b]);
    assert_eq!(g.scratch_entry(), 0x11019);
    assert_eq!(g.total(), 1 << 48);
    assert_eq!(g.table_count(), 1, "the PML4 is table #0");
    assert_eq!(
        plan.ops(),
        [
            Op::FillTable { table: S[1], entry: 0x11019 }, // fill_px(scratch[1], scratch[0]->encode)
            Op::FillTable { table: S[2], entry: 0x1201b },
            Op::FillTable { table: S[3], entry: 0x1301b },
            Op::FillTable { table: PML4, entry: 0x1401b }, // the top level points at scratch[3]
        ]
    );
    let w: Vec<Write> = plan.writes().collect();
    assert_eq!(w.len(), 4 * 512, "every entry of every scratch table and the PML4 is written");
    assert_eq!(w[0], tw(S[1], 0, 0x11019));
    assert_eq!(w[511], tw(S[1], 511 * 8, 0x11019));
    assert_eq!(w[512], tw(S[2], 0, 0x1201b));
    assert_eq!(w[4 * 512 - 1], tw(PML4, 511 * 8, 0x1401b));
    let f: Vec<Flush> = plan.flushes().collect();
    assert_eq!(f.last(), Some(&Flush::PpgttTlb));
    assert_eq!(f.len(), 5, "one CPU-cache flush per filled page, then the TLB step");
}

#[test]
fn creation_refuses_wrong_hardware_and_aliased_pages() {
    let mut d = kbl();
    d.info.ppgtt_size = 32;
    assert_eq!(P::new_ppgtt(&d, &PpgttInit { pml4: PML4, scratch: S }).err(), Some(MemError::NotFullPpgtt { size_bits: 32 }));
    let chv = i915_devinfo_core::lookup(0x22b0).unwrap();
    assert_eq!(P::new_ppgtt(&chv, &PpgttInit { pml4: PML4, scratch: S }).err(), Some(MemError::UnsupportedGeneration { gen: 8 }));
    assert_eq!(P::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4, scratch: [S[0], S[1], S[2], PML4] }).err(), Some(MemError::DuplicatePage { addr: PML4 }));
    assert_eq!(P::new_ppgtt(&kbl(), &PpgttInit { pml4: PML4 + 8, scratch: S }).err().map(|e| e.name()), Some("MISALIGNED"));
    assert_eq!(P::new_ppgtt(&kbl(), &PpgttInit { pml4: 1 << 39, scratch: S }).err(), Some(MemError::DeviceAddrBeyondMask { addr: 1 << 39, bits: 39 }));
}

#[test]
fn pml4_walk_hits_the_right_entry_at_every_level_boundary() {
    // (va, [pml4, pdp, pd, pt] indices) - gen8_pd_index for 47:39 | 38:30 | 29:21 | 20:12 (intel_gtt.h:123-131)
    let cases: [(u64, [u16; 4]); 11] = [
        (0x0000_0000_0000, [0, 0, 0, 0]),
        (0x0000_0000_1000, [0, 0, 0, 1]),           // second page
        (0x0000_001f_f000, [0, 0, 0, 511]),         // last page of the first PT
        (0x0000_0020_0000, [0, 0, 1, 0]),           // 2 MiB: next PD entry
        (0x0000_3fe0_0000, [0, 0, 511, 0]),         // last PD entry
        (0x0000_4000_0000, [0, 1, 0, 0]),           // 1 GiB: next PDP entry
        (0x007f_c000_0000, [0, 511, 0, 0]),         // last PDP entry
        (0x0080_0000_0000, [1, 0, 0, 0]),           // 512 GiB: next PML4 entry
        (0x7f80_0000_0000, [255, 0, 0, 0]),         // middle of the PML4
        (0xff80_0000_0000, [511, 0, 0, 0]),         // last PML4 entry
        (0xffff_ffff_f000, [511, 511, 511, 511]),   // the last page of the space
    ];
    for (va, idx) in cases {
        let (mut g, _) = init();
        let mut src = Pool::new(POOL0);
        let pages = [0x2_0000_0000u64];
        let plan = g.plan_bind(&mut src, &fixed(&pages, va)).unwrap();
        let wk = g.walk(va).unwrap();
        assert_eq!(wk.index, idx, "va {va:#x}");
        // the three pages the source gave, in level order PDP, PD, PT
        let (pdp, pd, pt) = (POOL0, POOL0 + 0x1000, POOL0 + 0x2000);
        assert_eq!(wk.table, [Some(PML4), Some(pdp), Some(pd), Some(pt)], "va {va:#x}");
        assert_eq!(src.asked.len(), 3);
        assert_eq!(src.asked[0], TablePageRequest { level: 2, base_va: va & !((1 << 39) - 1) });
        assert_eq!(src.asked[1], TablePageRequest { level: 1, base_va: va & !((1 << 30) - 1) });
        assert_eq!(src.asked[2], TablePageRequest { level: 0, base_va: va & !((1 << 21) - 1) });
        // the exact stores: fill PDP, link PML4->PDP, fill PD, link PDP->PD, fill PT, link PD->PT, then the PTE (pat 0x80 = Llc)
        let w: Vec<Write> = plan.writes().collect();
        let links: Vec<Write> = plan
            .ops()
            .iter()
            .filter_map(|o| if let Op::TableEntry { table, index, value } = *o { Some(tw(table, index * 8, value)) } else { None })
            .collect();
        assert_eq!(
            links,
            [
                tw(PML4, idx[0] * 8, pdp | 0x3), // pde(.., Llc) = addr | PRESENT | RW
                tw(pdp, idx[1] * 8, pd | 0x3),
                tw(pd, idx[2] * 8, pt | 0x3),
            ],
            "va {va:#x}"
        );
        assert_eq!(*w.last().unwrap(), tw(pt, idx[3] * 8, 0x2_0000_0000 | 0x83), "va {va:#x}: the leaf PTE");
        // fill-before-link: each table's fill precedes the link that publishes it
        let pos = |o: Op| plan.ops().iter().position(|x| *x == o).unwrap();
        assert!(pos(Op::FillTable { table: pdp, entry: g.scratch_chain()[2] }) < pos(Op::TableEntry { table: PML4, index: idx[0], value: pdp | 3 }));
        assert!(pos(Op::FillTable { table: pt, entry: g.scratch_chain()[0] }) < pos(Op::TableEntry { table: pd, index: idx[2], value: pt | 3 }));
    }
}

#[test]
fn new_tables_are_filled_with_the_scratch_entry_of_their_own_level() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    let plan = g.plan_bind(&mut src, &fixed(&[0x1000], 0x4000_0000)).unwrap();
    let chain = g.scratch_chain();
    // gen8_ppgtt.c:351 fill_px(pt, vm->scratch[lvl]->encode): PDP <- scratch[2], PD <- scratch[1], PT <- scratch[0]
    assert_eq!(plan.ops()[0], Op::FillTable { table: POOL0, entry: chain[2] });
    assert_eq!(plan.ops()[2], Op::FillTable { table: POOL0 + 0x1000, entry: chain[1] });
    assert_eq!(plan.ops()[4], Op::FillTable { table: POOL0 + 0x2000, entry: chain[0] });
    assert_eq!(plan.new_tables(), 3);
}

#[test]
fn the_48_bit_limit_is_refused_by_name_everywhere() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    let p = [0x1000u64];
    for va in [1u64 << 48, (1 << 48) + 0x1000, 0xffff_8000_0000_0000, u64::MAX & !0xfff] {
        assert_eq!(g.plan_bind(&mut src, &fixed(&p, va)).err(), Some(MemError::Beyond48Bit { va }), "fixed {va:#x}");
        assert_eq!(g.walk(va).err(), Some(MemError::Beyond48Bit { va }));
        assert_eq!(g.check_access(va, 1).err(), Some(MemError::Beyond48Bit { va }));
    }
    let mut r = fixed(&p, 0);
    r.place = Placement { mode: Mode::Best, bias: 1 << 48, mappable: false, zone_4g: false, evict: false };
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::Beyond48Bit { va: 1 << 48 }));
    // ...while the last page of the space is fine, and a range running off the end is a different, named error
    assert!(g.plan_bind(&mut src, &fixed(&p, (1 << 48) - 4096)).is_ok());
    let two = [0x1000u64, 0x2000];
    assert_eq!(
        g.plan_bind(&mut src, &fixed(&two, (1 << 48) - 4096)).err(),
        Some(MemError::Beyond48Bit { va: 1 << 48 }),
        "a range that crosses bit 48 is refused at the first address past it"
    );
    assert_eq!(g.check_access((1 << 48) - 4096, 8192).err(), Some(MemError::Beyond48Bit { va: 1 << 48 }));
    assert_eq!(MemError::Beyond48Bit { va: 0 }.name(), "VA-BEYOND-48BIT");
    assert!(src.asked.len() <= 3, "refusals asked for no pages");
}

#[test]
fn a_stale_mapping_is_refused_after_unbind() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    let pages = [0x3000u64, 0x4000, 0x5000];
    let plan = g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&pages, 0) }).unwrap();
    let (id, va) = (plan.vma(), plan.offset());
    assert_eq!(g.check_access(va, 3 * 4096), Ok(id));
    assert_eq!(g.check_access(va + 4096, 1), Ok(id));
    assert_eq!(g.check_access(va + 3 * 4096 - 1, 1), Ok(id));
    assert_eq!(g.check_access(va + 3 * 4096, 1), Err(MemError::StaleMapping { va: va + 3 * 4096 }), "one byte past the bound pages");
    assert_eq!(g.check_access(va, 3 * 4096 + 1), Err(MemError::StaleMapping { va: va + 3 * 4096 }), "a range running out of the mapping");
    assert_eq!(g.check_access(va, 0), Err(MemError::ZeroLength));

    let un = g.plan_unbind(id).unwrap();
    assert_eq!(g.check_access(va, 1), Err(MemError::StaleMapping { va }), "unbind then access");
    assert_eq!(g.check_access(va + 4096, 1), Err(MemError::StaleMapping { va: va + 4096 }));
    assert_eq!(g.plan_unbind(id).err(), Some(MemError::NotBound { id: id.0 }), "a second unbind of the same id");
    assert_eq!(g.set_pinned(id, true).err(), Some(MemError::NotBound { id: id.0 }));
    // the unbind itself: retire first, then the whole range back to scratch, then the TLB step
    assert_eq!(un.ops()[0], Op::AwaitRetire { vma: id });
    assert!(un.writes().all(|w| w.value == g.scratch_entry()), "every store of an unbind is the scratch PTE");
    assert_eq!(un.writes().count(), 3);
    assert_eq!(un.flushes().last(), Some(Flush::PpgttTlb));
    // the old address stays stale after the space is reused elsewhere
    let again = g.plan_bind(&mut src, &BindRequest { place: Placement::with_mode(Mode::High), ..fixed(&pages, 0) }).unwrap();
    assert_ne!(again.vma(), id, "ids are never reused");
    assert_eq!(g.check_access(va, 1).is_err(), again.offset() != va);
}

#[test]
fn guard_pages_and_the_scratch_tail_are_written_as_scratch() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    let pages = [0x7000u64, 0x8000];
    // 2 real pages inside a 3-page data area with a 2-page guard each side, placed at exactly 0x10_0000 + guard
    let req = BindRequest {
        pages: &pages,
        size: 3 * 4096,
        alignment: 0,
        guard: 2 * 4096,
        cache: CacheLevel::None,
        flags: PTE_READ_ONLY_FLAG,
        place: Placement::with_mode(Mode::Fixed(0x10_0000 + 0x2000)),
    };
    let plan = g.plan_bind(&mut src, &req).unwrap();
    assert_eq!(plan.node(), (0x10_0000, 7 * 4096), "guard + data + guard");
    assert_eq!(plan.offset(), 0x10_2000);
    let pt = POOL0 + 0x2000;
    let sc = g.scratch_entry();
    // the PT page's entries 256.. (0x10_0000 >> 12 = 256): scratch, scratch, data, data, scratch, scratch, scratch
    let pte_writes: Vec<Write> = plan
        .ops()
        .iter()
        .filter(|o| matches!(o, Op::PteRun { .. } | Op::PteFill { .. }))
        .flat_map(|o| BindPlanOps::expand(&plan, o))
        .collect();
    let ro_none = |a: u64| a | 0x1 | 0x18; // read-only, uncached
    assert_eq!(
        pte_writes,
        [
            tw(pt, 256 * 8, sc),
            tw(pt, 257 * 8, sc),
            tw(pt, 258 * 8, ro_none(0x7000)),
            tw(pt, 259 * 8, ro_none(0x8000)),
            tw(pt, 260 * 8, sc),
            tw(pt, 261 * 8, sc),
            tw(pt, 262 * 8, sc),
        ]
    );
    // the model agrees: guard is not accessible, the tail is not accessible, the data is
    assert_eq!(g.check_access(0x10_0000, 1), Err(MemError::GuardPageAccess { va: 0x10_0000 }));
    assert_eq!(g.check_access(0x10_1fff, 1), Err(MemError::GuardPageAccess { va: 0x10_1fff }));
    assert_eq!(g.check_access(0x10_2000, 8192), Ok(plan.vma()));
    assert_eq!(g.check_access(0x10_4000, 1), Err(MemError::StaleMapping { va: 0x10_4000 }), "the scratch tail");
    assert_eq!(g.check_access(0x10_5000, 1), Err(MemError::GuardPageAccess { va: 0x10_5000 }));
    assert_eq!(g.offset_of(plan.vma()), Some((0x10_2000, 8192, 8192)));
}

const PTE_READ_ONLY_FLAG: u32 = i915_memory_core::pte::PTE_READ_ONLY;

/// Expand one op of a plan to its stores (the public `writes()` flattens the whole plan; this picks one op out).
struct BindPlanOps;
impl BindPlanOps {
    fn expand(plan: &BindPlan<'_>, op: &Op) -> Vec<Write> {
        let all: Vec<Write> = plan.writes().collect();
        let mut at = 0usize;
        for o in plan.ops() {
            let n = o.entries() as usize;
            if o == op {
                return all[at..at + n].to_vec();
            }
            at += n;
        }
        unreachable!()
    }
}

#[test]
fn emptied_tables_are_reused_and_cost_nothing_to_rebind() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    let pages = [0x3000u64];
    let a = g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&pages, 0) }).unwrap();
    assert_eq!(a.new_tables(), 3);
    let tables = g.table_count();
    g.plan_unbind(a.vma()).unwrap();
    assert_eq!(g.table_count(), tables, "hal Dma has no free: empty tables are kept");
    let asked = src.asked.len();
    let b = g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&pages, 0) }).unwrap();
    assert_eq!(b.new_tables(), 0);
    assert_eq!(src.asked.len(), asked, "the source is not asked again");
    assert!(b.ops().iter().all(|o| matches!(o, Op::PteRun { .. } | Op::PteFill { .. })), "only leaf stores");
}

#[test]
fn a_refusing_source_leaves_the_model_unchanged_and_parks_the_pages_it_did_give() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    src.limit = 2; // PDP and PD are granted, the PT is refused
    let nodes = g.live_nodes();
    let e = g.plan_bind(&mut src, &fixed(&[0x1000], 0x4000_0000)).err();
    assert_eq!(e, Some(MemError::TablePageRefused { level: 0 }));
    assert_eq!((g.live_nodes(), g.table_count()), (nodes, 1), "no node, no table survives a failed plan");
    assert_eq!(g.spare_pages(), 2, "the two granted pages are parked, not dropped");
    assert_eq!(g.leaked_pages(), 0);
    let mut good = Pool::new(0x900000);
    let plan = g.plan_bind(&mut good, &fixed(&[0x1000], 0x4000_0000)).unwrap();
    assert_eq!(good.asked.len(), 1, "the spare pool is consumed first; only the third page is asked for");
    assert_eq!(g.spare_pages(), 0);
    assert_eq!(plan.new_tables(), 3);
    // NoTablePages refuses the very first table
    let (mut g2, _) = init();
    assert_eq!(g2.plan_bind(&mut NoTablePages, &fixed(&[0x1000], 0)).err(), Some(MemError::TablePageRefused { level: 2 }));
}

#[test]
fn unusable_pages_from_the_source_are_refused() {
    let one = |addr: u64| move |_r: TablePageRequest| Ok(TablePage { device_addr: addr });
    for (addr, why) in [(PML4, "aliases the PML4"), (S[2], "aliases a scratch page"), (POOL0 + 8, "unaligned"), (1u64 << 39, "beyond the DMA mask")] {
        let (mut g, _) = init();
        assert_eq!(g.plan_bind(&mut one(addr), &fixed(&[0x1000], 0)).err(), Some(MemError::TablePageRefused { level: 2 }), "{why}");
        assert_eq!(g.table_count(), 1);
    }
    // the same page twice in one plan
    let (mut g, _) = init();
    assert_eq!(g.plan_bind(&mut one(POOL0), &fixed(&[0x1000], 0)).err(), Some(MemError::TablePageRefused { level: 1 }));
}

#[test]
fn device_addresses_must_fit_the_dma_mask_and_be_aligned() {
    let (mut g, _) = init();
    let mut src = Pool::new(POOL0);
    assert!(g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&[(1 << 39) - 4096], 0) }).is_ok(), "the last page below 2^39");
    assert_eq!(
        g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&[1 << 39], 0) }).err(),
        Some(MemError::DeviceAddrBeyondMask { addr: 1 << 39, bits: 39 })
    );
    assert_eq!(g.plan_bind(&mut src, &BindRequest { place: Placement::best(), ..fixed(&[0x1008], 0) }).err().map(|e| e.name()), Some("MISALIGNED"));
    let mut r = fixed(&[0x1000], 0);
    r.flags = i915_memory_core::pte::PTE_LM;
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::UnsupportedFlag { flag: 2 }));
    r.flags = 0;
    r.size = 100;
    assert_eq!(g.plan_bind(&mut src, &r).err().map(|e| e.name()), Some("MISALIGNED"));
    r.size = 4096;
    r.pages = &[0x1000, 0x2000];
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::PagesExceedNode { pages: 2, node_pages: 1 }));
    r.pages = &[];
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::ZeroLength));
    r.pages = &[0x1000];
    r.alignment = 3 * 4096;
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::BadAlignment { align: 3 * 4096 }));
    r.alignment = 2048;
    assert_eq!(g.plan_bind(&mut src, &r).err(), Some(MemError::BadAlignment { align: 2048 }));
    assert_eq!(src.asked.len() <= 3, true);
}
