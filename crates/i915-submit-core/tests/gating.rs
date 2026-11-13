// SPDX-License-Identifier: GPL-2.0-only
//! Requests that wait on a fence (Atlas 2026-10-29 deadlock): a gated request is in its ring but not READY. `execlists_dequeue`
//! skips it and every later request of its context, keeps publishing ready requests of other contexts, and publishes a context's
//! `RING_TAIL` only up to its last ready request (`execlists_update_context`: `CTX_RING_TAIL = rq->tail` of the last request merged
//! into the port, gt/intel_execlists_submission.c:672-721). Before this, the device froze the WHOLE engine while any request was
//! gated, and a request gated on the fence of an earlier, not-yet-published request deadlocked the engine.
use i915_devinfo_core::lookup;
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass};
use i915_submit_core::engine::*;
use i915_submit_core::execlists::Port;
use i915_submit_core::lrc_reg::*;
use i915_submit_core::SubmitError;

const STATUS_PAGE: u32 = 0x0004_0000;
const BATCH: BatchSpec = BatchSpec { offset: 0x2345_6000, flags: 0, nopreempt: false };

struct Rig {
    e: Engine,
    mem: [Vec<u32>; 3],
}

impl Rig {
    fn new() -> Rig {
        let d = lookup(0x591c).unwrap();
        let info = engines_for(&d.info).unwrap().as_slice().iter().copied().find(|e| e.class == EngineClass::Render).unwrap();
        let cfg = EngineConfig { status_page_ggtt: STATUS_PAGE, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: 1000 };
        let mut e = Engine::new(&info, 9, cfg).unwrap();
        for n in 0..3u32 {
            let spec = ContextSpec {
                lrca: 0x0100_0000 + n * 0x0001_0000,
                ring_size: 4096,
                ring_ggtt: 0x0200_0000 + n * 0x1000,
                ppgtt: i915_memory_core::PpgttRoots::Pml4(0x1_0000_0000 + n as u64 * 0x1000),
                rpcs: 0,
                hwsp_ggtt: 0x0003_0000 + n * 0x40,
                initial_breadcrumb: true,
                default_state: None,
            };
            e.create_context(&spec).unwrap();
        }
        Rig { e, mem: [vec![0; 1024], vec![0; 1024], vec![0; 1024]] }
    }
    fn rq(&mut self, ctx: u32) -> RequestRef {
        self.e.build_request(ctx, &mut self.mem[ctx as usize], &BATCH).unwrap()
    }
    fn gate(&mut self, r: RequestRef, on: bool) {
        self.e.set_gated(r.ctx, r.seqno, on).unwrap();
    }
    /// The hardware acknowledges the last submission (idle -> active, or a lite restore), CSB entry `idx`.
    fn csb(&mut self, idx: u32, status: u64) -> Vec<Port> {
        self.e.process_csb(idx, |i| if i == idx { status } else { u64::MAX }, |_| panic!("mmio")).unwrap().inactive.as_slice().to_vec()
    }
    /// `rq->tail` as built (before the first submission turns it into `wa_tail`): the two WaIdleLiteRestore NOOPs are 8 bytes.
    fn built_tail(&self, r: RequestRef) -> u32 {
        let q = self.e.requests().iter().find(|q| q.ctx == r.ctx && q.seqno == r.seqno).unwrap();
        if q.submitted { q.wa_tail - 8 } else { q.tail }
    }
    fn submitted(&self, r: RequestRef) -> bool {
        self.e.requests().iter().any(|q| q.ctx == r.ctx && q.seqno == r.seqno && q.submitted)
    }
}

fn port(r: RequestRef) -> Port {
    Port { ctx: r.ctx, seqno: r.seqno }
}

/// Negative control 1: the producer of a gate is queued behind a busy ELSP, and the consumer (another context) waits on it.
#[test]
fn a_ready_request_queued_behind_busy_ports_is_published_although_a_later_request_waits_on_its_fence() {
    let mut r = Rig::new();
    let x = r.rq(0);
    let y = r.rq(1);
    assert_eq!(r.e.dequeue(1).unwrap().unwrap().ports, [Some(port(x)), Some(port(y))], "both ports busy");
    let a = r.rq(2); // ready, exports the fence b waits on
    let b = r.rq(1); // gated on a's fence
    r.gate(b, true);
    assert!(r.csb(0, 0x1).is_empty()); // idle -> active
    // x and y complete; the hardware goes idle
    r.e.observe_seqno(0, x.seqno).unwrap();
    r.e.observe_seqno(1, y.seqno).unwrap();
    assert_eq!(r.csb(1, 0x4 | 0x10).len(), 1);
    assert_eq!(r.csb(2, 0x8 | 0x10).len(), 1);
    assert!(r.e.execlists().is_idle());
    let plan = r.e.dequeue(2).unwrap().expect("a is ready: it must be published even though b is gated");
    assert_eq!(plan.ports, [Some(port(a)), None], "a alone; b (gated) is not");
    assert!(r.submitted(a) && !r.submitted(b));
    // a completes, its fence signals: b becomes ready and goes next
    assert!(r.csb(3, 0x1).is_empty());
    r.e.observe_seqno(2, a.seqno).unwrap();
    assert_eq!(r.csb(4, 0x8 | 0x10).len(), 1);
    r.gate(b, false);
    assert_eq!(r.e.dequeue(3).unwrap().unwrap().ports, [Some(port(b)), None]);
}

/// A request gated on an EXTERNAL fence: other contexts run; its own context's followers wait with it.
#[test]
fn a_gated_request_holds_back_its_context_followers_and_nothing_else() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    let a2 = r.rq(0);
    let c1 = r.rq(2);
    r.gate(a1, true);
    let plan = r.e.dequeue(1).unwrap().expect("c1 is ready");
    assert_eq!(plan.ports, [Some(port(c1)), None], "a1 is gated and a2 follows it in the same ring: only c1");
    assert!(!r.submitted(a1) && !r.submitted(a2));
    assert!(r.csb(0, 0x1).is_empty());
    // the external fence signals: a1 and a2 merge into one port behind the running c1
    r.gate(a1, false);
    let plan = r.e.dequeue(2).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(port(c1)), Some(port(a2))]);
    assert!(r.submitted(a1) && r.submitted(a2));
}

/// Negative control 2: the RING_TAIL published for a context is the tail of its last READY request, never past a gated one.
#[test]
fn ring_tail_stops_at_the_last_ready_request_and_lite_restores_forward_when_the_gate_opens() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    let a2 = r.rq(0);
    r.gate(a2, true);
    let (t1, t2) = (r.built_tail(a1), r.built_tail(a2));
    assert!(t2 > t1);
    let plan = r.e.dequeue(1).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(port(a1)), None]);
    assert_eq!(plan.image_stores.as_slice().len(), 1);
    assert_eq!(plan.image_stores.as_slice()[0].value, t1, "CTX_RING_TAIL = a1's tail: the GPU cannot run into the gated a2");
    assert_eq!(r.e.context_regs(0).unwrap().get(CTX_RING_TAIL), t1);
    assert!(r.csb(0, 0x1).is_empty());
    r.gate(a2, false);
    let plan = r.e.dequeue(2).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(port(a2)), None], "same context in flight: lite restore");
    assert_eq!(plan.image_stores.as_slice()[0].value, t2, "now the tail moves on to a2's");
    assert_eq!(plan.descs[0] & CTX_DESC_FORCE_RESTORE, 0, "the tail moved forward: no forced restore");
}

#[test]
fn a_gate_on_a_submitted_or_unknown_request_is_refused_by_name() {
    let mut r = Rig::new();
    let a = r.rq(0);
    r.e.dequeue(1).unwrap().unwrap();
    let e = r.e.set_gated(a.ctx, a.seqno, true).unwrap_err();
    assert_eq!(e, SubmitError::RequestAlreadySubmitted { ctx: a.ctx, seqno: a.seqno });
    assert_eq!(e.name(), "REQUEST-ALREADY-SUBMITTED");
    assert_eq!(r.e.set_gated(1, 99, true).unwrap_err(), SubmitError::NoSuchRequest { ctx: 1, seqno: 99 });
}
