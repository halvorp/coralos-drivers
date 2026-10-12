// SPDX-License-Identifier: GPL-2.0-only
//! Busy / wait / retire on seqno, and hang accounting (guilty, innocent, context ban, client ban).
mod common;
use common::*;
use i915_gem_core::limits::*;
use i915_gem_core::*;
use i915_uapi_core::consts::ctx_param as cp;
use i915_uapi_core::consts::exec_object as eo;

type R = Rig<16, 16>;

/// Submit `(write, batch)` on context 0 and return what the engine built.
fn submit(r: &mut R, write: u32, batch: u32, now: u64) -> ExecDone {
    let mut w = pinned(write, 0x10_0000);
    w.flags |= eo::EXEC_OBJECT_WRITE;
    r.exec(&Exec::new(0, vec![w, pinned(batch, 0x20_0000)]), now).unwrap()
}

#[test]
fn an_object_is_busy_until_the_timelines_seqno_passes_its_request() {
    let mut r = R::new();
    let (w, b) = (r.create(4096), r.create(4096));
    assert_eq!(r.gem.busy(r.s, w), Ok(0), "never used: idle");
    let d = submit(&mut r, w, b, 0);
    assert_ne!(r.gem.busy(r.s, w).unwrap(), 0);
    // the hardware has reached seqno - 1 (the initial breadcrumb): started, not finished -> still busy
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno - 1).unwrap();
    assert_ne!(r.gem.busy(r.s, w).unwrap(), 0);
    assert_eq!(r.gem.retire(&mut r.sink), 0, "nothing completed yet");
    // an older value does not un-complete or complete anything
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    assert_eq!(r.gem.busy(r.s, w), Ok(0), "i915_request_completed: seqno passed (i915_request.h:612)");
    assert_eq!(r.gem.busy(r.s, b), Ok(0));
    // complete but not yet retired: still in the table until retire() runs
    assert_eq!(r.gem.active_requests(), 1);
    assert_eq!(r.gem.retire(&mut r.sink), 1);
    assert_eq!(r.gem.active_requests(), 0);
}

#[test]
fn seqno_comparison_is_the_signed_difference_so_it_survives_u32_wraparound() {
    // i915_seqno_passed: (hw - seqno) as i32 >= 0, so "ahead" is the half-space after the request, wrap included
    let mut r = R::new();
    let (w, b) = (r.create(4096), r.create(4096));
    let d = submit(&mut r, w, b, 0);
    let busy = |r: &R| r.gem.busy(r.s, w).unwrap() & 0xffff;
    r.gem.observe_seqno(d.engine, d.engine_ctx, 0xffff_fff0).unwrap();
    assert_eq!(busy(&r), 1, "0xfffffff0 is BEFORE seqno 2 in wrap order");
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno.wrapping_add(0x8000_0000)).unwrap();
    assert_eq!(busy(&r), 1, "exactly half a space ahead reads as behind");
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno.wrapping_add(0x7fff_ffff)).unwrap();
    assert_eq!(busy(&r), 0, "one less is ahead: passed");
}

#[test]
fn busy_reports_the_class_that_reads_and_the_class_that_writes() {
    let mut r = R::new();
    let (shared, b1, b2) = (r.create(4096), r.create(4096), r.create(4096));
    // render writes, video reads: read bits of both classes, write id of render (class 0 -> 1)
    let mut wo = pinned(shared, 0x10_0000);
    wo.flags |= eo::EXEC_OBJECT_WRITE;
    r.exec(&Exec::new(0, vec![wo, pinned(b1, 0x20_0000)]), 0).unwrap();
    r.exec(&Exec::new(i915_uapi_core::consts::exec::I915_EXEC_BSD, vec![pinned(shared, 0x10_0000), pinned(b2, 0x30_0000)]), 1).unwrap();
    // read flags: 0x10000 << class: RENDER = 0 -> 0x10000, VIDEO = 2 -> 0x40000; write id = class + 1 = 1
    assert_eq!(r.gem.busy(r.s, shared).unwrap(), 1 | 0x1_0000 | 0x4_0000);
    assert_eq!(r.gem.busy(r.s, b1).unwrap(), 0x1_0000);
    assert_eq!(r.gem.busy(r.s, b2).unwrap(), 0x4_0000);
}

#[test]
fn wait_semantics_follow_gem_wait_ioctl() {
    let mut r = R::new();
    let (w, b) = (r.create(4096), r.create(4096));
    assert_eq!(r.gem.wait_plan(r.s, w, 0, 0), Ok(Wait::Idle), "idle object: returns at once whatever the timeout");
    assert_eq!(r.gem.wait_plan(r.s, w, 1, 5), Err(GemError::WaitFlags { flags: 1 }), "flags != 0: -EINVAL (gem/i915_gem_wait.c:240-241)");
    assert_eq!(r.gem.wait_plan(r.s, 0xdead, 0, 0).unwrap_err().errno(), 2);
    let d = submit(&mut r, w, b, 0);
    // timeout 0 is the busy ioctl: -ETIME while incomplete
    let t = r.gem.wait_plan(r.s, w, 0, 0).unwrap_err();
    assert_eq!((t, t.errno()), (GemError::WaitTimeout, 62));
    // positive: block for that long; negative: block forever (MAX_SCHEDULE_TIMEOUT)
    assert_eq!(r.gem.wait_plan(r.s, w, 0, 5_000), Ok(Wait::Blocked { pending: 1, timeout: WaitTimeout::Ns(5_000) }));
    assert_eq!(r.gem.wait_plan(r.s, w, 0, -1), Ok(Wait::Blocked { pending: 1, timeout: WaitTimeout::Forever }));
    // the object a batch merely READS is waited on too (I915_WAIT_ALL)
    assert_eq!(r.gem.wait_plan(r.s, b, 0, -1), Ok(Wait::Blocked { pending: 1, timeout: WaitTimeout::Forever }));
    let mut out = [RequestId(0); 4];
    assert_eq!(r.gem.pending_requests(r.gem.lookup_object(r.s, w).unwrap(), &mut out), 1);
    assert_eq!(out[0], d.request);
    // the GPU finishes: the next plan is Idle
    r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
    assert_eq!(r.gem.wait_plan(r.s, w, 0, 0), Ok(Wait::Idle));
    assert_eq!(r.gem.wait_plan(r.s, w, 0, -1), Ok(Wait::Idle));
}

#[test]
fn wait_finish_accounts_the_remaining_timeout_like_the_ioctl() {
    const JIFFY: u64 = 4_000_000; // HZ = 250
    // woke up because the object went idle: remaining = timeout - elapsed, result 0 (gem/i915_gem_wait.c:255-258)
    assert_eq!(wait_finish(1_000_000_000, 400_000_000, false, JIFFY), (600_000_000, Ok(())));
    // elapsed beyond the timeout never goes negative (gem/i915_gem_wait.c:257-258)
    assert_eq!(wait_finish(1_000, 5_000, false, JIFFY), (0, Ok(())));
    // timed out with plenty of the budget left (clock skew): restart (-EAGAIN, gem/i915_gem_wait.c:271-272)
    assert_eq!(wait_finish(1_000_000_000, 400_000_000, true, JIFFY), (600_000_000, Err(GemError::WaitRestart)));
    // timed out with less than a jiffy left: that is a real timeout, remaining is zeroed (gem/i915_gem_wait.c:267-268)
    assert_eq!(wait_finish(10_000_000, 7_000_000, true, JIFFY), (0, Err(GemError::WaitTimeout)));
    assert_eq!(wait_finish(10_000_000, 12_000_000, true, JIFFY), (0, Err(GemError::WaitTimeout)));
    // a zero or negative timeout is returned untouched (the block at gem/i915_gem_wait.c:255 is only for > 0)
    assert_eq!(wait_finish(0, 123, true, JIFFY), (0, Err(GemError::WaitTimeout)));
    assert_eq!(wait_finish(-1, 123, false, JIFFY), (-1, Ok(())));
    assert_eq!(wait_finish(-1, 123, true, JIFFY), (-1, Err(GemError::WaitTimeout)));
}

// ---- hangs --------------------------------------------------------------------------------------------------------------------------------

fn hang(r: &mut R, ctx: u32, now: u64, guilty: bool) -> ResetVerdict {
    let (a, b) = (r.create(4096), r.create(4096));
    let mut e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    e.ctx = ctx as u64;
    let d = r.exec(&e, now).unwrap();
    r.gem.request_reset(d.engine, d.engine_ctx, d.seqno, guilty, now).unwrap()
}

#[test]
fn three_hangs_inside_120_seconds_ban_a_recoverable_context() {
    let mut r = R::new();
    assert!(!hang(&mut r, 0, 1_000, true).banned);
    assert!(!hang(&mut r, 0, 2_000, true).banned);
    let v = hang(&mut r, 0, 3_000, true);
    assert!(v.banned, "hang_timestamp[0] (the first hang) is within 120 s of the third (gt/intel_reset.c:94-102)");
    assert_eq!(r.gem.context_banned(r.s, 0), Ok(true));
    assert_eq!(r.gem.reset_stats(r.s, 0, 0, 0).unwrap().batch_active, 3, "guilty_count");
}

#[test]
fn hangs_further_apart_than_120_seconds_do_not_ban() {
    let mut r = R::new();
    for (i, t) in [1_000u64, 200_000, 400_000, 600_000].iter().enumerate() {
        assert!(!hang(&mut r, 0, *t, true).banned, "hang {i} at {t} ms");
    }
    // exactly the window edge: time_before(now, prev + 120 s) is strict
    let mut r = R::new();
    hang(&mut r, 0, 0, true);
    hang(&mut r, 0, 60_000, true);
    assert!(!hang(&mut r, 0, CONTEXT_FAST_HANG_MS, true).banned, "now == prev + window is NOT before it");
    let mut r = R::new();
    hang(&mut r, 0, 0, true);
    hang(&mut r, 0, 60_000, true);
    assert!(hang(&mut r, 0, CONTEXT_FAST_HANG_MS - 1, true).banned, "one millisecond inside the window is");
}

#[test]
fn a_context_that_is_not_recoverable_is_banned_by_its_first_hang() {
    let mut r = R::new();
    r.gem.context_setparam(r.s, 0, &ContextParam::new(cp::I915_CONTEXT_PARAM_RECOVERABLE, 0)).unwrap();
    let v = hang(&mut r, 0, 1_000, true);
    assert!(v.banned, "banned = !recoverable (gt/intel_reset.c:99)");
}

#[test]
fn an_unbannable_context_is_never_banned_and_does_not_score_the_client() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let admin = r.gem.open_session(SessionCaps { sys_admin: true, sys_nice: false }, r.pool.init(), &mut sink).unwrap();
    r.gem.context_setparam(admin, 0, &ContextParam::new(cp::I915_CONTEXT_PARAM_BANNABLE, 0)).unwrap();
    let mut eng = Engines::new();
    eng.attach(&mut r.gem, admin, 0);
    let (a, b) = (r.gem.create(admin, 4096).unwrap().handle, r.gem.create(admin, 4096).unwrap().handle);
    for i in 0..(MAX_GUILTY_BEFORE_BAN + 4) {
        let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
        let d = with_req(&e, |req| r.gem.execbuffer2(admin, &r.sync, req, &mut r.pages, &mut TablesAdapter(&mut r.pool), &mut eng, &mut sink, i as u64)).unwrap();
        let v = r.gem.request_reset(d.engine, d.engine_ctx, d.seqno, true, 1_000 * i as u64).unwrap();
        assert!(!v.banned, "'cool contexts are too cool to be banned' (gt/intel_reset.c:84-87)");
        r.gem.observe_seqno(d.engine, d.engine_ctx, d.seqno).unwrap();
        r.gem.retire(&mut sink);
    }
    assert_eq!(r.gem.ban_score(admin), Ok(0));
    assert_eq!(r.gem.reset_stats(admin, 0, 0, 0).unwrap().batch_active, MAX_GUILTY_BEFORE_BAN + 4);
}

#[test]
fn the_port_bound_bans_a_slow_hanger_that_linux_would_never_ban() {
    // one hang per 121 s forever: Linux never bans it; MAX_GUILTY_BEFORE_BAN does
    let mut r = R::new();
    for i in 1..MAX_GUILTY_BEFORE_BAN {
        let v = hang(&mut r, 0, i as u64 * 121_000, true);
        assert!(!v.banned, "hang {i}");
    }
    let v = hang(&mut r, 0, MAX_GUILTY_BEFORE_BAN as u64 * 121_000, true);
    assert!(v.banned, "guilty_count reached {MAX_GUILTY_BEFORE_BAN}");
}

#[test]
fn an_innocent_victim_is_counted_but_never_banned() {
    let mut r = R::new();
    for i in 0..20u64 {
        assert!(!hang(&mut r, 0, i * 100, false).banned);
    }
    let s = r.gem.reset_stats(r.s, 0, 0, 0).unwrap();
    assert_eq!((s.batch_active, s.batch_pending), (0, 20), "guilty_count / active_count (gem/i915_gem_context.c:2660-2661)");
    assert_eq!(r.gem.ban_score(r.s), Ok(0));
}

#[test]
fn banning_fails_the_contexts_other_queued_requests_with_eio() {
    let mut r = R::new();
    r.gem.context_setparam(r.s, 0, &ContextParam::new(cp::I915_CONTEXT_PARAM_RECOVERABLE, 0)).unwrap();
    let (a, b) = (r.create(4096), r.create(4096));
    let e = Exec::new(0, vec![pinned(a, 0x10_0000), pinned(b, 0x20_0000)]);
    let d1 = r.exec(&e, 0).unwrap();
    let _d2 = r.exec(&e, 1).unwrap();
    let _d3 = r.exec(&e, 2).unwrap();
    let v = r.gem.request_reset(d1.engine, d1.engine_ctx, d1.seqno, true, 5).unwrap();
    assert_eq!((v.banned, v.cancelled), (true, 2));
}

#[test]
fn the_client_score_rises_with_bans_and_fast_hangs_and_blocks_new_contexts_at_nine() {
    let mut r = R::new();
    let ext = i915_uapi_core::consts::misc::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let vm = r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink).unwrap() as u64;
    let mk = |r: &mut R| {
        let c = r
            .gem
            .context_create(r.s, ext, &[ContextParam::new(cp::I915_CONTEXT_PARAM_VM, vm), ContextParam::new(cp::I915_CONTEXT_PARAM_RECOVERABLE, 0)], None, &mut r.sink)
            .unwrap();
        r.eng.attach(&mut r.gem, r.s, c);
        c
    };
    // each context is banned by its first hang: +3 (I915_CLIENT_SCORE_CONTEXT_BAN), and +1 if the client hung < 60 s ago
    let c1 = mk(&mut r);
    assert!(hang(&mut r, c1, 1_000, true).banned);
    assert_eq!(r.gem.ban_score(r.s), Ok(3), "first hang of the client: no 'fast' bonus");
    let c2 = mk(&mut r);
    assert!(hang(&mut r, c2, 10_000, true).banned);
    assert_eq!(r.gem.ban_score(r.s), Ok(3 + 3 + 1), "second ban 9 s after the first: +1 fast hang");
    let c3 = mk(&mut r);
    assert!(hang(&mut r, c3, 200_000, true).banned);
    assert_eq!(r.gem.ban_score(r.s), Ok(7 + 3), "a hang more than 60 s after the previous gets no bonus (gt/intel_reset.c:50-51)");
    assert_eq!(CLIENT_SCORE_BANNED, 9);
    let err = r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink).unwrap_err();
    assert_eq!(err, GemError::ClientBanned { score: 10 });
    assert_eq!(err.errno(), 5);
    // existing contexts still work and existing sessions are unaffected
    let mut sink = Sink::default();
    let other = r.gem.open_session(SessionCaps::default(), r.pool.init(), &mut sink).unwrap();
    assert!(r.gem.context_create(other, 0, &[], Some(r.pool.init()), &mut sink).is_ok());
}

#[test]
fn the_client_is_banned_at_exactly_score_nine_and_not_at_eight() {
    let mut r = R::new();
    let ext = i915_uapi_core::consts::misc::I915_CONTEXT_CREATE_FLAGS_USE_EXTENSIONS;
    let vm = r.gem.vm_create(r.s, 0, 0, r.pool.init(), &mut r.sink).unwrap() as u64;
    let mk = |r: &mut R| {
        let c = r
            .gem
            .context_create(r.s, ext, &[ContextParam::new(cp::I915_CONTEXT_PARAM_VM, vm), ContextParam::new(cp::I915_CONTEXT_PARAM_RECOVERABLE, 0)], None, &mut r.sink)
            .unwrap();
        r.eng.attach(&mut r.gem, r.s, c);
        c
    };
    // three bans more than 60 s apart: 3 + 3 + 3, no fast-hang bonus
    for (i, t) in [100_000u64, 200_000, 300_000].iter().enumerate() {
        let c = mk(&mut r);
        assert_eq!(r.gem.ban_score(r.s), Ok(3 * i as u32), "a client at {} may still create contexts", 3 * i);
        assert!(hang(&mut r, c, *t, true).banned);
    }
    assert_eq!(r.gem.ban_score(r.s), Ok(CLIENT_SCORE_BANNED));
    assert_eq!(r.gem.context_create(r.s, 0, &[], Some(r.pool.init()), &mut r.sink), Err(GemError::ClientBanned { score: 9 }));
}

#[test]
fn reset_count_is_reported_only_to_privileged_callers() {
    let mut r = R::new();
    let mut sink = Sink::default();
    let admin = r.gem.open_session(SessionCaps { sys_admin: true, sys_nice: false }, r.pool.init(), &mut sink).unwrap();
    r.gem.note_gpu_reset();
    r.gem.note_gpu_reset();
    assert_eq!(r.gem.reset_stats(r.s, 0, 0, 0).unwrap().reset_count, 0, "capable(CAP_SYS_ADMIN) only (gem/i915_gem_context.c:2656-2658)");
    assert_eq!(r.gem.reset_stats(admin, 0, 0, 0).unwrap().reset_count, 2);
    assert!(r.gem.reset_stats(r.s, 0, 1, 0).is_err());
    assert!(r.gem.reset_stats(r.s, 0, 0, 1).is_err());
    assert_eq!(r.gem.reset_stats(r.s, 5, 0, 0), Err(GemError::ContextMissing { id: 5 }));
}

#[test]
fn a_reset_for_an_unknown_request_is_refused_by_name() {
    let mut r = R::new();
    let err = r.gem.request_reset(0, 0, 99, true, 0).unwrap_err();
    assert_eq!(err, GemError::RequestUnknown { engine: 0, engine_ctx: 0, seqno: 99 });
}
