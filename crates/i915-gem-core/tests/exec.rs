// SPDX-License-Identifier: GPL-2.0-only
//! EXECBUFFER2 end to end: a two-object softpinned batch planned from handles to bind plan, request in the ring and ELSP plan;
//! eviction when the address space is full; relocation; fences; dependencies between engines; and what a failed submission leaves.
mod common;
use common::*;
use i915_gem_core::*;
use i915_gt_core::emit::{emit_bb_start, CsBuf};
use i915_memory_core::{Target, Write};
use i915_submit_core::execlists::Port;
use i915_uapi_core::consts::{exec as ef, exec_fence as xf, exec_object as eo};
use i915_uapi_core::types::{ExecFence, ExecObject2, RelocationEntry};

type R = Rig<16, 16>;
/// A rig whose address space has page-table room for a full 4 GiB zone (2048 PTs + PDs).
type Big = Rig<32, 2400>;

/// `gen8_pte_encode(addr, I915_CACHE_LLC, 0)`: PRESENT | RW | PPAT_CACHED, with `PPAT_CACHED = _PAGE_PAT = bit 7`
/// (gt/gen8_ppgtt.c:32-60, gt/intel_gtt.h:137, arch/x86 pgtable_types.h:63).
fn pte(addr: u64) -> u64 {
    addr | 0x1 | 0x2 | 0x80
}

fn pt_write(w: &Write) -> Option<(u16, u64)> {
    match w.target {
        Target::Table { byte_offset, .. } => Some((byte_offset, w.value)),
        _ => None,
    }
}

#[test]
fn a_two_object_softpinned_batch_plans_the_bind_the_request_and_the_elsp_submission() {
    let mut r = R::new();
    let data = r.create(8192);
    let batch = r.create(4096);
    let (od, ob) = (r.gem.lookup_object(r.s, data).unwrap(), r.gem.lookup_object(r.s, batch).unwrap());
    // iris order: the batch is LAST. Softpinned at 1 MiB and 2 MiB, both 48-bit capable.
    let e = Exec::new(0, vec![pinned_write(data, 0x10_0000), pinned(batch, 0x20_0000)]);
    let done = r.exec(&e, 7).unwrap();

    // ---- the bind plans: one per object, at exactly the pinned addresses, nothing evicted, nothing moved -----------------------------
    let binds = r.sink.binds();
    assert_eq!(binds.len(), 2);
    let Rec::Bind { obj, offset, evicted, writes, .. } = binds[0] else { unreachable!() };
    assert_eq!((*obj, *offset), (od, 0x10_0000));
    assert!(evicted.is_empty());
    // the object's two pages (handed out by the PageSource in order) become two consecutive PTEs of the PT covering 0x100000:
    // PD index 0, PT index (0x100000 >> 12) & 511 = 0x100 -> byte offset 0x800 (gen8_pd_index, gt/gen8_ppgtt.c:140-141)
    let pages = r.pages.map[&od].clone();
    assert_eq!(pages.len(), 2);
    let ptes: Vec<(u16, u64)> = writes.iter().filter_map(pt_write).filter(|(_, v)| *v & 0x80 != 0 && v & 0x1 != 0).collect();
    assert!(ptes.contains(&(0x800, pte(pages[0]))), "{ptes:x?}");
    assert!(ptes.contains(&(0x808, pte(pages[1]))));
    let Rec::Bind { obj, offset, .. } = binds[1] else { unreachable!() };
    assert_eq!((*obj, *offset), (ob, 0x20_0000));
    assert_eq!(done.offsets[0], 0x10_0000);
    assert_eq!(done.offsets[1], 0x20_0000);
    assert_eq!((done.n_objects, done.moved, done.evicted, done.passes), (2, 0, 0, 1));

    // ---- the request: gt-core's batch start (the LAST object, at its GPU address) is in the context's ring -----------------------------
    let mut want = [0u32; 8];
    let mut cs = CsBuf::new(&mut want);
    emit_bb_start(&mut cs, 0x20_0000, 0, false).unwrap();
    let n = cs.len();
    let ring = &r.eng.rings[&(0, 0, done.engine_ctx)];
    assert!(ring.windows(n).any(|w| w == &want[..n]), "MI_BATCH_BUFFER_START 0x200000 not in the ring");
    assert_eq!(done.seqno, 2, "a timeline with an initial breadcrumb advances by two (submit-core tests/engine.rs)");

    // ---- the ELSP plan -----------------------------------------------------------------------------------------------------------
    assert!(done.submitted);
    let sub = r.sink.effects().into_iter().find_map(|e| if let Effect::Submit { engine, plan } = e { Some((engine, plan)) } else { None }).expect("a Submit effect");
    assert_eq!(sub.0, 0, "render engine");
    assert_eq!(sub.1.ports[0], Some(Port { ctx: done.engine_ctx, seqno: done.seqno }));
    assert_ne!(sub.1.descs[0], 0);

    // ---- activity: both objects busy on the render class; the written one reports the write -----------------------------------------
    assert_eq!(r.gem.busy(r.s, data).unwrap(), 1 | 0x1_0000, "writer: class + 1 in the low half, read bit in the high half (gem/i915_gem_busy.c:22-38)");
    assert_eq!(r.gem.busy(r.s, batch).unwrap(), 0x1_0000, "reader: read bit only");
    // the CPU domain bookkeeping of _i915_vma_move_to_active (i915_vma.c:2024-2030, i915_vma.c:2038)
    assert_eq!(r.gem.busy(r.s, batch).unwrap() & 0xffff, 0);
}

#[test]
fn a_second_submission_reuses_the_bindings_and_costs_no_bind() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    r.exec(&e, 0).unwrap();
    r.sink.clear();
    let done = r.exec(&e, 1).unwrap();
    assert!(r.sink.binds().is_empty(), "eb_pin_vma reuses the current location (gem/i915_gem_execbuffer.c:438-489)");
    assert_eq!(done.passes, 0);
    assert_eq!(done.seqno, 4);
}

#[test]
fn a_softpinned_object_bound_elsewhere_is_unbound_and_rebound_where_it_asked() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 0).unwrap();
    r.sink.clear();
    // iris moved `a`: eb_vma_misplaced (gem/i915_gem_execbuffer.c:391-393) -> unbind, then rebind at the new address
    let done = r.exec(&Exec::new(0, vec![pinned(a, 0x30_0000), pinned(b, 0x20_0000)]), 1).unwrap();
    assert_eq!(done.offsets[0], 0x30_0000);
    assert_eq!(r.sink.count_unbinds(), 1);
    assert_eq!(r.sink.binds().len(), 1);
    let oa = r.gem.lookup_object(r.s, a).unwrap();
    let vm = r.gem.context_vm(r.s, 0).unwrap();
    assert_eq!(r.gem.bound_offset(vm, oa), Some(0x30_0000));
}

#[test]
fn unpinned_objects_are_placed_by_the_allocator_and_the_new_address_is_reported() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    let done = r.exec(&Exec::new(0, vec![free_obj(a), free_obj(b)]), 0).unwrap();
    // the user passed offset 0; any object placed elsewhere is reported as moved (UPDATE, gem/i915_gem_execbuffer.c:1019)
    assert_ne!(done.offsets[0], done.offsets[1]);
    assert_eq!(done.offsets[0] & 0xfff, 0);
    assert_eq!(done.offsets[1] & 0xfff, 0);
    assert!(done.moved >= 1);
    assert_eq!(done.moved as usize, done.offsets[..2].iter().filter(|o| **o != 0).count());
}

fn eight_pages_of_big(r: &mut Big, mib: u64) -> u32 {
    r.create(mib * MIB)
}

#[test]
fn a_full_address_space_evicts_the_least_recently_used_and_waits_for_it_to_retire() {
    // A `Gem` with page-table room for a 4 GiB zone is hundreds of KiB: the executor keeps it in a static, a test gives it a stack.
    std::thread::Builder::new().stack_size(256 << 20).spawn(full_zone_eviction).unwrap().join().unwrap();
}

fn full_zone_eviction() {
    // Objects without SUPPORTS_48B_ADDRESS live below 4 GiB (PIN_ZONE_4G, gem/i915_gem_execbuffer.c:419-424). Fill that zone with 256 MiB objects.
    let mut r = Big::new();
    let batch = r.create(4096);
    let mut bigs = vec![];
    let mut first_done = None;
    for i in 0..15 {
        let big = eight_pages_of_big(&mut r, 256);
        let mut o = free_obj(big);
        o.flags = 0; // restricted to the first 4 GiB
        let mut b = free_obj(batch);
        b.flags = 0;
        let done = r.exec(&Exec::new(0, vec![o, b]), i).unwrap();
        assert_eq!(done.evicted, 0, "pass 0 fits while the zone has room (i = {i})");
        first_done.get_or_insert(done);
        bigs.push(big);
    }
    let big0 = r.gem.lookup_object(r.s, bigs[0]).unwrap();
    let vm = r.gem.context_vm(r.s, 0).unwrap();
    assert!(r.gem.bound_offset(vm, big0).is_some());
    // The 16th no longer fits below 4 GiB (15 x 256 MiB + the batch): pass 0 gives -ENOSPC, pass 1 evicts.
    r.sink.clear();
    let big = eight_pages_of_big(&mut r, 256);
    let (mut o, mut b) = (free_obj(big), free_obj(batch));
    o.flags = 0;
    b.flags = 0;
    let done = r.exec(&Exec::new(0, vec![o, b]), 99).unwrap();
    assert_eq!(done.passes, 2, "pass 0 failed with ENOSPC, pass 1 succeeded");
    assert_eq!(done.evicted, 1);
    // the LRU victim is the oldest unpinned object; the batch was touched by every submission so it stays
    assert_eq!(r.gem.bound_offset(vm, big0), None, "the least recently used object was evicted");
    assert!(r.gem.bound_offset(vm, r.gem.lookup_object(r.s, bigs[14]).unwrap()).is_some());
    assert!(r.gem.bound_offset(vm, r.gem.lookup_object(r.s, batch).unwrap()).is_some(), "the batch is in use by this submission and never a victim");
    let binds = r.sink.binds();
    let Rec::Bind { evicted, retire_waits, .. } = binds.iter().find(|b| matches!(b, Rec::Bind { obj, .. } if *obj == r.gem.lookup_object(r.s, big).unwrap())).unwrap() else { unreachable!() };
    assert_eq!(evicted.len(), 1);
    assert!(*retire_waits >= 1, "the victim's range is not reused until the GPU has retired it (AwaitRetire)");
}

#[test]
fn a_softpin_over_a_stale_binding_evicts_it_on_the_second_pass() {
    let mut r = R::new();
    let stale = r.create(4096);
    let batch = r.create(4096);
    let user = r.create(4096);
    r.exec(&Exec::new(0, vec![pinned(stale, 0x10_0000), pinned(batch, 0x20_0000)]), 0).unwrap();
    r.sink.clear();
    // a different object wants exactly that address: RangeOccupied in pass 0 (i915_gem_gtt_reserve -ENOSPC), evicted in pass 1
    let done = r.exec(&Exec::new(0, vec![pinned(user, 0x10_0000), pinned(batch, 0x20_0000)]), 1).unwrap();
    assert_eq!(done.passes, 2);
    assert_eq!(done.evicted, 1);
    assert_eq!(done.offsets[0], 0x10_0000);
    let vm = r.gem.context_vm(r.s, 0).unwrap();
    assert_eq!(r.gem.bound_offset(vm, r.gem.lookup_object(r.s, stale).unwrap()), None);
}

#[test]
fn when_no_pass_can_make_room_the_submission_fails_with_enospc_and_leaves_no_pins() {
    // 5 GiB cannot be placed below 4 GiB however much is evicted; pass 2 empties the VM and still fails.
    let mut r = R::new();
    let batch = r.create(4096);
    let ok = r.create(4096);
    r.exec(&Exec::new(0, vec![pinned(ok, 0x10_0000), pinned(batch, 0x20_0000)]), 0).unwrap();
    let huge = r.create(5 * GIB);
    let mut o = free_obj(huge);
    o.flags = 0;
    let err = r.exec(&Exec::new(0, vec![o, pinned(batch, 0x20_0000)]), 1).unwrap_err();
    assert!(matches!(err, GemError::ExecReserve { why: i915_memory_core::MemError::NoSpace { .. }, .. }), "{err:?}");
    assert_eq!(err.errno(), 28);
    assert_eq!(err.name(), "EXEC-RESERVE");
    assert_eq!(r.gem.active_requests(), 1, "no request was built for the failed submission");
    // nothing stayed pinned: a following submission can evict anything it needs and succeeds
    let again = r.exec(&Exec::new(0, vec![pinned(ok, 0x10_0000), pinned(batch, 0x20_0000)]), 2).unwrap();
    assert_eq!(again.offsets, {
        let mut o = [0u64; 128];
        o[0] = 0x10_0000;
        o[1] = 0x20_0000;
        o
    });
}

#[test]
fn a_batch_start_offset_is_added_to_the_batch_address() {
    let mut r = R::new();
    let data = r.create(4096);
    let batch = r.create(8192);
    let mut e = Exec::new(0, vec![pinned(data, 0x10_0000), pinned(batch, 0x20_0000)]);
    e.start = 0x40;
    let done = r.exec(&e, 0).unwrap();
    let mut want = [0u32; 8];
    let mut cs = CsBuf::new(&mut want);
    emit_bb_start(&mut cs, 0x20_0040, 0, false).unwrap();
    let n = cs.len();
    assert!(r.eng.rings[&(0, 0, done.engine_ctx)].windows(n).any(|w| w == &want[..n]), "eb_request_submit: i915_vma_offset(batch) + batch_start_offset (gem/i915_gem_execbuffer.c:2397-2400)");
}

#[test]
fn batch_first_makes_object_zero_the_batch() {
    let mut r = R::new();
    let batch = r.create(4096);
    let data = r.create(4096);
    let e = Exec::new(ef::I915_EXEC_BATCH_FIRST, vec![pinned(batch, 0x20_0000), pinned(data, 0x10_0000)]);
    let done = r.exec(&e, 0).unwrap();
    let mut want = [0u32; 8];
    let mut cs = CsBuf::new(&mut want);
    emit_bb_start(&mut cs, 0x20_0000, 0, false).unwrap();
    let n = cs.len();
    assert!(r.eng.rings[&(0, 0, done.engine_ctx)].windows(n).any(|w| w == &want[..n]));
}

// ---- relocation ----------------------------------------------------------------------------------------------------------------------

fn reloc(target: u32, offset: u64, presumed: u64, delta: u32, write: u32) -> RelocationEntry {
    RelocationEntry { target_handle: target, delta, offset, presumed_offset: presumed, read_domains: 0x2, write_domain: write }
}

#[test]
fn a_correct_presumed_offset_needs_no_patch_and_a_wrong_one_becomes_a_patch_effect() {
    let mut r = R::new();
    let target = r.create(4096);
    let batch = r.create(4096);
    // batch (pinned, so no bias) holds a relocation to `target` at byte 0x10 of the batch
    let mut b = pinned(batch, 0x20_0000);
    b.relocation_count = 1;
    let mut e = Exec::new(0, vec![pinned(target, 0x10_0000), b]);
    e.relocs = vec![reloc(target, 0x10, 0x10_0000, 0x20, 0)];
    let done = r.exec(&e, 0).unwrap();
    assert_eq!(done.relocs_patched, 0, "presumed_offset already matches (gem/i915_gem_execbuffer.c:1472-1477)");

    // a stale presumed offset: patch value = gen8_canonical_addr((int)delta + target offset) (gem/i915_gem_execbuffer.c:1098-1101)
    e.relocs = vec![reloc(target, 0x10, 0xdead_0000, 0x20, 0)];
    r.sink.clear();
    let done = r.exec(&e, 1).unwrap();
    assert_eq!(done.relocs_patched, 1);
    let ob = r.gem.lookup_object(r.s, batch).unwrap();
    assert!(r.sink.effects().contains(&Effect::PatchReloc { owner: ob, byte_offset: 0x10, value: 0x10_0020, width: 8, reloc_index: 0, presumed: 0x10_0000 }));
}

#[test]
fn the_reloc_delta_is_signed_and_the_value_is_canonical() {
    let mut r = R::new();
    let target = r.create(4096);
    let batch = r.create(4096);
    let mut b = pinned(batch, 0x20_0000);
    b.relocation_count = 1;
    let mut t = pinned(target, 0x7fff_ffff_f000); // top of the lower canonical half
    t.flags |= eo::EXEC_OBJECT_SUPPORTS_48B_ADDRESS;
    let mut e = Exec::new(0, vec![t, b]);
    e.relocs = vec![reloc(target, 0x10, 0, 0xffff_fff0, 0)]; // delta = -16 as (int)
    r.exec(&e, 0).unwrap();
    let eff = r.sink.effects();
    assert!(eff.iter().any(|x| matches!(x, Effect::PatchReloc { value, .. } if *value == 0x7fff_ffff_eff0)), "{eff:x?}");
    // an address with bit 47 set is sign-extended to canonical form (gt/intel_gpu_commands.h:457-460)
    let mut r = R::new();
    let target = r.create(4096);
    let batch = r.create(4096);
    let mut b = pinned(batch, 0x20_0000);
    b.relocation_count = 1;
    let t = pinned(target, 0x8000_0000_0000 - 0x1000);
    let mut e = Exec::new(0, vec![t, b]);
    e.relocs = vec![reloc(target, 0x10, 0, 0x1000, 0)];
    r.exec(&e, 0).unwrap();
    let eff = r.sink.effects();
    assert!(eff.iter().any(|x| matches!(x, Effect::PatchReloc { value, .. } if *value == 0xffff_8000_0000_0000)), "{eff:x?}");
}

#[test]
fn a_relocation_with_a_write_domain_makes_its_target_a_written_object() {
    let mut r = R::new();
    let target = r.create(4096);
    let batch = r.create(4096);
    let mut b = pinned(batch, 0x20_0000);
    b.relocation_count = 1;
    let mut e = Exec::new(0, vec![pinned(target, 0x10_0000), b]); // target NOT flagged WRITE by the user
    e.relocs = vec![reloc(target, 0x10, 0x10_0000, 0, 0x2)]; // I915_GEM_DOMAIN_RENDER write
    r.exec(&e, 0).unwrap();
    assert_eq!(r.gem.busy(r.s, target).unwrap() & 0xffff, 1, "target->flags |= EXEC_OBJECT_WRITE (gem/i915_gem_execbuffer.c:1445-1446)");
}

#[test]
fn a_relocation_beyond_the_object_or_unaligned_is_refused_by_name() {
    let mut r = R::new();
    let target = r.create(4096);
    let batch = r.create(4096);
    let mk = |off: u64| {
        let mut b = pinned(batch, 0x20_0000);
        b.relocation_count = 1;
        let mut e = Exec::new(0, vec![pinned(target, 0x10_0000), b]);
        e.relocs = vec![reloc(target, off, 0xbad, 0, 0)];
        e
    };
    let err = r.exec(&mk(4096 - 4), 0).unwrap_err(); // needs 8 bytes at 4092: beyond the object (gem/i915_gem_execbuffer.c:1480-1488)
    assert_eq!(err.name(), "EXEC-RELOC-BOUNDS");
    let err = r.exec(&mk(0x12), 0).unwrap_err();
    assert_eq!(err.name(), "EXEC-RELOC-UNALIGNED");
    assert_eq!(r.gem.active_requests(), 0);
}

// ---- fences ----------------------------------------------------------------------------------------------------------------------------

#[test]
fn fences_become_effects_in_linux_order() {
    let mut r = R::new();
    r.sync.syncobjs = vec![
        (5, i915_uapi_core::SyncobjInfo { has_fence: true, chain_seqno: None }),
        (6, i915_uapi_core::SyncobjInfo { has_fence: false, chain_seqno: None }),
    ];
    r.sync.files = vec![77];
    let a = r.create(4096);
    let b = r.create(4096);
    let mut e = Exec::new(ef::I915_EXEC_FENCE_IN | ef::I915_EXEC_FENCE_OUT, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.rsvd2 = 77;
    e.fences = vec![ExecFence { handle: 5, flags: xf::I915_EXEC_FENCE_WAIT }, ExecFence { handle: 6, flags: xf::I915_EXEC_FENCE_SIGNAL }];
    let done = r.exec(&e, 0).unwrap();
    assert_eq!((done.waits, done.signals, done.out_fence), (1, 1, true));
    let eff = r.sink.effects();
    let rq = done.request;
    let pos = |want: Effect| eff.iter().position(|x| *x == want).unwrap_or_else(|| panic!("missing {want:?} in {eff:?}"));
    let i_in = pos(Effect::AwaitInFence { rq, token: 77, submit: false });
    let i_wait = pos(Effect::AwaitSyncobj { rq, handle: 5, point: 0 });
    let i_sig = pos(Effect::SignalSyncobj { rq, handle: 6, point: 0 });
    let i_out = pos(Effect::OutFence { rq });
    let i_submit = eff.iter().position(|x| matches!(x, Effect::Submit { .. })).unwrap();
    assert!(i_in < i_wait && i_wait < i_sig && i_sig < i_out && i_out < i_submit, "{eff:?}");
}

#[test]
fn fence_submit_waits_for_submission_not_completion() {
    let mut r = R::new();
    r.sync.files = vec![9];
    let a = r.create(4096);
    let b = r.create(4096);
    let mut e = Exec::new(ef::I915_EXEC_FENCE_SUBMIT, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.rsvd2 = 9;
    let done = r.exec(&e, 0).unwrap();
    assert!(r.sink.effects().contains(&Effect::AwaitInFence { rq: done.request, token: 9, submit: true }));
}

#[test]
fn timeline_points_are_carried_and_a_missing_wait_is_skipped() {
    let mut r = R::new();
    r.sync.syncobjs = vec![
        (3, i915_uapi_core::SyncobjInfo { has_fence: true, chain_seqno: Some(10) }),
        (4, i915_uapi_core::SyncobjInfo { has_fence: false, chain_seqno: None }),
    ];
    let a = r.create(4096);
    let b = r.create(4096);
    let mut e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.timeline = vec![(
        vec![ExecFence { handle: 3, flags: xf::I915_EXEC_FENCE_WAIT }, ExecFence { handle: 3, flags: xf::I915_EXEC_FENCE_SIGNAL }, ExecFence { handle: 4, flags: xf::I915_EXEC_FENCE_SIGNAL }],
        vec![8, 11, 5],
    )];
    let done = r.exec(&e, 0).unwrap();
    let eff = r.sink.effects();
    let rq = done.request;
    assert!(eff.contains(&Effect::AwaitSyncobj { rq, handle: 3, point: 8 }));
    assert!(eff.contains(&Effect::SignalSyncobj { rq, handle: 3, point: 11 }));
    assert!(eff.contains(&Effect::SignalSyncobj { rq, handle: 4, point: 5 }));
    assert_eq!((done.waits, done.signals), (1, 2));
}

// ---- dependencies between engines ------------------------------------------------------------------------------------------------------------

/// A softpinned object WITHOUT `EXEC_OBJECT_ASYNC`: the kernel tracks its dependencies.
fn tracked(handle: u32, offset: u64) -> ExecObject2 {
    let mut o = pinned(handle, offset);
    o.flags &= !eo::EXEC_OBJECT_ASYNC;
    o
}

#[test]
fn a_reader_on_another_engine_awaits_the_writer_and_a_same_engine_reader_does_not() {
    let mut r = R::new();
    let shared = r.create(4096);
    let b1 = r.create(4096);
    let b2 = r.create(4096);
    let mut w1 = tracked(shared, 0x10_0000);
    w1.flags |= eo::EXEC_OBJECT_WRITE;
    let w = r.exec(&Exec::new(0, vec![w1, tracked(b1, 0x20_0000)]), 0).unwrap();
    r.sink.clear();
    // the same engine: in-order, no dependency
    r.exec(&Exec::new(0, vec![tracked(shared, 0x10_0000), tracked(b2, 0x30_0000)]), 1).unwrap();
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::AwaitRequest { .. })));
    r.sink.clear();
    // the video engine reads what the render engine wrote: it must wait for it (i915_request_await_object, i915_request.c:1583)
    let done = r.exec(&Exec::new(ef::I915_EXEC_BSD, vec![tracked(shared, 0x10_0000), tracked(b2, 0x30_0000)]), 2).unwrap();
    assert_eq!(done.engine, r.gem.engine_index(2, 0).unwrap(), "VCS0");
    assert_eq!(r.sink.effects().iter().filter(|e| matches!(e, Effect::AwaitRequest { on, .. } if *on == w.request)).count(), 1, "once, however many objects link the two");
    assert!(r.sink.effects().contains(&Effect::AwaitRequest { rq: done.request, on: w.request }), "{:?}", r.sink.effects());
    // EXEC_OBJECT_ASYNC (what iris sets on every softpinned object) opts out of the kernel's tracking (gem/i915_gem_execbuffer.c:2123)
    r.sink.clear();
    r.exec(&Exec::new(ef::I915_EXEC_BSD, vec![pinned(shared, 0x10_0000), pinned(b2, 0x30_0000)]), 3).unwrap();
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::AwaitRequest { .. })));
    // once the writer completed there is nothing to wait for
    r.gem.observe_seqno(w.engine, w.engine_ctx, w.seqno).unwrap();
    r.gem.retire(&mut r.sink);
    r.sink.clear();
    r.exec(&Exec::new(ef::I915_EXEC_BSD, vec![tracked(shared, 0x10_0000), tracked(b2, 0x30_0000)]), 4).unwrap();
    assert!(!r.sink.effects().iter().any(|e| matches!(e, Effect::AwaitRequest { on, .. } if *on == w.request)));
}

// ---- failures leave a clean state ------------------------------------------------------------------------------------------------------------

#[test]
fn an_engine_that_refuses_to_build_the_request_leaves_binds_but_no_request_and_no_pins() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    r.eng.fail_build = true;
    let err = r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 0).unwrap_err();
    assert_eq!(err.name(), "RING-FULL");
    assert_eq!(err.errno(), 5);
    assert_eq!(r.gem.active_requests(), 0);
    assert_eq!(r.sink.binds().len(), 2, "binds are a cache and stay (Linux leaves bound VMAs after a failed execbuffer)");
    // unpinned again: the next submission may move them
    r.eng.fail_build = false;
    let again = r.exec(&Exec::new(0, vec![pinned(a, 0x30_0000), pinned(b, 0x20_0000)]), 1).unwrap();
    assert_eq!(again.offsets[0], 0x30_0000);
}

#[test]
fn a_context_without_an_engine_context_for_the_engine_is_refused_by_name() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let ctx = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut sink).unwrap();
    let a = r.create(4096);
    let b = r.create(4096);
    let mut e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.ctx = ctx as u64;
    let err = r.exec(&e, 0).unwrap_err();
    assert_eq!(err, GemError::ExecEngineContextMissing { ctx, class: 0, instance: 0 });
}

#[test]
fn the_same_object_twice_and_too_many_objects_are_refused() {
    let mut r = Rig::<160, 16>::new();
    let a = r.create(4096);
    let err = r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(a, 0x10_0000)]), 0).unwrap_err();
    assert_eq!(err, GemError::ExecObjectDuplicate { handle: a });
    let mut objs = vec![];
    for i in 0..(i915_gem_core::limits::MAX_EXEC_OBJECTS + 1) {
        let h = r.create(4096);
        objs.push(pinned(h, 0x10_0000 + i as u64 * 0x1000));
    }
    let max = i915_gem_core::limits::MAX_EXEC_OBJECTS;
    let err = r.exec(&Exec::new(0, objs.clone()), 0).unwrap_err();
    assert_eq!(err, GemError::ExecTooManyObjects { count: max + 1, max });
    // exactly the bound is accepted
    objs.pop();
    let ok = r.exec(&Exec::new(0, objs), 0);
    assert!(ok.is_ok(), "{ok:?}");
}

#[test]
fn execbuffer_with_a_purged_object_is_efault() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    r.gem.madvise(r.s, a, 1).unwrap(); // DONTNEED, no pages yet -> purged at once (i915_gem.c:1092-1095)
    let err = r.exec(&Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]), 0).unwrap_err();
    assert_eq!(err, GemError::ObjectPurged { handle: a });
    assert_eq!(err.errno(), 14);
}

#[test]
fn a_banned_context_cannot_submit() {
    let mut r = R::new();
    let a = r.create(4096);
    let b = r.create(4096);
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    let done = r.exec(&e, 0).unwrap();
    // "3 hangs within 120s? Banned!" (gem/i915_gem_context_types.h:389): the history holds two timestamps, so it is the THIRD hang
    // inside the window that finds a recent one in hang_timestamp[0] (gt/intel_reset.c:94-102)
    let v = r.gem.request_reset(done.engine, done.engine_ctx, done.seqno, true, 1000).unwrap();
    assert!(!v.banned, "first hang of a recoverable context");
    let e2 = r.exec(&e, 1).unwrap();
    let v = r.gem.request_reset(e2.engine, e2.engine_ctx, e2.seqno, true, 2000).unwrap();
    assert!(!v.banned, "second hang");
    let e3 = r.exec(&e, 2).unwrap();
    let v = r.gem.request_reset(e3.engine, e3.engine_ctx, e3.seqno, true, 3000).unwrap();
    assert!(v.banned, "third hang within 120 s");
    let err = r.exec(&e, 3).unwrap_err();
    assert_eq!(err, GemError::ContextBanned { id: 0 });
    assert_eq!(err.errno(), 5);
}

// ---- a submission never evicts its own members --------------------------------------------------------------------------------------

#[test]
fn an_object_of_the_submission_is_never_a_victim_of_the_same_submission() {
    std::thread::Builder::new().stack_size(256 << 20).spawn(own_members_are_pinned).unwrap().join().unwrap();
}

fn own_members_are_pinned() {
    // 16 x 256 MiB cannot all live below 4 GiB. Without the pin the 16th would silently evict the 1st (the oldest) and the
    // submission would run with an unbound object; with it, the reserve passes run out and the submission fails.
    let mut r = Big::new();
    let objs: Vec<ExecObject2> = (0..16)
        .map(|_| {
            let h = r.create(256 * MIB);
            let mut o = free_obj(h);
            o.flags = 0; // restricted to the first 4 GiB
            o
        })
        .collect();
    let err = r.exec(&Exec::new(0, objs.clone()), 0).unwrap_err();
    assert!(matches!(err, GemError::ExecReserve { why: i915_memory_core::MemError::NoSpace { .. }, .. }), "{err:?}");
    assert_eq!(r.gem.active_requests(), 0);
    // 15 of them (plus nothing else) do fit
    let mut fifteen = objs;
    fifteen.pop();
    // the batch is the last object and must also fit: replace the 15th by a small batch
    let batch = r.create(4096);
    let mut b = free_obj(batch);
    b.flags = 0;
    fifteen.pop();
    fifteen.push(b);
    assert!(r.exec(&Exec::new(0, fifteen), 1).is_ok());
}

// ---- relocation re-checks after the final placement ---------------------------------------------------------------------------------------

#[test]
fn relocation_bounds_are_rechecked_against_the_final_placement() {
    // The uAPI crate skips every check for a relocation whose presumed offset matches the target's CURRENT placement. If the
    // reserve then moves the target (here: a new alignment), the relocation is no longer a no-op and must be validated here.
    // Each case starts from a fresh state: after one failed submission the target has already moved.
    struct Case {
        r: R,
        e: Exec,
    }
    let setup = |off: u64| -> Case {
        let mut r = R::new();
        let filler = r.create(4096);
        let target = r.create(4096);
        let batch = r.create(4096);
        let first = r.exec(&Exec::new(0, vec![free_obj(filler), free_obj(target), pinned(batch, 0x20_0000)]), 0).unwrap();
        let at = first.offsets[1];
        assert_ne!(at, 0, "the filler took address 0, the target sits above it");
        let mut t = free_obj(target);
        t.alignment = 1 << 20; // forces a move: `at` is not 1 MiB aligned
        let mut b = pinned(batch, 0x20_0000);
        b.relocation_count = 1;
        let mut e = Exec::new(0, vec![free_obj(filler), t, b]);
        e.relocs = vec![reloc(target, off, at, 0, 0)]; // presumed = where it WAS
        Case { r, e }
    };
    let mut c = setup(0x12);
    assert_eq!(c.r.exec(&c.e, 1).unwrap_err().name(), "EXEC-RELOC-UNALIGNED");
    let mut c = setup(4096 - 4);
    assert_eq!(c.r.exec(&c.e, 1).unwrap_err().name(), "EXEC-RELOC-BOUNDS");
    // a valid one is patched with the NEW address
    let mut c = setup(0x10);
    let done = c.r.exec(&c.e, 1).unwrap();
    assert_eq!(done.relocs_patched, 1);
    assert_eq!(done.offsets[1] % (1 << 20), 0);
    assert!(c.r.sink.effects().iter().any(|e| matches!(e, Effect::PatchReloc { value, presumed, .. } if *value == done.offsets[1] && *presumed == done.offsets[1])));
}

#[test]
fn a_cpu_dirty_object_on_a_part_without_llc_is_flushed_once_before_the_gpu_reads_it() {
    // KBL is coherent, so the clflush path is dead there; exercise it on a device with HAS_LLC cleared (gem/i915_gem_execbuffer.c:2117-2120).
    let mut d = dev();
    d.desc.info.has_llc = false;
    let mut gem: Gem<16, 16> = Gem::new(d, GemConfig { pat_enabled: true });
    let mut pool = Pool::new();
    let mut sink = Sink::default();
    let s = gem.open_session(SessionCaps::default(), pool.init(), &mut sink).unwrap();
    let mut eng = Engines::new();
    eng.attach(&mut gem, s, 0);
    let (mut pages, sync) = (Pages::new(), Sync::default());
    let (a, b) = (gem.create(s, 4096).unwrap().handle, gem.create(s, 4096).unwrap().handle);
    assert_eq!(gem.get_caching(s, a), Ok(i915_uapi_core::consts::misc::I915_CACHING_NONE), "no LLC: uncached objects");
    let oa = gem.lookup_object(s, a).unwrap();
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    for i in 0..2 {
        sink.clear();
        with_req(&e, |req| gem.execbuffer2(s, &sync, req, &mut pages, &mut TablesAdapter(&mut pool), &mut eng, &mut sink, i)).unwrap();
        let n = sink.effects().iter().filter(|x| **x == Effect::Clflush { obj: oa }).count();
        assert_eq!(n, if i == 0 { 1 } else { 0 }, "flushed on the first use only: i915_gem_clflush_object clears cache_dirty (pass {i})");
    }
}
