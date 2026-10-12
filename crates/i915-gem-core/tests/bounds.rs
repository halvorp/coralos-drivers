// SPDX-License-Identifier: GPL-2.0-only
//! Every bound that is not already pinned by a behavioural test: the declared limits are the documented numbers, and the tables
//! that hold them refuse the next entry by name.
mod common;
use common::*;
use i915_gem_core::limits::*;
use i915_gem_core::*;

type R = Rig<16, 16>;

#[test]
fn the_declared_limits_are_these_numbers() {
    // A change to a bound is a deliberate act: it must be made here too.
    assert_eq!(MAX_SESSIONS, 8);
    assert_eq!(MAX_HANDLES, 256);
    assert_eq!(MAX_OBJECTS, 512);
    assert_eq!(MAX_CTX_PER_SESSION, 16);
    assert_eq!(MAX_CONTEXTS, 32);
    assert_eq!(MAX_VMS, 8);
    assert_eq!(MAX_USER_ENGINES, 64);
    assert_eq!(MAX_ACTIVE, 64);
    assert_eq!(MAX_EXEC_OBJECTS, 128);
    assert_eq!(MAX_HW_SEQNOS, 32 * 5);
    // Linux's own constants, from the headers
    assert_eq!(HANG_HISTORY, 2); // gem/i915_gem_context_types.h:388
    assert_eq!(CONTEXT_FAST_HANG_MS, 120_000); // gem/i915_gem_context_types.h:389
    assert_eq!(CLIENT_FAST_HANG_MS, 60_000); // i915_file_private.h:101
    assert_eq!((CLIENT_SCORE_HANG_FAST, CLIENT_SCORE_CONTEXT_BAN, CLIENT_SCORE_BANNED), (1, 3, 9)); // i915_file_private.h:100-103
}

#[test]
fn the_active_request_table_is_bounded_and_retiring_makes_room() {
    let mut r = R::new();
    r.eng.auto_retire = true; // the engine's own request table is not the bound under test
    let (a, b) = (r.create(4096), r.create(4096));
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    let mut last = None;
    for i in 0..MAX_ACTIVE {
        last = Some(r.exec(&e, i as u64).unwrap_or_else(|e| panic!("request {i}: {e:?}")));
    }
    assert_eq!(r.gem.active_requests(), MAX_ACTIVE);
    let err = r.exec(&e, 0).unwrap_err();
    assert_eq!(err, GemError::ActiveTableFull { cap: MAX_ACTIVE });
    assert_eq!(err.errno(), 12);
    // the table was checked BEFORE the engine was asked: no request was built that nobody tracks
    let before = r.eng.rcs().requests().len();
    let _ = r.exec(&e, 0);
    assert_eq!(r.eng.rcs().requests().len(), before);
    let d = last.unwrap();
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    assert_eq!(r.gem.retire(&mut r.sink), MAX_ACTIVE, "all requests of one timeline complete when its seqno passes the last");
    assert!(r.exec(&e, 0).is_ok());
}

#[test]
fn the_seqno_observation_table_is_bounded() {
    let mut r = R::new();
    for i in 0..MAX_HW_SEQNOS as u32 {
        r.gem.observe_seqno(0, 1000 + i, 5).unwrap();
    }
    assert_eq!(r.gem.observe_seqno(0, 1_000_000, 5), Err(GemError::SeqnoTableFull { cap: MAX_HW_SEQNOS }));
    // an engine already known updates in place and needs no new slot
    assert!(r.gem.observe_seqno(0, 1000, 6).is_ok());
    assert_eq!(r.gem.observe_seqno(9, 0, 0), Err(GemError::EngineIndex { index: 9 }), "no such engine");
}

#[test]
fn errors_have_distinct_stable_names() {
    use std::collections::HashSet;
    use GemError::*;
    let all: Vec<GemError> = vec![
        SessionTableFull { cap: 0 }, SessionGone { session: 0 }, HandleNotInSession { handle: 0 }, CloseHandle { handle: 0, cross: false }, CrossSessionHandle { handle: 0 },
        StaleHandle { handle: 0 }, HandleTableFull { cap: 0 }, ObjectTableFull { cap: 0 }, CreateZeroSize, CreateTooBig { size: 0, max: 0 }, CreateFlags { flags: 0 },
        CreateRegionUnsupported { class: 0, instance: 0 }, CreateNeedsCpuAccess, CreateProtectedUnsupported, CreateSetPatUnsupported, MadviseInvalid { madv: 0 },
        ObjectPurged { handle: 0 }, PagesRefused { bytes: 0 }, PagesMalformed { have: 0, want: 0 }, CachingInvalid { caching: 0 }, CachingUnsupported,
        SetDomainGpuDomain { read: 0, write: 0 }, SetDomainWriteNotRead { read: 0, write: 0 }, TilingNotOk { tiling: 0, stride: 0 }, TilingNoFences,
        TilingFramebuffer { handle: 0 }, MmapFlags { flags: 0 }, MmapTypeUnavailable { kind: "" }, MmapRange { offset: 0, size: 0, object: 0 },
        MmapOffsetDenied { offset: 0 }, LeaseTableFull { cap: 0 }, MmapTableFull { cap: 0 }, LeaseUnknown { lease: 0 }, UserptrRefused { size: 0 },
        ContextMissing { id: 0 }, ContextTableFull { cap: 0 }, ClientBanned { score: 0 }, ContextBanned { id: 0 }, ContextCreateFlags { flags: 0 },
        VmMissing { id: 0 }, VmTableFull { cap: 0 }, VmPagesMissing, ParamSize { param: 0, size: 0 }, ParamUnknown { param: 0 }, PriorityRange { priority: 0 },
        ParamPermission { param: 0 }, EnginesTwice, EnginesTooMany { count: 0 }, EngineMissing { class: 0, instance: 0 }, ParamUnsupported { param: 0 },
        VmFlags { flags: 0 }, WaitFlags { flags: 0 }, WaitTimeout, WaitRestart, RequestUnknown { engine: 0, engine_ctx: 0, seqno: 0 }, ActiveTableFull { cap: 0 },
        SeqnoTableFull { cap: 0 }, ExecTooManyObjects { count: 0, max: 0 }, ExecObjectDuplicate { handle: 0 }, ExecEngineContextMissing { ctx: 0, class: 0, instance: 0 },
        ExecRelocOwnerNoPages { handle: 0 }, ExecReserve { handle: 0, why: i915_memory_core::MemError::ZeroLength }, ExecPinnedMisplaced { handle: 0, want: 0, have: 0 },
        SyncRefused { what: "" }, EngineIndex { index: 0 },
    ];
    let names: HashSet<&str> = all.iter().map(|e| e.name()).collect();
    // CloseHandle{cross:false} has its own name; CrossSessionHandle and CloseHandle{cross:true} share one on purpose
    assert_eq!(names.len(), all.len(), "{:?}", all.iter().map(|e| e.name()).collect::<Vec<_>>());
    assert!(all.iter().all(|e| e.errno() > 0));
    assert!(all.iter().all(|e| e.name().chars().all(|c| c.is_ascii_uppercase() || c == '-')));
}
