// SPDX-License-Identifier: GPL-2.0-only
//! drm_syncobj semantics: expected values are Linux's own (file:line in each comment), written out literally.
use i915_syncobj_core::*;

const A: u64 = 100; // session incarnations are opaque u64s
const B: u64 = 200;

fn sync2() -> Sync {
    let mut s = Sync::new();
    s.open_session(A).unwrap();
    s.open_session(B).unwrap();
    s
}

fn err<T: core::fmt::Debug>(r: Result<T, SyncError>) -> SyncError {
    r.expect_err("must be refused")
}

#[test]
fn handles_start_at_one_reuse_the_lowest_free_and_destroy_of_a_missing_one_is_einval() {
    let mut s = sync2();
    assert_eq!(s.create(A, 0), Ok(1)); // XA_FLAGS_ALLOC1: ids start at 1 (drm_syncobj.c:797-805)
    assert_eq!(s.create(A, 0), Ok(2));
    assert_eq!(s.create(A, 0), Ok(3));
    s.destroy(A, 2).unwrap();
    assert_eq!(s.create(A, 0), Ok(2), "the freed handle is the lowest free one (xa_alloc)");
    let e = err(s.destroy(A, 9));
    assert_eq!(e, SyncError::DestroyMissing { handle: 9 });
    assert_eq!(e.errno(), 22, "drm_syncobj.c:633-635 returns -EINVAL, not -ENOENT");
    assert_eq!(err(s.destroy(A, 0)).errno(), 22);
    let e = err(s.create(A, 2)); // `args->flags & ~DRM_SYNCOBJ_CREATE_SIGNALED` (drm_syncobj.c:831-833)
    assert_eq!((e.errno(), e.name()), (22, "SYNCOBJ-BAD-FLAGS"));
    assert_eq!(CREATE_SIGNALED, 1); // drm.h:944
}

#[test]
fn a_session_sees_only_its_own_handles() {
    let mut s = sync2();
    let h = s.create(A, 0).unwrap();
    // drm_syncobj_find looks in file_private->syncobj_xa only (drm_syncobj.c:248-262); array_find says -ENOENT (:1291-1295)
    let e = err(s.signal(B, &[h]));
    assert_eq!(e, SyncError::NoSuchSyncobj { handle: h });
    assert_eq!(e.errno(), 2);
    assert_eq!(err(s.wait_eval(B, &[h], None, 0, false, None)).errno(), 2);
    assert_eq!(err(s.destroy(B, h)).errno(), 22, "B has no such handle to destroy");
    assert_eq!(s.session_handles(A), 1);
    assert_eq!(s.session_handles(B), 0);
    assert!(matches!(s.create(999, 0), Err(SyncError::NoSuchSession { session: 999 })));
}

#[test]
fn binary_wait_follows_drm_syncobj_array_wait_timeout() {
    let mut s = sync2();
    let sig = s.create(A, CREATE_SIGNALED).unwrap();
    let un = s.create(A, 0).unwrap();
    // a signalled syncobj satisfies a wait at once (drm_syncobj.c:1092-1100)
    assert_eq!(s.wait_eval(A, &[sig], None, 0, false, None), Ok(WaitEval::Done { first: 0 }));
    // no fence and no WAIT_FOR_SUBMIT: -EINVAL (drm_syncobj.c:1072-1080)
    assert_eq!(err(s.wait_eval(A, &[un], None, 0, false, None)), SyncError::NoFence { handle: un, point: 0 });
    // WAIT_FOR_SUBMIT: the wait is pending until a fence appears (drm_syncobj.c:1127-1131)
    assert_eq!(s.wait_eval(A, &[un], None, WAIT_FOR_SUBMIT, false, None), Ok(WaitEval::Pending { first: u32::MAX }));
    let f = s.new_fence().unwrap();
    s.attach_out(A, un, 0, f).unwrap();
    assert_eq!(s.wait_eval(A, &[un], None, WAIT_FOR_SUBMIT, false, None), Ok(WaitEval::Pending { first: u32::MAX }), "fence present but unsignalled");
    assert_eq!(s.wait_eval(A, &[un], None, 0, false, None), Ok(WaitEval::Pending { first: u32::MAX }));
    s.signal_fence(f, 0);
    assert_eq!(s.wait_eval(A, &[un], None, 0, false, Some(u32::MAX)), Ok(WaitEval::Done { first: 0 }));
    // reset removes the fence again (drm_syncobj_replace_fence(NULL), drm_syncobj.c:1538-1539)
    s.reset(A, &[un]).unwrap();
    assert_eq!(err(s.wait_eval(A, &[un], None, 0, false, None)).errno(), 22);
    s.put_fence(f);
}

#[test]
fn wait_any_reports_the_first_signalled_and_wait_all_needs_every_one() {
    let mut s = sync2();
    let (h0, h1, h2) = (s.create(A, 0).unwrap(), s.create(A, CREATE_SIGNALED).unwrap(), s.create(A, CREATE_SIGNALED).unwrap());
    let f = s.new_fence().unwrap();
    s.attach_out(A, h0, 0, f).unwrap();
    // any: handle index 1 is the first signalled (`*idx = i`, drm_syncobj.c:1096-1097)
    assert_eq!(s.wait_eval(A, &[h0, h1, h2], None, 0, false, None), Ok(WaitEval::Done { first: 1 }));
    // all: h0 still pending (drm_syncobj.c:1111-1113); `first` is the first signalled of the initial pass
    assert_eq!(s.wait_eval(A, &[h0, h1, h2], None, WAIT_ALL, false, None), Ok(WaitEval::Pending { first: 1 }));
    s.signal_fence(f, 0);
    assert_eq!(s.wait_eval(A, &[h0, h1, h2], None, WAIT_ALL, false, Some(1)), Ok(WaitEval::Done { first: 1 }));
    // bad flags (drm_syncobj.c:1332-1333): WAIT_AVAILABLE is timeline-only (:1384-1389)
    assert_eq!(err(s.wait_eval(A, &[h0], None, WAIT_AVAILABLE, false, None)).errno(), 22);
    assert!(s.wait_eval(A, &[h0], None, WAIT_AVAILABLE, true, None).is_ok());
    assert_eq!(err(s.wait_eval(A, &[h0], None, 1 << 9, false, None)).errno(), 22);
    // a zero count is success (drm_syncobj.c:1335-1336)
    assert_eq!(s.wait_eval(A, &[], None, 0, false, None), Ok(WaitEval::Done { first: 0 }));
    s.put_fence(f);
}

#[test]
fn timeline_points_chain_query_wait_and_available() {
    let mut s = sync2();
    let t = s.create(A, 0).unwrap();
    // timeline signal: flags must be 0, count must be non-zero (drm_syncobj.c:1592-1596)
    assert_eq!(err(s.timeline_signal(A, &[t], &[1], 1)).errno(), 22);
    assert_eq!(err(s.timeline_signal(A, &[], &[], 0)).errno(), 22);
    s.timeline_signal(A, &[t], &[1], 0).unwrap();
    s.timeline_signal(A, &[t], &[2], 0).unwrap();
    s.timeline_signal(A, &[t], &[3], 0).unwrap();
    let mut q = [0u64; 1];
    s.query(A, &[t], 0, &mut q).unwrap();
    assert_eq!(q, [3], "every point is signalled: last signalled = head seqno (drm_syncobj.c:1707-1711)");
    // a pending payload at point 5; point 4 is covered by that node (dma_fence_chain_find_seqno, dma-fence-chain.c:90-109)
    let f = s.new_fence().unwrap();
    s.attach_out(A, t, 5, f).unwrap();
    s.query(A, &[t], 0, &mut q).unwrap();
    assert_eq!(q, [3], "last signalled: the first unsignalled node's prev_seqno");
    s.query(A, &[t], QUERY_LAST_SUBMITTED, &mut q).unwrap();
    assert_eq!(q, [5], "last submitted: the head's seqno (drm_syncobj.c:1696-1698)");
    assert_eq!(s.wait_eval(A, &[t], Some(&[3]), 0, true, None), Ok(WaitEval::Done { first: 0 }), "point 3 is signalled");
    assert_eq!(s.wait_eval(A, &[t], Some(&[4]), 0, true, None), Ok(WaitEval::Pending { first: u32::MAX }), "point 4 signals with node 5");
    assert_eq!(s.wait_eval(A, &[t], Some(&[5]), 0, true, None), Ok(WaitEval::Pending { first: u32::MAX }));
    // WAIT_AVAILABLE: the point exists, that is enough (drm_syncobj.c:1092-1094)
    assert_eq!(s.wait_eval(A, &[t], Some(&[5]), WAIT_AVAILABLE, true, None), Ok(WaitEval::Done { first: 0 }));
    // a point beyond the head has not been submitted: -EINVAL, or pending under FOR_SUBMIT (dma-fence-chain.c:98-100)
    assert_eq!(err(s.wait_eval(A, &[t], Some(&[7]), 0, true, None)), SyncError::NoFence { handle: t, point: 7 });
    assert_eq!(s.wait_eval(A, &[t], Some(&[7]), WAIT_FOR_SUBMIT, true, None), Ok(WaitEval::Pending { first: u32::MAX }));
    s.signal_fence(f, 0);
    s.query(A, &[t], 0, &mut q).unwrap();
    assert_eq!(q, [5]);
    assert_eq!(s.wait_eval(A, &[t], Some(&[5]), 0, true, Some(u32::MAX)), Ok(WaitEval::Done { first: 0 }));
    // point 0 on a timeline means "the head" (dma_fence_chain_find_seqno: no seqno = no-op, dma-fence-chain.c:96-97)
    assert_eq!(s.wait_eval(A, &[t], Some(&[0]), 0, true, None), Ok(WaitEval::Done { first: 0 }));
    s.put_fence(f);
}

#[test]
fn an_unordered_point_starts_a_new_context_with_the_larger_seqno() {
    let mut s = sync2();
    let t = s.create(A, 0).unwrap();
    s.timeline_signal(A, &[t], &[5], 0).unwrap();
    let f = s.new_fence().unwrap();
    s.attach_out(A, t, 3, f).unwrap(); // dma_fence_chain_init: seqno = max(prev->seqno, seqno) = 5 (dma-fence-chain.c:258-262)
    let mut q = [0u64];
    s.query(A, &[t], QUERY_LAST_SUBMITTED, &mut q).unwrap();
    assert_eq!(q, [5], "the head keeps seqno 5, not 3");
    s.signal_fence(f, 0);
    s.put_fence(f);
}

#[test]
fn query_of_a_binary_syncobj_is_zero_and_bad_arguments_are_einval() {
    let mut s = sync2();
    let b = s.create(A, CREATE_SIGNALED).unwrap();
    let mut q = [9u64];
    s.query(A, &[b], 0, &mut q).unwrap();
    assert_eq!(q, [0], "not a chain: point = 0 (drm_syncobj.c:1717-1718)");
    assert_eq!(err(s.query(A, &[b], 2, &mut q)).errno(), 22); // drm_syncobj.c:1668-1669
    assert_eq!(err(s.query(A, &[], 0, &mut q)).errno(), 22); // drm_syncobj.c:1672-1673
    assert_eq!(err(s.signal(A, &[])).errno(), 22); // drm_syncobj.c:1556-1557
    assert_eq!(err(s.reset(A, &[])).errno(), 22); // drm_syncobj.c:1523-1524
}

#[test]
fn transfer_copies_a_fence_to_a_binary_or_a_timeline_destination() {
    let mut s = sync2();
    let (src, bin, tl) = (s.create(A, 0).unwrap(), s.create(A, 0).unwrap(), s.create(A, 0).unwrap());
    let f = s.new_fence().unwrap();
    s.attach_out(A, src, 0, f).unwrap();
    s.transfer(A, src, 0, bin, 0, 0).unwrap(); // dst_point == 0: transfer_to_binary (drm_syncobj.c:960-980)
    s.transfer(A, src, 0, tl, 7, 0).unwrap(); // dst_point != 0: transfer_to_timeline (drm_syncobj.c:919-958)
    assert_eq!(s.wait_eval(A, &[bin], None, 0, false, None), Ok(WaitEval::Pending { first: u32::MAX }));
    assert_eq!(s.wait_eval(A, &[tl], Some(&[7]), 0, true, None), Ok(WaitEval::Pending { first: u32::MAX }));
    s.signal_fence(f, 0);
    assert_eq!(s.wait_eval(A, &[bin], None, 0, false, None), Ok(WaitEval::Done { first: 0 }));
    assert_eq!(s.wait_eval(A, &[tl], Some(&[7]), 0, true, None), Ok(WaitEval::Done { first: 0 }));
    // a missing destination is -ENOENT, a missing source with no fence is -EINVAL
    assert_eq!(err(s.transfer(A, src, 0, 99, 0, 0)).errno(), 2);
    let empty = s.create(A, 0).unwrap();
    assert_eq!(err(s.transfer(A, empty, 0, bin, 0, 0)).errno(), 22);
    assert_eq!(err(s.transfer(A, src, 0, bin, 0, WAIT_ALL)).errno(), 22, "find_fence accepts only WAIT_FOR_SUBMIT (drm_syncobj.c:444-445)");
    s.put_fence(f);
}

#[test]
fn sync_file_export_import_merge_and_close() {
    let mut s = sync2();
    let (h, g) = (s.create(A, 0).unwrap(), s.create(A, 0).unwrap());
    let (f1, f2) = (s.new_fence().unwrap(), s.new_fence().unwrap());
    s.attach_out(A, h, 0, f1).unwrap();
    s.attach_out(A, g, 0, f2).unwrap();
    let (t1, t2) = (s.export_sync_file(A, h, 0).unwrap(), s.export_sync_file(A, g, 0).unwrap());
    assert_eq!(s.file_status(A, t1), Ok(FenceStatus::Pending));
    let m = s.file_merge(A, t1, t2).unwrap();
    s.signal_fence(f1, 0);
    assert_eq!(s.file_status(A, t1), Ok(FenceStatus::Signalled(0)));
    assert_eq!(s.file_status(A, m), Ok(FenceStatus::Pending), "a merge signals when BOTH have");
    s.signal_fence(f2, -5);
    assert_eq!(s.file_status(A, m), Ok(FenceStatus::Signalled(-5)), "the first error word survives the merge");
    // import into an existing syncobj (drm_syncobj.c:726-755)
    let fresh = s.create(A, 0).unwrap();
    s.import_sync_file(A, fresh, m, 0).unwrap();
    assert_eq!(s.wait_eval(A, &[fresh], None, 0, false, None), Ok(WaitEval::Done { first: 0 }));
    assert_eq!(err(s.import_sync_file(A, 99, m, 0)).errno(), 2);
    assert_eq!(err(s.import_sync_file(A, fresh, 12345, 0)).errno(), 22, "not a sync_file in this session: -EINVAL (drm_syncobj.c:731-732)");
    // descriptors are per session
    assert_eq!(err(s.file_fence(B, t1)).errno(), 22);
    assert!(!s.file_exists(B, t1));
    s.file_close(A, t1).unwrap();
    assert!(!s.file_exists(A, t1), "a closed descriptor is gone");
    assert_eq!(err(s.file_close(A, t1)).errno(), 22);
    s.put_fence(f1);
    s.put_fence(f2);
}

#[test]
fn export_tokens_share_a_syncobj_across_sessions_and_die_with_their_owner() {
    let mut s = sync2();
    let h = s.create(A, CREATE_SIGNALED).unwrap();
    let tok = s.export(A, h).unwrap();
    let hb = s.import(B, tok).unwrap();
    assert_eq!(s.live_syncobjs(), 1, "one object, two handle tables");
    assert_eq!(s.wait_eval(B, &[hb], None, 0, false, None), Ok(WaitEval::Done { first: 0 }));
    assert_eq!(err(s.release_export(B, tok)), SyncError::NoSuchExport { token: tok }, "only the owner closes the descriptor");
    s.close_session(A);
    assert_eq!(s.live_exports(), 0, "the token died with its owner");
    assert_eq!(err(s.import(B, tok)).errno(), 22);
    assert_eq!(s.live_syncobjs(), 1, "B's handle keeps the object alive");
    s.destroy(B, hb).unwrap();
    assert_eq!(s.live_syncobjs(), 0);
}

#[test]
fn closing_a_session_frees_everything_it_held() {
    let mut s = sync2();
    let base_fences = s.live_fences();
    let mut fences = vec![];
    for _ in 0..50 {
        let h = s.create(A, 0).unwrap();
        let f = s.new_fence().unwrap();
        s.attach_out(A, h, 0, f).unwrap();
        s.export_sync_file(A, h, 0).unwrap();
        let t = s.create(A, 0).unwrap();
        s.timeline_signal(A, &[t], &[1], 0).unwrap();
        s.export(A, h).unwrap();
        fences.push(f);
    }
    assert!(s.live_syncobjs() >= 100 && s.session_files(A) == 50 && s.live_exports() == 50);
    for f in fences {
        s.put_fence(f); // the request's own reference
    }
    s.close_session(A);
    assert_eq!((s.live_syncobjs(), s.live_exports(), s.session_handles(A), s.session_files(A)), (0, 0, 0, 0));
    assert_eq!(s.live_fences(), base_fences, "no fence outlived its last holder");
}

#[test]
fn growth_is_budgeted_by_name_and_a_refusal_leaves_nothing_behind() {
    let mut s = sync2();
    s.set_budget(Some(0));
    let e = err(s.create(A, 0));
    assert_eq!((e, e.name(), e.errno()), (SyncError::NoMem { table: "syncobjs" }, "SYNC-NOMEM", 12));
    assert_eq!(s.live_syncobjs(), 0);
    s.set_budget(Some(2)); // the syncobj slot and its handle entry
    let h = s.create(A, 0).unwrap();
    assert_eq!(s.budget(), Some(0));
    // the freed slot is REUSED for free: only growth charges
    s.destroy(A, h).unwrap();
    assert!(s.create(A, 0).is_ok());
    assert_eq!(s.budget(), Some(0));
    // the next growth is refused and the failed create left no object behind
    assert!(matches!(s.create(A, 0), Err(SyncError::NoMem { .. })));
    assert_eq!(s.live_syncobjs(), 1);
    s.set_budget(None);
    assert!(s.create(A, 0).is_ok());
}

#[test]
fn creating_signalled_after_a_refusal_does_not_leak_the_stub_fence() {
    let mut s = sync2();
    s.set_budget(Some(1)); // the stub fence takes the one allowed entry; the syncobj slot is refused
    assert!(s.create(A, CREATE_SIGNALED).is_err());
    assert_eq!(s.live_fences(), 0, "the stub taken for the refused create was put back");
}

#[test]
fn many_points_are_pruned_so_a_long_timeline_does_not_grow_without_end() {
    let mut s = sync2();
    let t = s.create(A, 0).unwrap();
    for p in 1..=2000u64 {
        s.timeline_signal(A, &[t], &[p], 0).unwrap();
    }
    assert_eq!(s.timeline_points(A, t), 1, "signalled points are garbage collected from the front; only the head stays");
    // a pruned point reads as signalled
    assert_eq!(s.wait_eval(A, &[t], Some(&[17]), 0, true, None), Ok(WaitEval::Done { first: 0 }));
}
