// SPDX-License-Identifier: GPL-2.0-only
//! Retirement without a synchronous wait (Atlas 2026-10-28, `AWAIT-RETIRE-TIMEOUT` on GEM_CLOSE): the device is single-threaded and
//! finds completion only by servicing the engine, so nothing may wait for the GPU without servicing it, and GEM_CLOSE never waits.
//!
//! * GEM_CLOSE drops the name and answers at once - an idle object is unbound and freed then, a busy one is DEFERRED (it stays alive,
//!   pages and PTEs untouched) until the poll that observes its last request's seqno frees it, exactly once (`i915_gem_close_object` +
//!   `__i915_gem_free_object` from the last `i915_request_retire`, gem/i915_gem_object.c, i915_request.c).
//! * A bind that must evict a VMA a request still uses waits for THAT VMA's requests (seqno vs what the hardware wrote) and services the
//!   engine between asks; it completes once the victim retires and is refused by name (`AWAIT-RETIRE-TIMEOUT`) at the stall bound.
//! * The wait handlers advance the device before they evaluate, so a zero-timeout poll sees a completion the 4 ms timer has not.
#![cfg(feature = "p3-exec")]
mod exec_common;
#[macro_use]
mod uapi_world;
use i915_wire::msg::exec::{self, ExecObject};
use i915_wire::msg::gem::*;
use i915_wire::msg::sync::*;
use i915_wire::{errno, Opcode};
use uapi_world::*;

const OTHER_RESULT_VA: u64 = 0x3000_0000;
const OTHER_BATCH_VA: u64 = 0x1100_0000_0000;

/// `[result, batch]` softpinned at the given addresses, no fences, the default context; the batch stores VALUE at `result_va`.
fn exec_at(result: u32, batch: u32, result_va: u64, batch_va: u64, out: Option<u32>) -> Vec<u8> {
    let objs = [
        ExecObject { handle: result, offset: result_va, flags: PINNED | SUPPORTS_48B | WRITE, ..Default::default() },
        ExecObject { handle: batch, offset: batch_va, flags: PINNED | SUPPORTS_48B, ..Default::default() },
    ];
    let fa: Vec<exec::Fence> = out.iter().map(|h| exec::Fence { handle: *h, flags: FENCE_SIGNAL }).collect();
    let mut flags = exec::flags::NO_RELOC;
    if out.is_some() {
        flags |= exec::flags::FENCE_ARRAY;
    }
    let hdr = exec::ExecHeader { batch_len: 24, flags, ..Default::default() };
    let mut buf = vec![0u8; 4096];
    let n = exec::encode_request(&mut buf, hdr, &objs, &[], &fa, &[]).unwrap();
    buf.truncate(n);
    buf
}

struct Job {
    result: u32,
    seqno: u32,
}

/// New result + batch objects, the batch written, submitted at `(result_va, batch_va)`. Panics if the submission is refused.
fn job(w: &mut W, inc: u64, result_va: u64, batch_va: u64, out: Option<u32>) -> Job {
    let (batch, result) = (create(w, inc, 4096), create(w, inc, 4096));
    let bl = lease(w, inc, batch, 4096);
    for (i, d) in [MI_STORE_DWORD_IMM_GEN4, result_va as u32, (result_va >> 32) as u32, VALUE, MI_BATCH_BUFFER_END].iter().enumerate() {
        w.gm.store32(bl.grant_addr + 4 * i as u64, *d).unwrap();
    }
    let (r, head) = submit(w, inc, &exec_at(result, batch, result_va, batch_va, out));
    let head = head.unwrap_or_else(|| panic!("EXECBUFFER2 refused: errno {} log {:?}", r.errno, &w.log.0[w.log.0.len().saturating_sub(8)..]));
    Job { result, seqno: head.seqno }
}

fn relaxes(w: &W) -> u32 {
    w.chip.0.borrow().relax_calls
}
fn parks(w: &W) -> usize {
    w.chip.0.borrow().park_ms.len()
}
fn lines(w: &W, pat: &str) -> usize {
    w.log.0.iter().filter(|l| l.contains(pat)).count()
}

#[test]
fn closing_an_idle_object_while_other_work_is_in_flight_answers_at_once_and_frees_it() {
    let mut w = world();
    let inc = open(&mut w);
    let a = job(&mut w, inc, RESULT_VA, BATCH_VA, None);
    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, a.seqno);
    poll(&mut w);
    poll(&mut w);
    assert_eq!(w.svc.requests_in_flight(), 0);
    let _b = job(&mut w, inc, OTHER_RESULT_VA, OTHER_BATCH_VA, None); // in flight, never completes in this test
    assert_eq!(w.svc.requests_in_flight(), 1);

    let (pages, objs, spins, waits) = (w.svc.live_object_pages(), w.svc.gem().object_count(), relaxes(&w), parks(&w));
    let r = call(&mut w, inc, Opcode::GemClose, &GemCloseReq { handle: a.result, ..Default::default() });
    assert_eq!(r.errno, 0, "GEM_CLOSE of an IDLE object must not wait on unrelated work: {:?}", &w.log.0[w.log.0.len().saturating_sub(6)..]);
    assert_eq!((relaxes(&w), parks(&w)), (spins, waits), "the close did not yield or park: nothing of it waits on the GPU");
    assert_eq!(w.svc.live_object_pages(), pages - 1, "idle: its pages are released by the close itself");
    assert_eq!(w.svc.gem().object_count(), objs - 1);
    assert_eq!(lines(&w, "AWAIT-RETIRE"), 0);
    assert_eq!(lines(&w, "[I915-RETIRE-DEFER"), 0, "an idle close is not deferred");
}

#[test]
fn a_busy_object_closed_is_freed_by_service_once_its_request_completes_and_only_once() {
    let mut w = world();
    let inc = open(&mut w);
    let a = job(&mut w, inc, RESULT_VA, BATCH_VA, None);
    let b = job(&mut w, inc, OTHER_RESULT_VA, OTHER_BATCH_VA, None);
    assert_eq!(w.svc.requests_in_flight(), 2);
    let tl = w.svc.timeline_addrs()[0];

    let (pages, spins) = (w.svc.live_object_pages(), relaxes(&w));
    let r = call(&mut w, inc, Opcode::GemClose, &GemCloseReq { handle: a.result, ..Default::default() });
    assert_eq!(r.errno, 0, "GEM_CLOSE answers at once, busy or not");
    assert_eq!(relaxes(&w), spins, "no wait inside the close");
    assert!(call(&mut w, inc, Opcode::GemBusy, &GemBusyReq { handle: a.result, ..Default::default() }).is_err(errno::ENOENT), "the NAME is gone at once");
    assert_eq!(w.svc.live_object_pages(), pages, "busy: the pages stay until the GPU is done with them");
    assert_eq!(w.svc.closes_deferred(), 1);
    assert_eq!(lines(&w, "[I915-RETIRE-DEFER"), 1, "the deferral is announced: {:?}", w.log.find("RETIRE"));

    poll(&mut w); // nothing completed: still deferred
    assert_eq!((w.svc.live_object_pages(), w.svc.closes_deferred()), (pages, 1));

    gpu_complete(&mut w, tl, a.seqno); // the GPU finishes the request that used it; `b` is still running
    for _ in 0..4 {
        poll(&mut w);
    }
    assert_eq!(w.svc.requests_in_flight(), 1, "b is still in flight");
    assert_eq!(lines(&w, "retire failed"), 0, "the drain unbinds without a refusal: {:?}", w.log.find("retire"));
    assert_eq!(lines(&w, "AWAIT-RETIRE"), 0);
    assert_eq!(w.svc.live_object_pages(), pages - 1, "freed by service once its seqno was observed - exactly one object's pages");
    assert_eq!(w.svc.closes_deferred(), 0);
    assert_eq!(lines(&w, "[I915-RETIRE-DRAIN"), 1, "drained exactly once: {:?}", w.log.find("RETIRE"));

    gpu_complete(&mut w, tl, b.seqno);
    for _ in 0..3 {
        poll(&mut w);
    }
    assert_eq!(w.svc.live_object_pages(), pages - 1, "nothing is released twice");
    assert_eq!(lines(&w, "[I915-RETIRE-DRAIN"), 1);
}

#[test]
fn a_bind_that_evicts_a_busy_vma_completes_once_service_observes_the_victim_retire() {
    let mut w = world();
    let inc = open(&mut w);
    let a = job(&mut w, inc, RESULT_VA, BATCH_VA, None);
    let tl = w.svc.timeline_addrs()[0];
    // the GPU has finished `a` but no poll has run: only servicing the engine can find that out
    gpu_complete(&mut w, tl, a.seqno);
    let spins = relaxes(&w);
    // `b` softpins its result where `a`'s still sits: `a`'s VMA is the victim, and its request is complete only once serviced
    let _b = job(&mut w, inc, RESULT_VA, OTHER_BATCH_VA, None);
    assert_eq!(relaxes(&w), spins, "the wait parks and services; it does not spin yields");
    assert!(parks(&w) >= 1, "the wait serviced the engine at least once");
    assert_eq!(lines(&w, "AWAIT-RETIRE"), 0);
}

#[test]
fn a_bind_that_evicts_a_vma_that_never_retires_is_refused_by_name_at_the_stall_bound() {
    let mut w = world();
    let inc = open(&mut w);
    let _a = job(&mut w, inc, RESULT_VA, BATCH_VA, None); // never completes
    let (batch, result) = (create(&mut w, inc, 4096), create(&mut w, inc, 4096));
    let (spins, waits) = (relaxes(&w), parks(&w));
    let (r, head) = submit(&mut w, inc, &exec_at(result, batch, RESULT_VA, OTHER_BATCH_VA, None));
    assert!(head.is_none() && r.is_err(errno::EBUSY), "refused: errno {}", r.errno);
    assert!(lines(&w, "AWAIT-RETIRE-TIMEOUT") >= 1, "the refusal is named: {:?}", &w.log.0[w.log.0.len().saturating_sub(6)..]);
    assert_eq!(relaxes(&w), spins, "bounded by parking at the completion-poll cadence, not by spinning yields");
    // the world's stall bound is 1000 ms: 1000 / COMPLETION_POLL_MS (4) parks, each followed by an engine service
    assert_eq!(parks(&w) - waits, (1000 / i915_device::uapi::COMPLETION_POLL_MS) as usize);
    assert!(w.chip.0.borrow().park_ms[waits..].iter().all(|ms| *ms == i915_device::uapi::COMPLETION_POLL_MS));
}

#[test]
fn a_zero_timeout_wait_sees_a_completion_the_timer_has_not_serviced_yet() {
    let mut w = world();
    let inc = open(&mut w);
    let out = syncobj(&mut w, inc, 0);
    let a = job(&mut w, inc, RESULT_VA, BATCH_VA, Some(out));
    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, a.seqno); // written by the GPU; no poll has run
    let mut wait = vec![0u8; 64];
    let n = WaitReq::encode(&mut wait, 0, 0, &[out], None).unwrap();
    let r = send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).expect("a zero timeout never parks");
    assert_eq!(r.errno, 0, "SYNCOBJ_WAIT(timeout 0) services the engine before it evaluates: the fence is signalled");
    assert_eq!(call(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: a.result, flags: 0, timeout_ns: 0 }).errno, 0);
}

#[test]
fn a_zero_timeout_gem_wait_sees_a_completion_the_timer_has_not_serviced_yet() {
    let mut w = world();
    let inc = open(&mut w);
    let a = job(&mut w, inc, RESULT_VA, BATCH_VA, None);
    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, a.seqno);
    assert_eq!(call(&mut w, inc, Opcode::GemWait, &GemWaitReq { handle: a.result, flags: 0, timeout_ns: 0 }).errno, 0, "GEM_WAIT services first");
}

/// The per-VMA predicate: a VMA retires when every request that used it has completed by the WRAPPING comparison
/// (`i915_seqno_passed`, i915_request.h): a seqno issued just before the 32-bit wrap is complete once the hardware has wrapped past it.
#[test]
fn the_retirement_predicate_compares_seqnos_across_the_wrap() {
    use i915_device::uapi::vma_retired;
    use i915_gem_core::{VmId, VmaUse};
    use i915_memory_core::VmaId;
    let u = |vma: u32, seqno: u32| VmaUse { vm: VmId(1), vma: VmaId(vma), engine: 0, engine_ctx: 7, seqno };
    let uses = [u(3, 0xffff_fff0), u(3, 0xffff_fff8), u(4, 0x10)];
    let hw = |v: u32| move |_e: u8, ctx: u32| (ctx == 7).then_some(v);
    assert!(vma_retired(&uses, VmId(1), VmaId(3), hw(0x0000_0004)), "wrapped past both uses");
    assert!(!vma_retired(&uses, VmId(1), VmaId(3), hw(0xffff_fff4)), "past the first use only");
    assert!(!vma_retired(&uses, VmId(1), VmaId(4), hw(0xffff_fff9)), "0x10 is AFTER 0xffff_fff9 on the wrapped timeline");
    assert!(vma_retired(&uses, VmId(1), VmaId(4), hw(0x10)), "equal is complete");
    assert!(vma_retired(&uses, VmId(2), VmaId(3), hw(0)), "a VMA no request uses is retired (same id, other address space)");
    assert!(!vma_retired(&uses, VmId(1), VmaId(3), |_, _| None), "an unobservable timeline is never proven retired");
    // the engine index is the device's: a use on engine 1 is read on engine 1's timeline, and an engine with no rig answers None
    let other_engine = [VmaUse { engine: 1, ..u(5, 1) }];
    let only_render = |v: u32| move |e: u8, ctx: u32| (e == 0 && ctx == 7).then_some(v);
    assert!(!vma_retired(&other_engine, VmId(1), VmaId(5), only_render(100)), "a use on an engine this device does not service is never proven retired");
    let both = |v: u32| move |e: u8, ctx: u32| (e <= 1 && ctx == 7).then_some(v);
    assert!(vma_retired(&other_engine, VmId(1), VmaId(5), both(100)), "a use on the video engine is read on ITS timeline");
    assert!(!vma_retired(&[u(3, 50), VmaUse { engine: 1, ..u(3, 60) }], VmId(1), VmaId(3), move |e: u8, _| Some(if e == 0 { 100 } else { 59 })),
        "a VMA used on two engines retires only when BOTH have passed their use");
}
