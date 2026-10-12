// SPDX-License-Identifier: GPL-2.0-only
//! Every bound that is not already pinned by a behavioural test: the declared limits are the documented numbers, and the tables
//! that hold them refuse the next entry by name.
mod common;
use common::*;
use i915_gem_core::limits::*;
use i915_gem_core::*;

type R = Rig;

#[test]
fn what_stays_fixed_is_hardware_abi_or_policy_and_has_these_values() {
    // RUNTIME-TABLES-HAVE-NO-CAP: no table of a runtime-growing population has a constant size. `limits` holds only these.
    assert_eq!(MAX_ENGINES, 5); // RCS0 + BCS0 + VCS0..1 + VECS0: fused into the part
    assert_eq!(MAX_USER_ENGINES, 64); // I915_EXEC_RING_MASK + 1: a 6-bit selector (gem/i915_gem_context.c:758-760)
    // Linux's own constants, from the headers
    assert_eq!(HANG_HISTORY, 2); // gem/i915_gem_context_types.h:388
    assert_eq!(CONTEXT_FAST_HANG_MS, 120_000); // gem/i915_gem_context_types.h:389
    assert_eq!(CLIENT_FAST_HANG_MS, 60_000); // i915_file_private.h:101
    assert_eq!((CLIENT_SCORE_HANG_FAST, CLIENT_SCORE_CONTEXT_BAN, CLIENT_SCORE_BANNED), (1, 3, 9)); // i915_file_private.h:100-103
    assert_eq!(MAX_GUILTY_BEFORE_BAN, 16); // policy threshold
}

#[test]
fn many_requests_are_tracked_and_retiring_makes_slots_reusable() {
    let mut r = R::new();
    r.eng.auto_retire = true; // the engine's own request table is not under test
    let (a, b) = (r.create(4096), r.create(4096));
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    let mut last = None;
    for i in 0..1000 {
        last = Some(r.exec(&e, i as u64).unwrap_or_else(|e| panic!("request {i}: {e:?}")));
    }
    assert_eq!(r.gem.active_requests(), 1000, "no request cap: the table grew to hold them");
    let d = last.unwrap();
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    assert_eq!(r.gem.retire(&mut r.sink), 1000, "all requests of one timeline complete when its seqno passes the last");
    // out of budget: freed slots are reused for free, the next growth is refused, and the ENGINE was never asked
    r.gem.set_table_budget(Some(0));
    for i in 0..1000 {
        r.exec(&e, i).unwrap();
    }
    let before = r.eng.engines[0].2.requests().len();
    let err = r.exec(&e, 0).unwrap_err();
    assert_eq!(err, GemError::NoMem { table: "requests" });
    assert_eq!(err.errno(), 12);
    assert_eq!(r.eng.engines[0].2.requests().len(), before, "the table was charged BEFORE the engine was asked: no untracked request");
    assert_eq!(r.gem.active_requests(), 1000);
}

#[test]
fn the_seqno_observation_table_grows_and_forgets_dead_contexts() {
    let mut r = R::new();
    for i in 0..2000u32 {
        r.gem.observe_seqno(0, 1000 + i, 5).unwrap();
    }
    r.gem.set_table_budget(Some(0));
    assert_eq!(r.gem.observe_seqno(0, 1_000_000, 5), Err(GemError::NoMem { table: "hw-seqnos" }));
    // an engine context already known updates in place and needs no new slot
    assert!(r.gem.observe_seqno(0, 1000, 6).is_ok());
    assert_eq!(r.gem.observe_seqno(9, 0, 0), Err(GemError::EngineIndex { index: 9 }), "no such engine");
    // a destroyed context's observation is dropped with it
    r.gem.set_table_budget(None);
    let tracked = r.gem.observed_seqnos();
    let ctx = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap();
    let ectx = r.eng.attach(&mut r.gem, r.s, ctx);
    r.gem.observe_seqno(0, ectx, 7).unwrap();
    assert_eq!(r.gem.observed_seqnos(), tracked + 1);
    r.gem.context_destroy(r.s, ctx, 0, &mut r.sink).unwrap();
    assert_eq!(r.gem.observed_seqnos(), tracked, "the hardware context is gone, so is its observation");
}

#[test]
fn errors_have_distinct_stable_names() {
    use std::collections::HashSet;
    use GemError::*;
    let all: Vec<GemError> = vec![
        NoMem { table: "" }, HandleSpaceExhausted { table: "" }, SessionGone { session: 0 }, HandleNotInSession { handle: 0 }, CloseHandle { handle: 0, cross: false }, CrossSessionHandle { handle: 0 },
        StaleHandle { handle: 0 }, CreateZeroSize, CreateTooBig { size: 0, max: 0 }, CreateFlags { flags: 0 },
        CreateRegionUnsupported { class: 0, instance: 0 }, CreateNeedsCpuAccess, CreateProtectedUnsupported, CreateSetPatUnsupported, MadviseInvalid { madv: 0 },
        ObjectPurged { handle: 0 }, PagesRefused { bytes: 0 }, PagesMalformed { have: 0, want: 0 }, CachingInvalid { caching: 0 }, CachingUnsupported,
        SetDomainGpuDomain { read: 0, write: 0 }, SetDomainWriteNotRead { read: 0, write: 0 }, TilingNotOk { tiling: 0, stride: 0 }, TilingNoFences,
        TilingFramebuffer { handle: 0 }, MmapFlags { flags: 0 }, MmapTypeUnavailable { kind: "" }, MmapRange { offset: 0, size: 0, object: 0 },
        MmapOffsetDenied { offset: 0 }, LeaseUnknown { lease: 0 }, UserptrRefused { size: 0 },
        ContextMissing { id: 0 }, ClientBanned { score: 0 }, ContextBanned { id: 0 }, ContextCreateFlags { flags: 0 },
        VmMissing { id: 0 }, VmPagesMissing, ParamSize { param: 0, size: 0 }, ParamUnknown { param: 0 }, PriorityRange { priority: 0 },
        ParamPermission { param: 0 }, EnginesTwice, EnginesTooMany { count: 0 }, EngineMissing { class: 0, instance: 0 }, ParamUnsupported { param: 0 },
        VmFlags { flags: 0 }, WaitFlags { flags: 0 }, WaitTimeout, WaitRestart, RequestUnknown { engine: 0, engine_ctx: 0, seqno: 0 }, ExecObjectDuplicate { handle: 0 }, ExecEngineContextMissing { ctx: 0, class: 0, instance: 0 },
        ExecRelocOwnerNoPages { handle: 0 }, ExecReserve { handle: 0, why: i915_memory_core::MemError::ZeroLength }, ExecPinnedMisplaced { handle: 0, want: 0, have: 0 },
        SyncRefused { what: "" }, EngineIndex { index: 0 },
    ];
    let names: HashSet<&str> = all.iter().map(|e| e.name()).collect();
    // CloseHandle{cross:false} has its own name; CrossSessionHandle and CloseHandle{cross:true} share one on purpose
    assert_eq!(names.len(), all.len(), "{:?}", all.iter().map(|e| e.name()).collect::<Vec<_>>());
    assert!(all.iter().all(|e| e.errno() > 0));
    assert!(all.iter().all(|e| e.name().chars().all(|c| c.is_ascii_uppercase() || c == '-')));
}
