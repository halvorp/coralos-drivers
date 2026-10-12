// SPDX-License-Identifier: GPL-2.0-only
//! The Global GTT planner: creation, PTE stores at BAR0+8 MiB, placement modes, the aperture, guard pages, fixed offsets,
//! LRU eviction with retirement, pinning, and the table-capacity bound.
mod common;
use common::*;
use i915_memory_core::regs::{GFX_FLSH_CNTL_EN, GFX_FLSH_CNTL_GEN6};
use i915_memory_core::*;

type G = Gtt;

const SCRATCH: u64 = 0x9000;
const GIB: u64 = 1 << 30;
const END: u64 = (4 << 30) - 4096; // allocation ends one page before the top: the guard page

fn ggtt() -> (Gtt, UnbindPlan) {
    Gtt::new_ggtt(&kbl(), &geom_4g(), SCRATCH).unwrap()
}

fn req(pages: &[u64], size: u64, place: Placement) -> BindRequest<'_> {
    BindRequest { pages, size, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place }
}

fn gw(entry: u32, value: u64) -> Write {
    Write { target: Target::Ggtt { mmio_offset: 0x80_0000 + entry * 8 }, value }
}

#[test]
fn creation_clears_every_entry_to_scratch_and_reserves_the_error_capture_slot() {
    let (g, plan) = G::new_ggtt(&kbl(), &geom_4g(), SCRATCH).unwrap();
    assert_eq!(g.total(), 4 << 30);
    assert_eq!(g.scratch_entry(), 0x9001, "scratch[0]->encode = pte_encode(scratch dma) = addr | PRESENT");
    assert_eq!(plan.ops(), [Op::GgttFill { first: 0, count: 1 << 20, value: 0x9001 }]);
    assert_eq!(plan.writes().count(), 1 << 20);
    assert_eq!(plan.writes().last(), Some(gw((1 << 20) - 1, 0x9001)), "the last page (the guard page) is scratch too");
    assert_eq!(g.live_nodes(), 1, "the 2-page error-capture slot");
    // gen8_ggtt_invalidate on Gen9 non-LP: one uncached write to GFX_FLSH_CNTL_GEN6 (intel_ggtt.c:217-230)
    let f: Vec<Flush> = plan.flushes().collect();
    assert_eq!(f, [Flush::GgttInvalidate(RegWrite { reg: GFX_FLSH_CNTL_GEN6, value: GFX_FLSH_CNTL_EN })]);
    assert_eq!(GFX_FLSH_CNTL_GEN6.offset(), 0x101008);
}

#[test]
fn creation_refuses_other_generations_and_bad_scratch() {
    let chv = i915_devinfo_core::lookup(0x22b0).unwrap();
    assert_eq!(G::new_ggtt(&chv, &geom_4g(), SCRATCH).err(), Some(MemError::UnsupportedGeneration { gen: 8 }));
    assert_eq!(G::new_ggtt(&kbl(), &geom_4g(), SCRATCH + 8).err().map(|e| e.name()), Some("MISALIGNED"));
    assert_eq!(G::new_ggtt(&kbl(), &geom_4g(), 1 << 39).err(), Some(MemError::DeviceAddrBeyondMask { addr: 1 << 39, bits: 39 }));
}

#[test]
fn bind_stores_one_pte_per_page_at_gtt_base_plus_8_bytes_each() {
    let (mut g, _) = ggtt();
    let pages = [0x10_0000u64, 0x30_0000, 0x20_0000];
    let p = g.plan_bind(&mut NoTablePages, &req(&pages, 3 * 4096, Placement::best())).unwrap();
    // first hole is after the 2-page error-capture slot: GGTT address 0x2000 = entry 2 = BAR0 + 0x800000 + 16
    assert_eq!(p.offset(), 0x2000);
    let w: Vec<Write> = p.writes().collect();
    assert_eq!(w, [gw(2, 0x10_0001), gw(3, 0x30_0001), gw(4, 0x20_0001)], "PTE = page | PRESENT, no cache bits, no RW");
    assert_eq!(w[0].target, Target::Ggtt { mmio_offset: 0x80_0010 });
    // a different cache level or RO flag changes nothing in the GGTT PTE (gen8_ggtt_insert_entries ignores them)
    let (mut g2, _) = ggtt();
    let mut r = req(&pages, 3 * 4096, Placement::best());
    r.cache = CacheLevel::None;
    r.flags = i915_memory_core::pte::PTE_READ_ONLY;
    let w2: Vec<Write> = g2.plan_bind(&mut NoTablePages, &r).unwrap().writes().collect();
    assert_eq!(w, w2);
    // the TLB step comes last, and only after every store
    assert_eq!(p.flushes().last(), Some(Flush::GgttInvalidate(RegWrite { reg: GFX_FLSH_CNTL_GEN6, value: 1 })));
    assert_eq!(p.new_tables(), 0);
}

#[test]
fn guard_pages_and_the_scratch_tail_are_scratch_entries() {
    let (mut g, _) = ggtt();
    let pages = [0x10_0000u64];
    let mut r = req(&pages, 3 * 4096, Placement::with_mode(Mode::Fixed(0x10_0000)));
    r.guard = 4096;
    let p = g.plan_bind(&mut NoTablePages, &r).unwrap();
    assert_eq!(p.node(), (0xf_f000, 5 * 4096));
    assert_eq!(p.offset(), 0x10_0000);
    let e = |n: u32| 0xf_f000 / 4096 + n;
    assert_eq!(
        p.writes().collect::<Vec<_>>(),
        [gw(e(0), 0x9001), gw(e(1), 0x10_0001), gw(e(2), 0x9001), gw(e(3), 0x9001), gw(e(4), 0x9001)],
        "guard, the one real page, two scratch pages of tail, guard"
    );
}

#[test]
fn guard_is_rounded_up_to_the_alignment_as_i915_vma_insert_does() {
    let (mut g, _) = ggtt();
    let pages = [0x10_0000u64];
    let mut r = req(&pages, 4096, Placement::best());
    r.alignment = 64 << 10;
    r.guard = 4096; // -> 64 KiB
    let p = g.plan_bind(&mut NoTablePages, &r).unwrap();
    assert_eq!(p.node().1, 4096 + 2 * (64 << 10));
    assert_eq!(p.node().0 % (64 << 10), 0, "the node is aligned");
    assert_eq!(p.offset() % (64 << 10), 0, "and so is the data, because guard is a multiple of the alignment");
    assert_eq!(g.offset_of(p.vma()), Some((p.offset(), 4096, 64 << 10)));
}

#[test]
fn placement_modes_low_high_best_and_the_aperture() {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    // HIGH: ends exactly at the guard page, which is never allocated
    let h = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::with_mode(Mode::High))).unwrap();
    assert_eq!(h.offset(), END - 4096);
    assert_eq!(g.check_access(END, 1), Err(MemError::StaleMapping { va: END }), "the last page is scratch, not mapped");
    // LOW + mappable stays under the 256 MiB aperture
    let mut p = Placement::with_mode(Mode::Low);
    p.mappable = true;
    let l = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, p)).unwrap();
    assert_eq!(l.offset(), 0x2000);
    // a node bigger than the aperture cannot be mappable, but fits the GGTT
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 300 << 20, p)).err(), Some(MemError::NoSpace { size: 300 << 20 }));
    assert!(g.plan_bind(&mut NoTablePages, &req(&pg, 300 << 20, Placement::best())).is_ok());
    // bias: nothing below it
    let mut b = Placement::best();
    b.bias = 3 * GIB;
    assert!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, b)).unwrap().offset() >= 3 * GIB);
    // ZONE_4G is a no-op inside a 4 GiB GGTT but keeps the last page out
    let mut z = Placement::with_mode(Mode::High);
    z.zone_4g = true;
    assert!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, z)).unwrap().offset() + 4096 <= (1 << 32) - 4096);
    // larger than everything: refused, named
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 5 * GIB, Placement::best())).err(), Some(MemError::NoSpace { size: 5 * GIB }));
}

#[test]
fn best_fit_takes_the_smallest_hole_that_fits() {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    let low = Placement::with_mode(Mode::Low);
    let a = g.plan_bind(&mut NoTablePages, &req(&pg, 4 * 4096, low)).unwrap(); // 0x2000..0x6000
    let b = g.plan_bind(&mut NoTablePages, &req(&pg, 8 * 4096, low)).unwrap(); // 0x6000..0xe000
    let c = g.plan_bind(&mut NoTablePages, &req(&pg, 4 * 4096, low)).unwrap(); // 0xe000..0x12000
    let d = g.plan_bind(&mut NoTablePages, &req(&pg, 2 * 4096, low)).unwrap(); // 0x12000..0x14000 (keeps c from merging with the tail)
    assert_eq!((a.offset(), b.offset(), c.offset(), d.offset()), (0x2000, 0x6000, 0xe000, 0x12000));
    g.plan_unbind(a.vma()).unwrap(); // hole of 4 pages
    g.plan_unbind(c.vma()).unwrap(); // hole of 4 pages
    // 3 pages: both 4-page holes fit, the tail is huge -> the smaller hole, lowest first
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 3 * 4096, Placement::best())).unwrap().offset(), 0x2000);
    // 5 pages: no 4-page hole fits; only the tail does
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 5 * 4096, Placement::best())).unwrap().offset(), 0x14000);
    let _ = (b, d);
}

#[test]
fn fixed_offsets_are_checked_and_never_land_on_a_reservation() {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    let fx = |o| Placement::with_mode(Mode::Fixed(o));
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x1000))).err(), Some(MemError::RangeOccupied { start: 0x1000, len: 4096 }), "the error-capture slot");
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x1000).evicting())).err(), Some(MemError::RangeOccupied { start: 0x1000, len: 4096 }), "a reservation is never a victim");
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x10_0800))).err().map(|e| e.name()), Some("MISALIGNED"));
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(END))).err(), Some(MemError::BeyondSpace { va: END, len: 4096, total: END }), "the guard page");
    let ok = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x10_0000))).unwrap();
    assert_eq!(ok.offset(), 0x10_0000);
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx(0x10_0000))).err(), Some(MemError::RangeOccupied { start: 0x10_0000, len: 4096 }));
    // fixed + guard: the data offset is what is fixed; the node starts a guard earlier
    let mut r = req(&pg, 4096, fx(0x20_0000));
    r.guard = 4096;
    assert_eq!(g.plan_bind(&mut NoTablePages, &r).unwrap().node(), (0x1f_f000, 3 * 4096));
}

/// Fill the GGTT with four 1 GiB-ish nodes (pages are few; the node size reserves the space, the tail maps scratch).
fn full() -> (Gtt, [VmaId; 4]) {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    let low = Placement::with_mode(Mode::Low);
    let a = g.plan_bind(&mut NoTablePages, &req(&pg, GIB, low)).unwrap().vma();
    let b = g.plan_bind(&mut NoTablePages, &req(&pg, GIB, low)).unwrap().vma();
    let c = g.plan_bind(&mut NoTablePages, &req(&pg, GIB, low)).unwrap().vma();
    let rest = END - 0x2000 - 3 * GIB;
    let d = g.plan_bind(&mut NoTablePages, &req(&pg, rest, low)).unwrap().vma();
    (g, [a, b, c, d])
}

#[test]
fn a_full_space_refuses_without_eviction_and_evicts_the_least_recently_used_with_a_retire_wait() {
    let (mut g, [a, b, c, d]) = full();
    let pg = [0x20_0000u64; 1];
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, GIB, Placement::best())).err(), Some(MemError::NoSpace { size: GIB }));
    // use order: a, b, c, d are creation-ordered; touching a and c makes b the oldest
    g.touch(a).unwrap();
    g.touch(c).unwrap();
    g.touch(d).unwrap();
    let (b_start, _, _) = g.offset_of(b).unwrap();
    let p = g.plan_bind(&mut NoTablePages, &req(&pg, GIB, Placement::best().evicting())).unwrap();
    assert_eq!(p.evicted(), [b]);
    assert_eq!(p.offset(), b_start, "the new node takes the victim's place");
    // retirement before reuse: wait, scratch the victim's range, then write the new range
    assert_eq!(p.ops()[0], Op::AwaitRetire { vma: b });
    assert_eq!(p.ops()[1], Op::GgttFill { first: (b_start / 4096) as u32, count: (GIB / 4096) as u32, value: 0x9001 });
    assert!(matches!(p.ops()[2], Op::GgttRun { first, count: 1, .. } if first as u64 == b_start / 4096));
    // the victim is gone from the model, the survivors stay
    assert_eq!(g.check_access(b_start, 1), Ok(p.vma()), "the address now belongs to the new node");
    assert!(g.offset_of(b).is_none());
    assert!(g.offset_of(a).is_some() && g.offset_of(c).is_some() && g.offset_of(d).is_some());
}

#[test]
fn eviction_gives_back_victims_the_hole_does_not_need() {
    let (mut g, [a, b, c, d]) = full();
    // age order a < c < b < d: a, then c, then b are taken; a+b are adjacent and enough for 1.5 GiB, so c is given back
    g.touch(c).unwrap();
    g.touch(b).unwrap();
    g.touch(d).unwrap();
    let (a_start, _, _) = g.offset_of(a).unwrap();
    let pg = [0x20_0000u64; 1];
    let p = g.plan_bind(&mut NoTablePages, &req(&pg, GIB + GIB / 2, Placement::best().evicting())).unwrap();
    let mut ev = p.evicted().to_vec();
    ev.sort();
    assert_eq!(ev, [a, b]);
    assert_eq!(p.offset(), a_start);
    assert!(g.offset_of(c).is_some(), "c survives");
    assert_eq!(p.ops().iter().filter(|o| matches!(o, Op::AwaitRetire { .. })).count(), 2);
}

#[test]
fn pinned_nodes_and_reservations_are_never_victims_and_cannot_be_unbound() {
    let (mut g, [a, b, c, d]) = full();
    for v in [a, b, c, d] {
        g.set_pinned(v, true).unwrap();
    }
    let pg = [0x20_0000u64; 1];
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, GIB, Placement::best().evicting())).err(), Some(MemError::NoSpace { size: GIB }));
    assert_eq!(g.plan_unbind(b).err(), Some(MemError::Pinned { id: b.0 }));
    g.set_pinned(c, false).unwrap();
    let p = g.plan_bind(&mut NoTablePages, &req(&pg, GIB, Placement::best().evicting())).unwrap();
    assert_eq!(p.evicted(), [c], "only the unpinned one");
    assert_eq!(g.plan_unbind(VmaId(1)).err(), Some(MemError::Pinned { id: 1 }), "id 1 is the error-capture reservation");
    assert_eq!(g.plan_unbind(VmaId(999)).err(), Some(MemError::NotBound { id: 999 }));
}

#[test]
fn a_fixed_range_over_live_nodes_evicts_them_only_when_allowed() {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    let a = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::with_mode(Mode::Fixed(0x10_0000)))).unwrap().vma();
    let fx = Placement::with_mode(Mode::Fixed(0x10_0000));
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx)).err(), Some(MemError::RangeOccupied { start: 0x10_0000, len: 4096 }));
    g.set_pinned(a, true).unwrap();
    assert_eq!(g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx.evicting())).err(), Some(MemError::RangeOccupied { start: 0x10_0000, len: 4096 }));
    g.set_pinned(a, false).unwrap();
    let p = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, fx.evicting())).unwrap();
    assert_eq!(p.evicted(), [a]);
}

#[test]
fn victims_and_vmas_grow_past_the_old_caps() {
    // RUNTIME-TABLES-HAVE-NO-CAP: the old caps were 16 victims per plan and 128 VMAs per space.
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    for i in 0..300u64 {
        g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::with_mode(Mode::Fixed(0x10_0000 + i * 4096)))).unwrap();
    }
    assert_eq!(g.live_nodes(), 301, "300 nodes plus the error-capture reservation: far past the old 128");
    let big = req(&pg, 300 * 4096, Placement::with_mode(Mode::Fixed(0x10_0000)).evicting());
    let p = g.plan_bind(&mut NoTablePages, &big).unwrap();
    assert_eq!(p.evicted().len(), 300, "one plan evicts 300 victims: far past the old 16");
    assert_eq!(g.live_nodes(), 2, "the reservation and the one big node");
}

#[test]
fn ids_are_never_reused() {
    let (mut g, _) = ggtt();
    let pg = [0x10_0000u64; 1];
    let a = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::best())).unwrap().vma();
    g.plan_unbind(a).unwrap();
    let b = g.plan_bind(&mut NoTablePages, &req(&pg, 4096, Placement::best())).unwrap().vma();
    assert_ne!(a, b);
    assert_eq!(g.check_access(0x2000, 1), Ok(b));
}
