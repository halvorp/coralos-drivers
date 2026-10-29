// SPDX-License-Identifier: GPL-2.0-only
//! A request that waits on a fence holds back ITSELF and its context's later requests - never the engine (Atlas 2026-10-29: frames
//! stopped at seq 490 with `[I915-STALL ... gates=1 hold=1 exp=490 hw=489]`). The old device froze every dequeue while any request was
//! gated; when the gate's producer was itself still queued behind a busy ELSP, nothing could ever publish it.
#![cfg(feature = "p3-exec")]
mod exec_common;
#[macro_use]
mod uapi_world;
use i915_device::exec::dma::ObjKind;
use i915_wire::msg::ctx::*;
use i915_wire::msg::exec;
use i915_wire::msg::sync::*;
use i915_wire::{errno, Opcode};
use uapi_world::*;

fn elsp_writes(w: &W) -> usize {
    w.chip.writes_to(0x2230).len()
}

/// The hardware acknowledges the last ELSP submission and keeps running it (CSB idle -> active only, no completion).
fn gpu_ack(w: &mut W) {
    let hwsp = *w.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap();
    let i = w.csb;
    let entry = hwsp.cpu_mapping + (0x10 + 2 * (i % 6)) as u64 * 4;
    w.mem.put(entry, 1);
    w.mem.put(entry + 4, 0);
    w.mem.put(hwsp.cpu_mapping + 0x1f * 4, i % 6);
    w.csb += 1;
}

fn ready_job(w: &mut W, inc: u64) -> (u32, u32) {
    let (batch, result) = (create(w, inc, 4096), create(w, inc, 4096));
    let bl = lease(w, inc, batch, 4096);
    write_batch(w, &bl);
    (result, batch)
}

#[test]
fn a_request_gated_on_the_fence_of_a_queued_request_does_not_stop_that_request_from_being_published() {
    let mut w = world();
    let inc = open(&mut w);
    let (result, batch) = ready_job(&mut w, inc);
    let s = syncobj(&mut w, inc, 0);
    let r1 = submit(&mut w, inc, &exec_payload(result, batch, &[], None, 0)).1.unwrap();
    assert_eq!(elsp_writes(&w), 4, "r1 is in the ELSP, not yet acknowledged");
    // a: ready, signals s; queued because the ELSP submission is still pending
    let a = submit(&mut w, inc, &exec_payload(result, batch, &[(s, FENCE_SIGNAL)], None, 0)).1.unwrap();
    assert_eq!(a.flags & exec::REP_SUBMITTED, 0, "a waits for the ports");
    // b: waits on s (a's own fence)
    let b = submit(&mut w, inc, &exec_payload(result, batch, &[(s, FENCE_WAIT)], None, 0)).1.unwrap();
    assert_eq!(b.flags & exec::REP_SUBMITTED, 0);
    assert_eq!(w.svc.gated_requests(), 1);

    let tl = w.svc.timeline_addrs()[0];
    gpu_complete(&mut w, tl, r1.seqno);
    poll(&mut w);
    poll(&mut w);
    assert_eq!(elsp_writes(&w), 8, "r1 is done and a is READY: it must reach the ELSP although b (gated on a) is queued behind it: {:?}", w.log.find("STALL"));
    gpu_complete(&mut w, tl, a.seqno);
    poll(&mut w);
    poll(&mut w);
    assert_eq!(w.svc.gated_requests(), 0, "a signalled s: b's gate opened");
    assert_eq!(elsp_writes(&w), 12, "and b reached the ELSP");
    gpu_complete(&mut w, tl, b.seqno);
    poll(&mut w);
    assert_eq!(w.svc.requests_in_flight(), 0);
}

#[test]
fn a_gated_request_holds_back_its_own_context_and_no_other() {
    let mut w = world();
    let inc = open(&mut w);
    let (result, batch) = ready_job(&mut w, inc);
    let c1 = call(&mut w, inc, Opcode::ContextCreate, &ContextCreateReq {}).body::<ContextRep>().ctx_id;
    let c2 = call(&mut w, inc, Opcode::ContextCreate, &ContextCreateReq {}).body::<ContextRep>().ctx_id;
    let s = syncobj(&mut w, inc, 0);
    // x (context c1) signals s and keeps running
    let x = submit(&mut w, inc, &exec_payload(result, batch, &[(s, FENCE_SIGNAL)], None, c1)).1.unwrap();
    assert_ne!(x.flags & exec::REP_SUBMITTED, 0);
    gpu_ack(&mut w);
    poll(&mut w);
    // b (default context) waits on s; b2 follows b in the same context; c (context c2) is independent
    let b = submit(&mut w, inc, &exec_payload(result, batch, &[(s, FENCE_WAIT)], None, 0)).1.unwrap();
    let b2 = submit(&mut w, inc, &exec_payload(result, batch, &[], None, 0)).1.unwrap();
    let before = elsp_writes(&w);
    let c = submit(&mut w, inc, &exec_payload(result, batch, &[], None, c2)).1.unwrap();
    poll(&mut w);
    assert_eq!(b.flags & exec::REP_SUBMITTED, 0, "b is gated");
    assert_eq!(b2.flags & exec::REP_SUBMITTED, 0, "b2 runs after b in the same ring: it waits with b");
    assert_ne!(c.flags & exec::REP_SUBMITTED, 0, "c is in another context: the gate on b must not hold it back ({} ELSP writes)", elsp_writes(&w));
    assert_eq!(elsp_writes(&w), before + 4, "exactly one more submission: [x, c] - neither b nor b2 in it");
    assert_eq!(w.svc.gated_requests(), 1);
}

/// The stall dump is evidence only when the registers were readable: it takes forcewake for the render domain, and a read that fails
/// prints `stage=?` (never a stage letter guessed from 0xdeaddead).
#[test]
fn the_stall_dump_reads_the_ring_awake_and_names_the_request_and_the_gate_producer() {
    let mut w = world();
    let inc = open(&mut w);
    let (result, batch) = ready_job(&mut w, inc);
    let out = syncobj(&mut w, inc, 0);
    let _r = submit(&mut w, inc, &exec_payload(result, batch, &[(out, FENCE_SIGNAL)], None, 0)).1.unwrap();
    let mut wait = vec![0u8; 64];
    let n = WaitReq::encode(&mut wait, 0, 0, &[out], None).unwrap();
    assert!(send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).unwrap().is_err(errno::ETIME));
    let line = w.log.find("[I915-STALL").last().cloned().expect("the poll dumps the engine");
    assert!(!line.contains("0xdeaddead"), "every register in the dump was read awake: {line}");
    assert!(line.contains("published=1"), "the expected request was handed to a port: {line}");
    assert!(line.contains("acthd="), "{line}");
    assert!(!line.contains("stage=?"), "readable: a real stage: {line}");
}

#[test]
fn a_stall_dump_whose_registers_cannot_be_read_says_stage_unknown_not_a_guess() {
    let mut w = world();
    let inc = open(&mut w);
    let (result, batch) = ready_job(&mut w, inc);
    let out = syncobj(&mut w, inc, 0);
    let _r = submit(&mut w, inc, &exec_payload(result, batch, &[(out, FENCE_SIGNAL)], None, 0)).1.unwrap();
    {
        let mut c = w.chip.0.borrow_mut();
        c.fw_never_acks = true; // the GT stops waking
        c.deadline_after_reads = 50; // and each forcewake wait gives up by its deadline
    }
    let mut wait = vec![0u8; 64];
    let n = WaitReq::encode(&mut wait, 0, 0, &[out], None).unwrap();
    assert!(send_raw(&mut w, inc, Opcode::SyncobjWait, &wait[..n]).unwrap().is_err(errno::ETIME));
    let line = w.log.find("[I915-STALL").last().cloned().expect("the poll dumps the engine");
    assert!(line.contains("stage=? (mmio-unreadable)"), "an unreadable register yields no stage letter: {line}");
}
