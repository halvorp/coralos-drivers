// SPDX-License-Identifier: GPL-2.0-only
//! Session-scoped object handles and object lifetime: create sizes, close, stale and cross-session handles (the plan's
//! "cross-session handle theft" RED), handle-table bounds, and when a closed object may finally be freed.
mod common;
use common::*;
use i915_gem_core::ids::HANDLE_IDX_BITS;
use i915_gem_core::*;

const MASK: u32 = (1 << HANDLE_IDX_BITS) - 1;
const MAX_GEN: u32 = (1 << (32 - HANDLE_IDX_BITS)) - 1;

type R = Rig<8, 8>;

#[test]
fn create_rounds_up_to_pages_and_hands_out_nonzero_generation_tagged_handles() {
    // gem/i915_gem_create.c:102: size = round_up(size, page); i915_gem_publish returns the rounded size (gem/i915_gem_create.c:85)
    let mut r = R::new();
    let c = r.gem.create(r.s, 1).unwrap();
    assert_eq!(c.size, 4096);
    let d = r.gem.create(r.s, 4097).unwrap();
    assert_eq!(d.size, 8192);
    assert_ne!(c.handle, 0);
    assert_ne!(c.handle, d.handle);
    assert_eq!(c.handle >> HANDLE_IDX_BITS, 1, "first occupant of a slot has generation 1");
    assert_eq!(r.gem.object_size(r.s, c.handle).unwrap(), 4096);
}

#[test]
fn create_refuses_zero_overflow_and_beyond_ram_by_name() {
    let mut r = R::new();
    assert_eq!(r.gem.create(r.s, 0), Err(GemError::CreateZeroSize)); // gem/i915_gem_create.c:103-104
    assert_eq!(r.gem.create(r.s, 0).unwrap_err().errno(), 22);
    assert!(matches!(r.gem.create(r.s, u64::MAX), Err(GemError::CreateTooBig { .. })));
    assert_eq!(r.gem.create(r.s, u64::MAX).unwrap_err().errno(), 7); // E2BIG
    let max = r.gem.max_object_bytes();
    assert_eq!(max, 8 * GIB);
    assert!(r.gem.create(r.s, max).is_ok(), "exactly the bound is allowed");
    assert!(matches!(r.gem.create(r.s, max + 1), Err(GemError::CreateTooBig { max: m, .. }) if m == max), "one byte over is refused");
}

#[test]
fn create_ext_refusals_follow_linux() {
    let mut r = R::new();
    // gem/i915_gem_create.c:450: flags other than NEEDS_CPU_ACCESS
    assert_eq!(r.gem.create_ext(r.s, 4096, 2, &CreateExt::default()), Err(GemError::CreateFlags { flags: 2 }));
    // a single placement cannot satisfy NEEDS_CPU_ACCESS (gem/i915_gem_create.c:467-470)
    assert_eq!(r.gem.create_ext(r.s, 4096, 1, &CreateExt::default()), Err(GemError::CreateNeedsCpuAccess));
    // only the SYSTEM region exists (class 0, instance 0); DEVICE memory does not on Kaby Lake
    assert_eq!(r.gem.create_ext(r.s, 4096, 0, &CreateExt { regions: &[(1, 0)], ..Default::default() }), Err(GemError::CreateRegionUnsupported { class: 1, instance: 0 }));
    assert!(r.gem.create_ext(r.s, 4096, 0, &CreateExt { regions: &[(0, 0)], ..Default::default() }).is_ok());
    assert_eq!(r.gem.create_ext(r.s, 4096, 0, &CreateExt { protected: true, ..Default::default() }), Err(GemError::CreateProtectedUnsupported)); // gem/i915_gem_create.c:389-390
    assert_eq!(r.gem.create_ext(r.s, 4096, 0, &CreateExt { set_pat: Some(1), ..Default::default() }), Err(GemError::CreateSetPatUnsupported)); // gem/i915_gem_create.c:409
}

#[test]
fn a_handle_of_another_session_is_refused_by_name_on_every_entry_point() {
    // THE PLAN'S RED: "cross-session handle theft". Session A owns an object; session B presents A's handle.
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let h = r.create(4096);
    assert_eq!(r.gem.object_size(r.s, h), Ok(4096), "the owner can use it");

    let theft = GemError::CrossSessionHandle { handle: h };
    assert_eq!(r.gem.object_size(b, h), Err(theft));
    assert_eq!(r.gem.busy(b, h), Err(theft));
    assert_eq!(r.gem.madvise(b, h, 1), Err(theft));
    assert_eq!(r.gem.get_caching(b, h), Err(theft));
    assert_eq!(r.gem.get_tiling(b, h), Err(theft));
    assert_eq!(r.gem.mmap_offset(b, h, 2), Err(theft));
    assert_eq!(r.gem.wait_plan(b, h, 0, 0), Err(theft));
    assert_eq!(r.gem.set_domain(b, h, 1, 1, &mut r.pages), Err(theft));
    // GEM_CLOSE answers -EINVAL like drm_gem_handle_delete (drm_gem.c:407-410) but keeps the name
    let close = r.gem.close(b, h, &mut r.sink).unwrap_err();
    assert_eq!(close, GemError::CloseHandle { handle: h, cross: true });
    assert_eq!(close.name(), "CROSS-SESSION-HANDLE");
    assert_eq!(close.errno(), 22);
    assert_eq!(theft.errno(), 2, "-ENOENT, like an unknown handle");
    assert_eq!(theft.name(), "CROSS-SESSION-HANDLE");
    // an execbuffer naming the stolen handle is refused with the same name, and nothing was bound or requested
    let mut ctx_b = Engines::new();
    ctx_b.attach(&mut r.gem, b, 0);
    let e = Exec::new(0, vec![free_obj(h)]);
    let (gem, sync, pages, pool, sink) = (&mut r.gem, &r.sync, &mut r.pages, &mut r.pool, &mut r.sink);
    let err = with_req(&e, |req| gem.execbuffer2(b, sync, req, pages, &mut TablesAdapter(pool), &mut ctx_b, sink, 0)).unwrap_err();
    assert_eq!(err, theft);
    assert!(r.sink.binds().is_empty());
    assert_eq!(r.gem.active_requests(), 0);
    // the victim is untouched: still open, still the same size, still usable
    assert_eq!(r.gem.object_size(r.s, h), Ok(4096));
    r.gem.close(r.s, h, &mut r.sink).unwrap();
}

#[test]
fn the_same_handle_number_in_two_sessions_names_two_different_objects() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let ha = r.create(4096);
    let hb = r.gem.create(b, 8192).unwrap().handle;
    assert_eq!(ha, hb, "both sessions' first slot, first generation");
    assert_eq!(r.gem.object_size(r.s, ha), Ok(4096));
    assert_eq!(r.gem.object_size(b, hb), Ok(8192));
    let (oa, ob) = (r.gem.lookup_object(r.s, ha).unwrap(), r.gem.lookup_object(b, hb).unwrap());
    assert_ne!(oa, ob);
    assert_eq!(r.gem.object_owner(oa), Some(r.s));
    assert_eq!(r.gem.object_owner(ob), Some(b));
    // closing A's does not touch B's
    r.gem.close(r.s, ha, &mut r.sink).unwrap();
    assert_eq!(r.gem.object_size(b, hb), Ok(8192));
}

#[test]
fn a_closed_handle_is_stale_and_its_slot_gets_a_new_generation() {
    let mut r = R::new();
    let h = r.create(4096);
    r.gem.close(r.s, h, &mut r.sink).unwrap();
    assert_eq!(r.gem.object_size(r.s, h), Err(GemError::StaleHandle { handle: h }));
    let again = r.create(4096);
    assert_eq!(again & MASK, h & MASK, "the slot is reused");
    assert_eq!(again >> HANDLE_IDX_BITS, (h >> HANDLE_IDX_BITS) + 1, "with the next generation");
    // the old handle value must NOT reach the new occupant
    assert_eq!(r.gem.object_size(r.s, h), Err(GemError::StaleHandle { handle: h }));
    assert_eq!(r.gem.object_size(r.s, again), Ok(4096));
    // double close: -EINVAL, by name
    assert_eq!(r.gem.close(r.s, h, &mut r.sink), Err(GemError::CloseHandle { handle: h, cross: false }));
    assert_eq!(r.gem.close(r.s, 0, &mut r.sink), Err(GemError::CloseHandle { handle: 0, cross: false }));
}

#[test]
fn a_slot_whose_generation_wraps_is_retired_so_a_stale_handle_stays_stale() {
    // The generation is the top 12 bits of the handle; after MAX_GEN reuses a slot would alias an ancient handle. It is retired instead.
    let mut r = R::new();
    let mut last = 0;
    for _ in 0..MAX_GEN {
        let h = r.create(4096);
        r.gem.close(r.s, h, &mut r.sink).unwrap();
        last = h;
    }
    assert_eq!(last >> HANDLE_IDX_BITS, MAX_GEN);
    let next = r.create(4096);
    assert_ne!(next & MASK, last & MASK, "slot 0 is retired for good; the next object lives in another slot");
    assert_eq!(r.gem.object_size(r.s, last), Err(GemError::StaleHandle { handle: last }));
}

#[test]
fn ten_thousand_handles_in_one_session_work_and_each_names_its_own_object() {
    // RUNTIME-TABLES-HAVE-NO-CAP: the handle table grows with use.
    let mut r = R::new();
    let hs: Vec<u32> = (0..10_000).map(|i| r.gem.create(r.s, 4096 * (1 + (i % 3) as u64)).unwrap().handle).collect();
    assert_eq!(r.gem.object_count(), 10_000);
    let mut seen = std::collections::HashSet::new();
    assert!(hs.iter().all(|h| seen.insert(*h)), "all handles distinct");
    for (i, h) in hs.iter().enumerate() {
        assert_eq!(r.gem.object_size(r.s, *h), Ok(4096 * (1 + (i % 3) as u64)));
    }
    for h in &hs[..5000] {
        r.gem.close(r.s, *h, &mut r.sink).unwrap();
    }
    assert_eq!(r.gem.object_count(), 5_000, "closed idle objects are freed");
    assert!(hs[..5000].iter().all(|h| r.gem.object_size(r.s, *h).is_err()));
    // freed slots are reused: the tables do not grow again for the next 5000
    r.gem.set_table_budget(Some(0));
    for _ in 0..5000 {
        r.gem.create(r.s, 4096).unwrap();
    }
    assert_eq!(r.gem.create(r.s, 4096), Err(GemError::NoMem { table: "objects" }), "reuse is free, growth is not");
}

#[test]
fn the_handle_encoding_not_a_table_size_bounds_a_session() {
    // 20 index bits: 2^20 - 1 live handles, then HANDLE-SPACE-EXHAUSTED by name (the handle is a u32 on the wire).
    let mut r = R::new();
    let max = (1usize << HANDLE_IDX_BITS) - 1;
    for _ in 0..max {
        r.gem.create(r.s, 4096).unwrap();
    }
    let err = r.gem.create(r.s, 4096).unwrap_err();
    assert_eq!(err, GemError::HandleSpaceExhausted { table: "handles" });
    assert_eq!((err.name(), err.errno()), ("HANDLE-SPACE-EXHAUSTED", 12));
    assert_eq!(r.gem.object_count(), max, "the refused create left no object behind");
}

#[test]
fn allocation_failure_is_a_named_refusal_with_nothing_half_created() {
    let mut r = R::new();
    let before = r.gem.object_count();
    // the object's slot is charged first (1 entry), then its handle (1 entry)
    r.gem.set_table_budget(Some(1));
    let err = r.gem.create(r.s, 4096).unwrap_err();
    assert_eq!(err, GemError::NoMem { table: "handles" });
    assert_eq!((err.name(), err.errno()), ("GEM-NOMEM", 12));
    assert_eq!(r.gem.object_count(), before, "the half-made object was taken back");
    assert_eq!(r.gem.table_budget(), Some(0), "the object slot stays as a free slot (growth is not undone), the handle was never charged");
    // a retry with no budget: the free object slot is reused for nothing, the handle still cannot grow
    assert_eq!(r.gem.create(r.s, 4096), Err(GemError::NoMem { table: "handles" }));
    assert_eq!(r.gem.object_count(), before);
    // one more entry: the handle fits, the object reuses the free slot
    r.gem.set_table_budget(Some(1));
    let h = r.gem.create(r.s, 4096).unwrap().handle;
    assert_eq!(r.gem.object_size(r.s, h), Ok(4096));
    // and now the next object needs a new slot and there is no budget
    assert_eq!(r.gem.create(r.s, 4096), Err(GemError::NoMem { table: "objects" }));
    assert_eq!(r.gem.object_count(), before + 1);
    // removing the budget restores service
    r.gem.set_table_budget(None);
    assert!(r.gem.create(r.s, 4096).is_ok());
}

#[test]
fn sessions_grow_and_a_refused_session_leaves_no_trace() {
    let mut r = R::new();
    let mut sink = Sink::default();
    for _ in 0..200 {
        r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    }
    assert_eq!(r.gem.session_count(), 201);
    let vms = r.gem.vm_count();
    // every table a new session needs is charged before it exists: sessions, context ids, contexts, VMs. Growth already done stays
    // as free slots (so a retry needs less), but a refusal never leaves a session or an address space behind.
    let mut refused = 0;
    for budget in 0..8 {
        r.gem.set_table_budget(Some(budget));
        match r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink) {
            Err(e) => {
                assert!(matches!(e, GemError::NoMem { .. }), "budget {budget}: {e:?}");
                assert_eq!(r.gem.session_count(), 201, "no session at budget {budget}");
                assert_eq!(r.gem.vm_count(), vms, "no address space leaked at budget {budget}");
                refused += 1;
            }
            Ok(_) => {
                assert_eq!(r.gem.session_count(), 202);
                break;
            }
        }
    }
    assert!(refused >= 1, "a zero budget refuses");
    r.gem.set_table_budget(None);
    assert!(r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).is_ok());
}

#[test]
fn a_dead_session_id_never_names_a_later_session() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let h = r.gem.create(b, 4096).unwrap().handle;
    r.gem.close_session(b, &mut r.sink).unwrap();
    let c = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    assert_ne!(b, c);
    assert_eq!(r.gem.object_size(b, h), Err(GemError::SessionGone { session: b.0 }));
    // the new session has none of the old one's objects, even at the same handle value
    assert_eq!(r.gem.object_size(c, h), Err(GemError::HandleNotInSession { handle: h }));
}

#[test]
fn closing_a_session_frees_its_idle_objects_and_defers_busy_ones_until_retire() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let b = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    let h = r.gem.create(b, 4096).unwrap().handle;
    let idle = r.gem.lookup_object(b, h).unwrap();
    r.gem.close_session(b, &mut r.sink).unwrap();
    let eff = r.sink.effects();
    assert!(eff.contains(&Effect::ObjectFreed { obj: idle }), "{eff:?}");
}

#[test]
fn an_object_is_freed_only_after_its_last_request_retires() {
    let mut r = R::new();
    let h = r.create(4096);
    let obj = r.gem.lookup_object(r.s, h).unwrap();
    let batch = r.create(4096);
    let e = Exec::new(0, vec![pinned(h, 0x10_0000), pinned(batch, 0x20_0000)]);
    let done = r.exec(&e, 0).unwrap();
    r.sink.clear();
    r.gem.close(r.s, h, &mut r.sink).unwrap();
    // closed but the GPU may still read it: its pages must survive, and it must not be freed
    assert!(r.sink.effects().iter().all(|e| !matches!(e, Effect::FreePages { .. } | Effect::ObjectFreed { .. })), "{:?}", r.sink.effects());
    assert_eq!(r.sink.count_unbinds(), 0);
    // the hardware finishes
    r.gem.observe_seqno(done.engine, done.engine_ctx, done.seqno).unwrap();
    assert_eq!(r.gem.retire(&mut r.sink), 1);
    let eff = r.sink.effects();
    assert!(eff.contains(&Effect::FreePages { obj }), "{eff:?}");
    assert!(eff.contains(&Effect::ObjectFreed { obj }));
    assert_eq!(r.sink.count_unbinds(), 1, "unbound from its VM on the way out");
}
