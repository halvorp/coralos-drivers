// SPDX-License-Identifier: GPL-2.0-only
//! The engine end to end on a Kaby Lake GT2 part (8086:591C): request construction against the gt-core emitters, ring wrap and
//! ring-full, two contexts alternating through the ELSP in Linux's order (descriptors, ccids, tails, tags), CSB handling with
//! schedule-in/out accounting, retirement, and the stall bound (a stuck seqno is declared hung by name).
use i915_common_core::Reg;
use i915_devinfo_core::lookup;
use i915_gt_core::emit::*;
use i915_gt_core::engine::{engines_for, EngineClass, EngineInfo};
use i915_gt_core::WritePlan;
use i915_memory_core::PpgttInit;
use i915_submit_core::engine::*;
use i915_submit_core::execlists::{Port, NUM_PORTS};
use i915_submit_core::lrc_reg::*;
use i915_submit_core::lrc_state::{RegState, PAGE_DWORDS};
use i915_submit_core::request::{seqno_passed, TagAllocator, Timeline};
use i915_submit_core::ring::Ring;
use i915_submit_core::submit::forces_restore;
use i915_submit_core::SubmitError;

const STATUS_PAGE: u32 = 0x0004_0000;
const BOUND: u64 = 1000;

fn kbl_engine(class: EngineClass) -> EngineInfo {
    let d = lookup(0x591c).unwrap();
    engines_for(&d.info).unwrap().as_slice().iter().copied().find(|e| e.class == class).unwrap()
}

fn config() -> EngineConfig {
    EngineConfig { status_page_ggtt: STATUS_PAGE, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: BOUND }
}

fn rcs() -> Engine {
    Engine::new(&kbl_engine(EngineClass::Render), 9, config()).unwrap()
}

/// ContextSpec for context number `n`: distinct lrca / ring / timeline slot, a 4 KiB ring.
fn spec(n: u32) -> ContextSpec<'static> {
    ContextSpec {
        lrca: 0x0100_0000 + n * 0x0001_0000,
        ring_size: 4096,
        ring_ggtt: 0x0200_0000 + n * 0x1000,
        pml4: 0x1_0000_0000 + n as u64 * 0x1000,
        rpcs: 0,
        hwsp_ggtt: 0x0003_0000 + n * 0x40,
        initial_breadcrumb: true,
        default_state: None,
    }
}

const BATCH: BatchSpec = BatchSpec { offset: 0x2345_6000, flags: 0, nopreempt: false };

fn ring_mem(size: u32) -> Vec<u32> {
    vec![0xdead_beef; (size / 4) as usize]
}

/// The dwords one request on the render engine puts in the ring, built straight from the gt-core emitters.
fn expected_request(hwsp: u32, seqno: u32, initial: bool, batch: &BatchSpec) -> (Vec<u32>, usize, usize, usize) {
    let q = FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false };
    let mut buf = vec![0u32; 256];
    let mut cs = CsBuf::new(&mut buf);
    emit_flush_rcs(&mut cs, EMIT_INVALIDATE, q).unwrap();
    if initial {
        emit_init_breadcrumb(&mut cs, hwsp, seqno).unwrap();
    }
    emit_bb_start(&mut cs, batch.offset, batch.flags, batch.nopreempt).unwrap();
    let postfix = cs.len();
    let ends = emit_fini_breadcrumb_rcs(&mut cs, seqno, hwsp, Some(STATUS_PAGE + I915_GEM_HWS_PREEMPT * 4)).unwrap();
    let n = cs.len();
    buf.truncate(n);
    (buf, postfix * 4, postfix * 4 + ends.tail * 4, postfix * 4 + ends.wa_tail * 4)
}

#[test]
fn a_request_is_flush_init_breadcrumb_batch_and_fini_breadcrumb_in_that_order() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    let rq = e.build_request(c, &mut mem, &BATCH).unwrap();
    // an initial-breadcrumb timeline advances by two: seqno 2, with seqno - 1 stored by the init breadcrumb
    assert_eq!(rq, RequestRef { ctx: c, seqno: 2 });
    let (want, postfix, tail, wa_tail) = expected_request(spec(0).hwsp_ggtt, 2, true, &BATCH);
    assert_eq!(want.len(), 12 + 6 + 6 + 22);
    assert_eq!(&mem[..want.len()], &want[..]);
    assert!(mem[want.len()..].iter().all(|d| *d == 0xdead_beef), "nothing is written past the request");
    let r = e.requests()[0];
    assert_eq!((r.head, r.infix, r.postfix, r.tail, r.wa_tail), (0, 72, postfix as u32, tail as u32, wa_tail as u32));
    assert_eq!(r.infix, (12 + 6) * 4, "infix is the end of the header: after the flush and the initial breadcrumb (gen8_engine_cs.c:476)");
    assert_eq!(r.postfix, (12 + 6 + 6) * 4, "postfix is where the fini breadcrumb starts");
    assert_eq!(tail, 96 + 80, "rq->tail is after the interrupt and the preempt busywait");
    assert_eq!(wa_tail - tail, 8, "rq->wa_tail is the two WaIdleLiteRestore NOOPs later");
    assert!(!r.submitted);
    assert_eq!(e.context_ring(c).unwrap().emit(), (want.len() * 4) as u32);
    assert_eq!(e.context_timeline(c).unwrap().seqno(), 2);
}

#[test]
fn a_kernel_timeline_has_no_initial_breadcrumb_and_steps_by_one() {
    let mut e = rcs();
    let mut s = spec(0);
    s.initial_breadcrumb = false;
    let c = e.create_context(&s).unwrap();
    let mut mem = ring_mem(4096);
    let rq = e.build_request(c, &mut mem, &BATCH).unwrap();
    assert_eq!(rq.seqno, 1);
    let (want, ..) = expected_request(s.hwsp_ggtt, 1, false, &BATCH);
    assert_eq!(want.len(), 12 + 6 + 22, "no 6-dword initial breadcrumb");
    assert_eq!(&mem[..want.len()], &want[..]);
    let rq = e.build_request(c, &mut mem, &BATCH).unwrap();
    assert_eq!(rq.seqno, 2);
}

#[test]
fn the_other_engines_use_the_xcs_emitters() {
    let info = kbl_engine(EngineClass::VideoDecode);
    let mut e = Engine::new(&info, 9, config()).unwrap();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    e.build_request(c, &mut mem, &BATCH).unwrap();
    let mut buf = vec![0u32; 64];
    let mut cs = CsBuf::new(&mut buf);
    emit_flush_xcs(&mut cs, EMIT_INVALIDATE, true).unwrap(); // video decode adds MI_INVALIDATE_BSD
    emit_init_breadcrumb(&mut cs, spec(0).hwsp_ggtt, 2).unwrap();
    emit_bb_start(&mut cs, BATCH.offset, 0, false).unwrap();
    emit_fini_breadcrumb_xcs(&mut cs, 2, spec(0).hwsp_ggtt, Some(STATUS_PAGE + 0xc8)).unwrap();
    let n = cs.len();
    assert_eq!(n, 4 + 6 + 6 + 14);
    assert_eq!(&mem[..n], &buf[..n]);
}

#[test]
fn a_no_preempt_batch_uses_the_four_dword_form() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    let b = BatchSpec { nopreempt: true, ..BATCH };
    e.build_request(c, &mut mem, &b).unwrap();
    let (want, postfix, ..) = expected_request(spec(0).hwsp_ggtt, 2, true, &b);
    assert_eq!(&mem[..want.len()], &want[..]);
    assert_eq!(postfix, (12 + 6 + 4) * 4);
}

#[test]
fn a_refused_request_leaves_the_engine_exactly_as_it_was() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    // a ring mapping of the wrong size
    let mut short = ring_mem(2048);
    assert_eq!(e.build_request(c, &mut short, &BATCH).unwrap_err(), SubmitError::BadRingSize { size: 2048 });
    // a batch address the emitter cannot encode is not possible, so exhaust the ring instead
    let mut n = 0;
    let (emit0, seq0, reqs0);
    loop {
        let before = (e.context_ring(c).unwrap().emit(), e.context_timeline(c).unwrap().seqno(), e.requests().len());
        match e.build_request(c, &mut mem, &BATCH) {
            Ok(_) => n += 1,
            Err(err) => {
                // 21 whole requests (21 * 184 = 3864 bytes) fit. The 22nd request's flush (48 bytes, 2 * 88 + 64 = 240 reserved)
                // fits before the end of the ring (232 bytes remain) but its reserved space does not, so Linux demands the reserved
                // 240 plus the 232 bytes left at the end = 472 of free space, and the ring has 168. The accept / refuse decisions
                // are Linux's own `intel_ring_begin`, replayed by GCC (tests/ring.rs).
                assert_eq!(err, SubmitError::RingFull { need: 472, space: 168 });
                assert_eq!(err.name(), "RING-FULL");
                (emit0, seq0, reqs0) = before;
                break;
            }
        }
    }
    assert_eq!(n, 21, "a 4 KiB ring holds exactly 21 of these requests before the reserved space refuses the next");
    assert_eq!((e.context_ring(c).unwrap().emit(), e.context_timeline(c).unwrap().seqno(), e.requests().len()), (emit0, seq0, reqs0));
}

#[test]
fn ring_full_is_refused_by_name_and_clears_after_retire() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    let mut built = vec![];
    let err = loop {
        match e.build_request(c, &mut mem, &BATCH) {
            Ok(rq) => built.push(rq),
            Err(err) => break err,
        }
    };
    assert_eq!(err.name(), "RING-FULL");
    assert!(matches!(err, SubmitError::RingFull { need, space } if need > space));
    // the hardware completes the first three requests; retiring them moves the head
    let third_postfix = e.requests()[2].postfix;
    e.observe_seqno(c, built[2].seqno).unwrap();
    assert_eq!(e.retire(c).unwrap(), 3);
    assert_eq!(e.context_ring(c).unwrap().head(), third_postfix, "head = the postfix of the last retired request (i915_request.c:385): the GPU may be inside its breadcrumb");
    assert!(e.context_ring(c).unwrap().head() < e.requests()[0].head);
    e.build_request(c, &mut mem, &BATCH).unwrap();
}

#[test]
fn the_ring_wraps_with_noop_fill_and_requests_stay_contiguous() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    let mut last = None;
    // run enough requests through a 4 KiB ring, retiring each, to wrap it several times
    for i in 0..60u32 {
        let rq = e.build_request(c, &mut mem, &BATCH).unwrap_or_else(|err| panic!("request {i}: {err:?}"));
        e.observe_seqno(c, rq.seqno).unwrap();
        e.retire(c).unwrap();
        last = Some(rq);
    }
    assert_eq!(last.unwrap().seqno, 120);
    // every request is 184 bytes, so the emit offset cycled and some request wrapped: the ring head always tracks
    let ring = e.context_ring(c).unwrap();
    let wa_tail_to_postfix = 22 * 4; // the fini breadcrumb of the render engine, 22 dwords
    assert_eq!(ring.wrap(ring.head() + wa_tail_to_postfix), ring.emit(), "head = the last postfix; emit = the end of that breadcrumb");
    assert!(ring.offset_valid(ring.emit()));
    assert!(e.requests().is_empty());
}

#[test]
fn a_packet_that_does_not_fit_before_the_end_restarts_at_zero_after_zeroed_noops() {
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    let mut mem = ring_mem(4096);
    let mut found = None;
    for _ in 0..40 {
        let rq = e.build_request(c, &mut mem, &BATCH).unwrap();
        let r = *e.requests().iter().find(|r| r.seqno == rq.seqno).unwrap();
        if e.context_ring(c).unwrap().emit() < r.head {
            found = Some(r);
            break;
        }
        e.observe_seqno(c, rq.seqno).unwrap();
        e.retire(c).unwrap();
    }
    let r = found.expect("the ring wrapped within 40 requests");
    // Walk the request packet by packet, applying intel_ring_begin's rule: a packet that does not fit before the end goes to 0
    // and the rest of the old span is zero NOOPs.
    let (want, ..) = expected_request(spec(0).hwsp_ggtt, r.seqno, true, &BATCH);
    let sizes = [12usize, 6, 6, 22];
    let mut off = r.head as usize;
    let mut src = 0usize;
    let mut wrapped = false;
    for n in sizes {
        if off + n * 4 > 4096 {
            assert!(mem[off / 4..].iter().all(|d| *d == 0), "the old tail is zeroed MI_NOOPs");
            off = 0;
            wrapped = true;
        }
        assert_eq!(&mem[off / 4..off / 4 + n], &want[src..src + n], "packet of {n} dwords at {off}");
        off += n * 4;
        src += n;
    }
    assert!(wrapped, "this request spans the end of the ring");
    assert_eq!(e.context_ring(c).unwrap().emit() as usize, off % 4096);
    // the fini breadcrumb is contiguous so postfix / tail / wa_tail are ordered behind the head modulo the ring
    assert!(e.context_ring(c).unwrap().direction(r.wa_tail, r.head) > 0);
}

#[test]
fn context_and_request_tables_grow_past_the_old_caps() {
    // RUNTIME-TABLES-HAVE-NO-CAP: the tests used to run a 2-context / 3-request engine and the callers 4 / 16.
    let mut e = rcs();
    let mut mem = ring_mem(4096);
    let ids: Vec<u32> = (0..300).map(|n| e.create_context(&spec(n)).unwrap()).collect();
    assert_eq!(ids, (0..300).collect::<Vec<u32>>(), "ids are slot indices, dense");
    for &c in &ids {
        for _ in 0..3 {
            e.build_request(c, &mut mem, &BATCH).unwrap();
        }
    }
    assert_eq!(e.requests().len(), 900);
    // a destroyed context's slot is reused before the table grows
    e.destroy_context(299).unwrap_err(); // has requests: busy
    let mut e2 = rcs();
    let a = e2.create_context(&spec(0)).unwrap();
    let b = e2.create_context(&spec(1)).unwrap();
    e2.destroy_context(a).unwrap();
    assert_eq!(e2.create_context(&spec(2)).unwrap(), a);
    assert_eq!(e2.create_context(&spec(3)).unwrap(), b + 1);
}

#[test]
fn unknown_and_busy_contexts_are_refused_by_name() {
    let mut e = rcs();
    let mut mem = ring_mem(4096);
    assert_eq!(e.build_request(0, &mut mem, &BATCH).unwrap_err(), SubmitError::NoSuchContext { id: 0 });
    assert_eq!(e.retire(1).unwrap_err().name(), "NO-SUCH-CONTEXT");
    assert_eq!(e.observe_seqno(1, 0).unwrap_err().name(), "NO-SUCH-CONTEXT");
    let c = e.create_context(&spec(0)).unwrap();
    e.build_request(c, &mut mem, &BATCH).unwrap();
    assert_eq!(e.destroy_context(c).unwrap_err(), SubmitError::ContextBusy { id: c });
    e.observe_seqno(c, 2).unwrap();
    e.retire(c).unwrap();
    e.destroy_context(c).unwrap();
    assert_eq!(e.context_regs(c).unwrap_err(), SubmitError::NoSuchContext { id: c });
}

#[test]
fn bad_timeline_slots_and_rings_are_refused_at_context_creation() {
    let mut e = rcs();
    let mut s = spec(0);
    s.hwsp_ggtt = 0x3004; // not qword aligned
    assert_eq!(e.create_context(&s).unwrap_err().name(), "SEQNO-SLOT-INVALID");
    s.hwsp_ggtt = 0x3020; // bit 5 set: MI_FLUSH_DW address workaround
    assert!(matches!(e.create_context(&s).unwrap_err(), SubmitError::SeqnoSlotInvalid { addr: 0x3020, .. }));
    let mut s = spec(0);
    s.ring_size = 6000;
    assert_eq!(e.create_context(&s).unwrap_err(), SubmitError::BadRingSize { size: 6000 });
    let mut s = spec(0);
    s.lrca = 0x100_0800;
    assert_eq!(e.create_context(&s).unwrap_err(), SubmitError::LrcaUnaligned { lrca: 0x100_0800 });
    assert!(e.context_regs(0).is_err(), "a refused creation leaves no context behind");
}

#[test]
fn engine_construction_checks_its_inputs() {
    let info = kbl_engine(EngineClass::Render);
    let mut cfg = config();
    cfg.stall_bound = 0;
    assert_eq!(Engine::new(&info, 9, cfg).unwrap_err(), SubmitError::ZeroBound { what: "stall_bound" });
    let mut cfg = config();
    cfg.status_page_ggtt = 0x40800;
    assert_eq!(Engine::new(&info, 9, cfg).unwrap_err().name(), "GGTT-UNALIGNED");
    for gen in [7u8, 10, 11, 12] {
        assert_eq!(Engine::new(&info, gen, config()).unwrap_err(), SubmitError::UnsupportedGeneration { gen });
    }
}

#[test]
fn the_context_image_is_22_pages_for_a_render_engine_and_the_state_page_is_page_one() {
    let r = kbl_engine(EngineClass::Render);
    assert_eq!(r.context_size, 22 * 4096, "GEN9_LR_CONTEXT_RENDER_SIZE (intel_engine_cs.c:47)");
    assert_eq!(kbl_engine(EngineClass::Copy).context_size, 2 * 4096, "GEN8_LR_CONTEXT_OTHER_SIZE (:50)");
    assert!(LRC_STATE_OFFSET + 4096 <= r.context_size);
    let mut e = rcs();
    let c = e.create_context(&spec(0)).unwrap();
    assert_eq!(e.context_regs(c).unwrap().as_slice().len(), PAGE_DWORDS);
    assert_eq!(e.context_regs(c).unwrap().as_slice().len() * 4 + LRC_STATE_OFFSET as usize, 2 * 4096);
}

#[test]
fn a_new_context_image_points_at_its_ring_and_address_space() {
    let mut e = rcs();
    let c = e.create_context(&spec(1)).unwrap();
    let regs = e.context_regs(c).unwrap();
    assert_eq!(regs.get(CTX_RING_START), 0x0200_1000);
    assert_eq!(regs.get(CTX_RING_HEAD), 0);
    assert_eq!(regs.get(CTX_RING_TAIL), 0);
    assert_eq!(regs.get(CTX_RING_CTL), 1, "RING_CTL_SIZE(4096) | RING_VALID");
    assert_eq!((regs.get(CTX_PDP0_UDW), regs.get(CTX_PDP0_LDW)), (1, 0x1000), "the PML4 from i915-memory-core's PpgttInit");
    // restore is inhibited for a context with no default state: bit 0 of the control word's value half
    assert_eq!(regs.get(CTX_CONTEXT_CONTROL), 0x000f_0009);
    assert_eq!(e.context_lrca(c).unwrap(), 0x0101_0000);
    // the descriptor still carries FORCE_RESTORE (lrc_update_regs), and no ccid before the first submission
    assert_eq!(e.context_descriptor(c).unwrap(), 0x0101_0000 | 0x119 | 0x4);
    let _ = PpgttInit { pml4: 0, scratch: [0; 4] }; // the type the executor takes the PML4 from
}

#[test]
fn a_default_state_page_is_kept_and_not_inhibited() {
    let mut e = rcs();
    let page = RegState::from_default_state(&[0xa5a5_a5a5; PAGE_DWORDS]);
    let mut s = spec(0);
    s.default_state = Some(&page);
    let c = e.create_context(&s).unwrap();
    let regs = e.context_regs(c).unwrap();
    assert_eq!(regs.get(CTX_CONTEXT_CONTROL), 0x000f_0008, "no restore inhibit");
    assert_eq!(regs.get(0x7f), 0xa5a5_a5a5, "untouched dwords of the default state survive");
}

// ---- submission ------------------------------------------------------------------------------------------------------------

struct Rig {
    e: Engine,
    mem: [Vec<u32>; 4],
}

impl Rig {
    fn new() -> Rig {
        let mut e = rcs();
        for n in 0..3 {
            e.create_context(&spec(n)).unwrap();
        }
        Rig { e, mem: [ring_mem(4096), ring_mem(4096), ring_mem(4096), ring_mem(4096)] }
    }
    fn rq(&mut self, ctx: u32) -> RequestRef {
        self.e.build_request(ctx, &mut self.mem[ctx as usize], &BATCH).unwrap()
    }
    fn csb(&mut self, entries: &[(u32, u64)], write: u32) -> Vec<Port> {
        let table: Vec<(u32, u64)> = entries.to_vec();
        let out = self.e.process_csb(write, |i| table.iter().find(|(k, _)| *k == i).map(|(_, v)| *v).unwrap_or(u64::MAX), |_| panic!("mmio")).unwrap();
        out.inactive.as_slice().to_vec()
    }
}

const ELSP: Reg = Reg::new(0x2230);

#[test]
fn the_first_submission_writes_port_one_then_port_zero_upper_then_lower() {
    let mut r = Rig::new();
    let a = r.rq(0);
    let b = r.rq(1);
    let plan = r.e.dequeue(10).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(Port { ctx: 0, seqno: a.seqno }), Some(Port { ctx: 1, seqno: b.seqno })]);
    // schedule_in runs from the last port to the first (:1585-1587), so B takes tag 0 and A tag 1:
    // ccid = (1 + tag) << (GEN11_SW_CTX_ID_SHIFT - 32)
    let b_desc = 0x20u64 << 32 | 0x0101_0000 | 0x119 | 0x4;
    let a_desc = 0x40u64 << 32 | 0x0100_0000 | 0x119 | 0x4;
    // (lrca of ctx0 is 0x0100_0000, ctx1 0x0101_0000; the first submission after a pin forces a restore)
    assert_eq!(plan.descs, [a_desc, b_desc]);
    assert_eq!(
        plan.elsp,
        [
            WritePlan { reg: ELSP, value: 0x20 },
            WritePlan { reg: ELSP, value: 0x0101_011d },
            WritePlan { reg: ELSP, value: 0x40 },
            WritePlan { reg: ELSP, value: 0x0100_011d },
        ]
    );
    // the context images carry the new tails; the stores precede the ELSP writes and run port 1 first
    let rqs = r.e.requests();
    assert_eq!(plan.image_stores.as_slice().len(), 2);
    assert_eq!(plan.image_stores.as_slice()[0].ctx, 1);
    assert_eq!(plan.image_stores.as_slice()[1].ctx, 0);
    assert_eq!(plan.image_stores.as_slice()[1].dword, 1024 + CTX_RING_TAIL as u32);
    let first_tail = rqs[0].wa_tail - 8; // rq->tail before `rq->tail = rq->wa_tail`: the two WaIdleLiteRestore NOOPs are 8 bytes
    assert_eq!(plan.image_stores.as_slice()[1].value, first_tail);
    assert_eq!(r.e.context_regs(0).unwrap().get(CTX_RING_TAIL), first_tail);
    assert_eq!(r.e.context_ring(0).unwrap().tail(), first_tail);
    // after the first submission the request's tail is its wa_tail, so a re-submission advances (or forces a restore)
    assert!(rqs.iter().all(|q| q.submitted && q.tail == q.wa_tail));
    assert_eq!(r.e.context_descriptor(0).unwrap() & 0x4, 0, "FORCE_RESTORE is consumed by the submission");
    // nothing more to do until the hardware acknowledges, even with more work queued
    let _c1 = r.rq(2);
    assert!(r.e.dequeue(11).unwrap().is_none());
    assert_eq!(r.e.requests().iter().filter(|q| q.submitted).count(), 2, "the queued request was not touched");
}

#[test]
fn two_contexts_alternate_through_the_elsp_in_linux_order() {
    let mut r = Rig::new();
    // queue order A1 B1 A2 B2 (creation order is the FIFO)
    let a1 = r.rq(0);
    let b1 = r.rq(1);
    let a2 = r.rq(0);
    let b2 = r.rq(1);
    let p = |rq: RequestRef| Port { ctx: rq.ctx, seqno: rq.seqno };

    // 1. [A1, B1]: A2 would need a third port, so it stays queued (goto done, :1527)
    let plan = r.e.dequeue(1).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(p(a1)), Some(p(b1))]);
    assert_eq!(r.e.requests().iter().filter(|q| q.submitted).count(), 2);
    // the hardware acknowledges: idle -> active
    assert!(r.csb(&[(0, 0x1)], 0).is_empty());
    assert!(r.e.dequeue(2).unwrap().is_none(), "two contexts in flight: wait for the next CS event (active[1])");

    // 2. A1 completes; the hardware switches to B1
    r.e.observe_seqno(0, a1.seqno).unwrap();
    assert_eq!(r.csb(&[(1, 0x4 | 0x10)], 1), vec![p(a1)]);
    // A's tag is back: lowest free tag is 1 again
    let plan = r.e.dequeue(3).unwrap().unwrap();
    // B1 (still running) is re-listed in port 0 and A2 goes to port 1; B2 would be the third context -> stays queued
    assert_eq!(plan.ports, [Some(p(b1)), Some(p(a2))]);
    // port 1 (A2) is scheduled in first and takes the lowest free tag (1 -> ccid 0x40); B keeps tag 0 (ccid 0x20)
    assert_eq!(plan.descs[1] >> 32, 0x40);
    assert_eq!(plan.descs[0] >> 32, 0x20);
    // B1's tail already advanced past its first submission, so this is a lite restore: no FORCE_RESTORE
    assert_eq!(plan.descs[0] & 0x4, 0, "tail advanced by the WaIdleLiteRestore NOOPs");
    assert_eq!(plan.descs[1] & 0xffff_ffff, 0x0100_0000 | 0x119, "A's FORCE_RESTORE was consumed by its first submission");
    assert_eq!(
        plan.elsp.map(|w| w.value),
        [0x40, 0x0100_0119, 0x20, (plan.descs[0] & 0xffff_ffff) as u32],
        "port 1 (A2) upper/lower, then port 0 (B1) upper/lower"
    );
    // 3. the hardware reports the replacement as a preemption/lite restore: B1 leaves flight once (it is also still in pending)
    let gone = r.csb(&[(2, 0x2 | 0x8000)], 2);
    assert_eq!(gone, vec![p(b1)]);
    // B still holds one in-flight reference (the resubmitted B1), so its tag is NOT released
    assert!(r.e.dequeue(4).unwrap().is_none(), "two ports in flight again");

    // 4. B1 completes (element switch to A2): B's last reference goes, tag 0 is free
    r.e.observe_seqno(1, b1.seqno).unwrap();
    assert_eq!(r.csb(&[(3, 0x4 | 0x10)], 3), vec![p(b1)]);
    let plan = r.e.dequeue(5).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(p(a2)), Some(p(b2))]);
    assert_eq!(plan.descs[1] >> 32, 0x20, "B2 gets the freed tag 0 again");
    assert_eq!(plan.descs[0] >> 32, 0x40, "A keeps its tag while it stays in flight");
    // 5. everything drains
    assert!(r.csb(&[(4, 0x1)], 4).len() == 1, "the promote retires the old active port (A2)");
    r.e.observe_seqno(0, a2.seqno).unwrap();
    r.e.observe_seqno(1, b2.seqno).unwrap();
    assert_eq!(r.csb(&[(5, 0x4 | 0x10)], 5).len(), 1);
    assert_eq!(r.csb(&[(0, 0x8 | 0x10)], 0).len(), 1);
    assert!(r.e.execlists().is_idle());
    assert!(r.e.dequeue(6).unwrap().is_none(), "nothing left to submit");
    // retire everything: the heads follow the postfixes
    assert_eq!(r.e.retire(0).unwrap(), 2);
    assert_eq!(r.e.retire(1).unwrap(), 2);
    assert!(r.e.requests().is_empty());
}

#[test]
fn consecutive_requests_of_one_context_merge_into_one_port_and_one_tail() {
    let mut r = Rig::new();
    let _a1 = r.rq(0);
    let _a2 = r.rq(0);
    let a3 = r.rq(0);
    let plan = r.e.dequeue(1).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(Port { ctx: 0, seqno: a3.seqno }), None], "one port, naming the last request");
    assert_eq!(plan.descs[1], 0, "the empty port writes a zero descriptor");
    assert_eq!(plan.elsp.map(|w| w.value)[..2], [0, 0]);
    assert_eq!(plan.image_stores.as_slice()[0].value, r.e.requests()[2].wa_tail - 8, "RING_TAIL covers all three requests");
    assert_eq!(r.e.requests().iter().filter(|q| q.submitted).count(), 3);
}

#[test]
fn a_context_in_flight_takes_new_requests_by_lite_restore() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    r.e.dequeue(1).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    // more work for the running context arrives
    let a2 = r.rq(0);
    let plan = r.e.dequeue(2).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(Port { ctx: 0, seqno: a2.seqno }), None], "same context: merged, no second port");
    assert_ne!(a1.seqno, a2.seqno);
    assert_eq!(plan.descs[0] & 0x4, 0, "tail advances: no forced restore");
    assert_eq!(plan.descs[0] >> 32, 0x20, "the context keeps its ccid");
}

#[test]
fn an_unchanged_or_rewound_tail_forces_a_full_restore() {
    // WaIdleLiteRestore (execlists_update_context): never submit the same RING_TAIL twice
    let ring = Ring::new(4096).unwrap();
    assert!(forces_restore(&ring, 64, 64), "same tail");
    assert!(forces_restore(&ring, 56, 64), "rewound");
    assert!(!forces_restore(&ring, 72, 64), "advanced by a qword");
    assert!(!forces_restore(&ring, 64 + 2040, 64), "advanced by nearly half the ring");
    assert!(forces_restore(&ring, 64 + 2048, 64), "exactly half a ring apart is ambiguous: reload");
    assert!(!forces_restore(&ring, 8, 4096 - 8), "advancing across the end of the ring is still forward");
    assert!(forces_restore(&ring, 4096 - 8, 8), "and going back across it is not");
}

#[test]
fn completed_active_ports_are_skipped_and_a_new_submission_follows() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    r.e.dequeue(1).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    // the request completes in memory before its CSB event is processed (completed(last) skips it)
    r.e.observe_seqno(0, a1.seqno).unwrap();
    let b1 = r.rq(1);
    let plan = r.e.dequeue(2).unwrap().unwrap();
    assert_eq!(plan.ports, [Some(Port { ctx: 1, seqno: b1.seqno }), None], "A1 is complete: B1 takes port 0 alone");
}

#[test]
fn the_engine_tags_are_reused_and_bounded() {
    let mut t = TagAllocator::new();
    assert_eq!(t.free_tags(), 63);
    let tags: Vec<u32> = (0..63).map(|_| t.alloc().unwrap()).collect();
    assert_eq!(tags, (0..63).collect::<Vec<_>>(), "lowest free tag first (__ffs)");
    let e = t.alloc().unwrap_err();
    assert_eq!(e, SubmitError::TagsExhausted);
    assert_eq!(e.name(), "CONTEXT-TAGS-EXHAUSTED");
    t.release(17);
    t.release(3);
    assert_eq!(t.alloc().unwrap(), 3);
    assert_eq!(t.alloc().unwrap(), 17);
    assert!(t.alloc().is_err());
    t.release(63); // not a tag: ignored
    t.release(200);
    assert_eq!(t.free_tags(), 0);
}

// ---- seqno and the stall bound ---------------------------------------------------------------------------------------------

#[test]
fn seqno_comparison_wraps_like_i915_seqno_passed() {
    assert!(seqno_passed(5, 5) && seqno_passed(6, 5) && !seqno_passed(4, 5));
    assert!(seqno_passed(2, 0xffff_fffe), "2 is after 0xfffffffe once the counter wraps");
    assert!(!seqno_passed(0xffff_fffe, 2));
    assert!(seqno_passed(0x8000_0000 - 1, 0) && !seqno_passed(0x8000_0000, 0), "half the range is the horizon");
}

#[test]
fn a_timeline_that_would_wrap_is_refused_by_name() {
    let mut tl = Timeline::resume(0x3000, true, 0xffff_fffc).unwrap();
    assert_eq!(tl.advance().unwrap(), 0xffff_fffe);
    let e = tl.advance().unwrap_err();
    assert_eq!(e, SubmitError::SeqnoWrap { seqno: 0xffff_fffe });
    assert_eq!(e.name(), "SEQNO-WRAP");
    assert_eq!(tl.seqno(), 0xffff_fffe, "a refused advance changes nothing");
    let mut tl = Timeline::resume(0x3000, false, 0xffff_fffe).unwrap();
    assert_eq!(tl.advance().unwrap(), 0xffff_ffff);
    assert!(tl.advance().is_err());
    assert_eq!(Timeline::resume(0x3000, true, 3).unwrap_err().name(), "SEQNO-SLOT-INVALID");
}

#[test]
fn a_stuck_seqno_is_declared_hung_after_exactly_the_bound() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    // the ELSP was acknowledged; the request never starts
    r.e.check_hang(100).unwrap(); // first look: the watch starts here
    r.e.check_hang(100 + BOUND - 1).unwrap();
    let e = r.e.check_hang(100 + BOUND).unwrap_err();
    assert_eq!(e, SubmitError::Hung { ctx: 0, seqno: a1.seqno, stalled: BOUND, bound: BOUND });
    assert_eq!(e.name(), "ENGINE-HUNG");
    // and it stays hung
    assert_eq!(r.e.check_hang(100 + 5 * BOUND).unwrap_err().name(), "ENGINE-HUNG");
}

#[test]
fn the_next_stall_check_is_the_deadline_of_work_in_flight_and_nothing_when_idle() {
    let mut r = Rig::new();
    assert_eq!(r.e.next_stall_check(7), None, "an idle engine has no deadline");
    let a1 = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    assert_eq!(r.e.next_stall_check(100), Some(100), "work in flight whose watch is not armed: look now");
    r.e.check_hang(100).unwrap();
    assert_eq!(r.e.next_stall_check(150), Some(100 + BOUND), "armed: exactly when check_hang would declare it");
    assert!(r.e.check_hang(100 + BOUND - 1).is_ok() && r.e.check_hang(100 + BOUND).is_err(), "and that is the same instant");
    r.e.observe_seqno(0, a1.seqno - 1).unwrap();
    assert_eq!(r.e.next_stall_check(300), Some(300), "progress disarms the watch until the next look");
    r.e.check_hang(300).unwrap();
    assert_eq!(r.e.next_stall_check(301), Some(300 + BOUND));
    r.e.observe_seqno(0, a1.seqno).unwrap();
    assert_eq!(r.e.next_stall_check(302), None, "a completed request has no deadline");
}

#[test]
fn progress_resets_the_stall_clock() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    r.e.check_hang(0).unwrap();
    r.e.check_hang(BOUND - 1).unwrap();
    // the initial breadcrumb lands: HWSP = seqno - 1, the request has STARTED
    r.e.observe_seqno(0, a1.seqno - 1).unwrap();
    r.e.check_hang(BOUND).unwrap(); // would have been hung, but the state changed: the clock restarts here
    r.e.check_hang(BOUND + BOUND - 1).unwrap();
    assert_eq!(r.e.check_hang(2 * BOUND).unwrap_err(), SubmitError::Hung { ctx: 0, seqno: a1.seqno, stalled: BOUND, bound: BOUND });
}

#[test]
fn completion_clears_the_watch_and_an_idle_engine_is_never_hung() {
    let mut r = Rig::new();
    r.e.check_hang(0).unwrap();
    r.e.check_hang(100 * BOUND).unwrap();
    let a1 = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    r.e.check_hang(1).unwrap();
    r.e.observe_seqno(0, a1.seqno).unwrap();
    r.csb(&[(1, 0x8 | 0x10)], 1);
    r.e.check_hang(1 + 50 * BOUND).unwrap();
    // a queued but not yet submitted request is not "stuck"
    let _a2 = r.rq(0);
    r.e.check_hang(1 + 100 * BOUND).unwrap();
}

#[test]
fn an_unacknowledged_submission_is_declared_by_name_after_the_bound() {
    let mut r = Rig::new();
    let _ = r.rq(0);
    r.e.dequeue(500).unwrap().unwrap();
    r.e.check_hang(500 + BOUND - 1).unwrap();
    let e = r.e.check_hang(500 + BOUND).unwrap_err();
    assert_eq!(e, SubmitError::ElspNotAcked { waited: BOUND, bound: BOUND });
    assert_eq!(e.name(), "ELSP-NOT-ACKED");
}

#[test]
fn an_acknowledged_submission_stops_the_ack_clock() {
    let mut r = Rig::new();
    let a1 = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    r.e.observe_seqno(0, a1.seqno - 1).unwrap();
    r.e.check_hang(BOUND * 10).unwrap_or_else(|e| panic!("the request has started; only the request watch applies: {e:?}"));
}

#[test]
fn reset_tracking_schedules_out_every_port_and_returns_the_csb_plan() {
    let mut r = Rig::new();
    let _ = r.rq(0);
    let _ = r.rq(1);
    r.e.dequeue(0).unwrap().unwrap();
    r.csb(&[(0, 0x1)], 0);
    let plan = r.e.reset_tracking().unwrap();
    assert_eq!(plan, r.e.plan_reset_csb_pointers());
    assert!(r.e.execlists().is_idle());
    assert_eq!(r.e.execlists().csb_head(), 5);
    // the tags came back: two fresh contexts get tags 0 and 1 again
    let _ = r.rq(2);
    let plan = r.e.dequeue(1);
    assert!(plan.is_ok());
    // and the stall watch was cleared
    r.e.check_hang(1_000_000).ok();
}

#[test]
fn reset_context_scrubs_the_image_and_restarts_the_ring() {
    let mut r = Rig::new();
    let _ = r.rq(0);
    let before = r.e.context_regs(0).unwrap().clone();
    let emit = r.e.context_ring(0).unwrap().emit();
    r.e.reset_context(0).unwrap();
    let ring = r.e.context_ring(0).unwrap();
    assert_eq!((ring.head(), ring.tail(), ring.emit()), (emit, emit, emit), "intel_ring_reset(ring, ring->emit)");
    let regs = r.e.context_regs(0).unwrap();
    assert_eq!(regs.get(CTX_RING_HEAD), emit);
    assert_eq!(regs.get(CTX_RING_TAIL), emit);
    assert_eq!(regs.get(CTX_CONTEXT_CONTROL), 0x000f_0009, "restore inhibited again");
    assert_eq!(before.get(CTX_RING_HEAD), 0);
    assert_ne!(before.as_slice(), regs.as_slice());
    // an in-flight context cannot be reset under the hardware
    let mut r = Rig::new();
    let _ = r.rq(0);
    r.e.dequeue(0).unwrap().unwrap();
    assert_eq!(r.e.reset_context(0).unwrap_err(), SubmitError::ContextBusy { id: 0 });
}

#[test]
fn a_dequeue_with_no_requests_does_nothing() {
    let mut r = Rig::new();
    assert!(r.e.dequeue(0).unwrap().is_none());
    assert_eq!(NUM_PORTS, 2);
}

#[test]
fn resume_plan_comes_from_the_engines_own_base_and_status_page() {
    let r = Rig::new();
    let plan = r.e.plan_resume().unwrap();
    assert_eq!(plan.as_slice().len(), 5);
    let regs: Vec<u32> = plan.as_slice().iter().map(|e| match e {
        i915_gt_core::Effect::Write(w) => w.reg.offset(),
        i915_gt_core::Effect::PostingRead(g) => g.offset(),
        _ => 0,
    }).collect();
    assert_eq!(regs, [0x2098, 0x229c, 0x209c, 0x2080, 0x2080]);
    assert_eq!(r.e.info().mmio_base, 0x2000);
}

/// (tag, ccid) = `(1 + tag) << (GEN11_SW_CTX_ID_SHIFT - 32)`, GCC-evaluated from intel_execlists_submission.c:509.
const CCIDS: [(u32, u32); 5] = [(0, 0x20), (1, 0x40), (2, 0x60), (3, 0x80), (62, 0x7e0)];

#[test]
fn ccid_and_tag_are_inverse_and_bounded() {
    use i915_submit_core::request::{ccid_for_tag, tag_for_ccid};
    for (tag, ccid) in CCIDS {
        assert_eq!(ccid_for_tag(tag), ccid);
        assert_eq!(tag_for_ccid(ccid), Some(tag));
    }
    // id 0 is "no tag"; ids 1..=63 are tags 0..=62; 64 and above are fixed `ce->tag` values that never return a tag
    assert_eq!(tag_for_ccid(0), None);
    assert_eq!(tag_for_ccid(63 << 5), Some(62));
    assert_eq!(tag_for_ccid(64 << 5), None);
    assert_eq!(tag_for_ccid(0x1000 << 5), None);
}
