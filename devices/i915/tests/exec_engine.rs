// SPDX-License-Identifier: GPL-2.0-only
//! INCREMENT P3 on a simulated GPU: an `MI_STORE_DWORD` batch submitted through submit-core's `Engine` produces the expected context
//! image, ring and ELSP writes in Linux's order; a stuck seqno is declared hung BY NAME; the reset planner's plan is executed
//! (RESET_CTL handshake, GDRST twice, cancel); the engine resumes and a post-reset request completes; two contexts are isolated
//! both ways; and the `[I915-EXEC ...]` marker appears only when all of that really happened.
//!
//! The simulated GPU does nothing by itself: the test PLAYS the hardware (writes the CSB entries and the timeline seqno into the status
//! page) exactly when it wants the engine to make progress, so a hang is simply the hardware not doing so.
mod exec_common;
use exec_common::*;
use i915_device::exec::dma::{ObjKind, PAGE};
use i915_device::exec::engine::{CtxObjects, EngineRig};
use i915_device::exec::memory::stale_access_denied;
use i915_device::exec::Executor;
use i915_gt_core::emit::FlushQuirks;
use i915_gt_core::engine::{engines_for, EngineClass, EngineInfo};
use i915_gt_core::reset::ResetScope;
use i915_memory_core::probe::GgttGeometry;
use i915_memory_core::{BindRequest, CacheLevel, Gtt, Mode, NoTablePages, Placement, PpgttInit};
use i915_submit_core::lrc_reg::{CTX_RING_TAIL, LRC_STATE_OFFSET};
use i915_submit_core::{BatchSpec, EngineConfig};

const STALL_BOUND: u64 = 1000;
const STORE_VALUE: u32 = 0xc0ff_ee00;
const BATCH_VA: u64 = 0x1000_0000_0000;
const RESULT_VA: [u64; 2] = [0x2000_0000, 0x3000_0000];
const MI_STORE_DWORD_IMM_GEN4: u32 = (0x20 << 23) | 2; // intel_gpu_commands.h:136 (MI_INSTR(0x20, 2))
const MI_BATCH_BUFFER_END: u32 = 0x0a << 23; // intel_gpu_commands.h:62

fn rcs() -> EngineInfo {
    *engines_for(&kbl().info).unwrap().as_slice().iter().find(|e| e.class == EngineClass::Render).unwrap()
}

fn all_engines() -> Vec<EngineInfo> {
    engines_for(&kbl().info).unwrap().as_slice().to_vec()
}

fn req(pages: &[u64], place: Placement) -> BindRequest<'_> {
    BindRequest { pages, size: pages.len() as u64 * 4096, alignment: 0, guard: 0, cache: CacheLevel::Llc, flags: 0, place }
}

fn config(status_page_ggtt: u32) -> EngineConfig {
    EngineConfig { status_page_ggtt, quirks: FlushQuirks { vf_flush_wa: true, dc_flush_wa_kbl: false }, wa_ctx: None, stall_bound: STALL_BOUND }
}

/// The caller's monotonic clock: advances by `step` on every read.
struct Tick {
    t: u64,
    step: u64,
}
impl Tick {
    fn new(step: u64) -> Tick {
        Tick { t: 0, step }
    }
    fn now(&mut self) -> u64 {
        self.t += self.step;
        self.t
    }
}

struct Ctx {
    id: u32,
    objs: CtxObjects,
    gtt: Gtt<16, 32>,
    timeline_ggtt: u32,
    result_va: u64,
}

struct World {
    chip: Chip,
    mem: Mem,
    ex: Executor<Chip>,
    gm: Gm,
    log: Lines,
    ggtt: Gtt<16, 4>,
    rig: EngineRig<2, 8>,
    hwsp_ggtt: u32,
    ctxs: Vec<Ctx>,
}

impl World {
    fn hwsp_cpu(&self) -> u64 {
        self.rig_hwsp().cpu_mapping
    }
    fn rig_hwsp(&self) -> i915_device::exec::dma::Grant {
        *self.gm.alloc.grants().find(|g| g.kind == ObjKind::Hwsp).unwrap()
    }
    fn dword_at_cpu(&self, g: &i915_device::exec::dma::Grant, byte: u64) -> u32 {
        self.mem.get(g.cpu_mapping + byte)
    }
}

fn bind_ggtt(ex: &mut Executor<Chip>, gm: &mut Gm, log: &mut Lines, ggtt: &mut Gtt<16, 4>, g: &i915_device::exec::dma::Grant) -> u32 {
    let pages: Vec<u64> = (0..g.pages()).map(|i| g.page(i)).collect();
    let plan = ggtt.plan_bind(&mut NoTablePages, &req(&pages, Placement::best())).unwrap();
    ex.run_bind_step(log, gm, &plan, true, &mut |_| true).unwrap();
    assert_eq!(ex.verify_ggtt(&plan), None);
    plan.offset() as u32
}

/// A GGTT with the status page bound, an engine brought up, and `n` contexts (each with its own PPGTT holding a batch that stores
/// `STORE_VALUE + n` into that context's result page).
fn world(contexts: usize) -> World {
    let (chip, mem, mut ex, mut gm) = built();
    let mut log = Lines::default();
    let geom = GgttGeometry::probe(1 << 6, 16 << 20, 256 << 20).unwrap();
    ex.set_ggtt_window(i915_device::exec::allow::GgttWindow::for_table(geom.pte_base, geom.entries).unwrap()).unwrap();
    let scratch = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
    let (mut ggtt, create) = Gtt::<16, 4>::new_ggtt(&kbl(), &geom, scratch.device_addr).unwrap();
    ex.run_bind_step(&mut log, &mut gm, &create, true, &mut |_| true).unwrap();
    let hwsp = gm.alloc.alloc_hwsp().unwrap();
    let hwsp_ggtt = bind_ggtt(&mut ex, &mut gm, &mut log, &mut ggtt, &hwsp);
    assert_eq!(hwsp_ggtt % 4096, 0);
    let mut rig = EngineRig::<2, 8>::new(&rcs(), 9, config(hwsp_ggtt), hwsp, hwsp_ggtt).unwrap();
    rig.bring_up(&mut ex, &mut gm, &mut log).unwrap();
    let mut ctxs = vec![];
    for n in 0..contexts {
        let image = gm.alloc.alloc_context_image(rcs().context_size).unwrap();
        let ring = gm.alloc.alloc_ring(4096).unwrap();
        let image_ggtt = bind_ggtt(&mut ex, &mut gm, &mut log, &mut ggtt, &image);
        let ring_ggtt = bind_ggtt(&mut ex, &mut gm, &mut log, &mut ggtt, &ring);
        // this context's address space
        let pml4 = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let sc = [(); 4].map(|_| gm.alloc.alloc(ObjKind::Data, PAGE).unwrap());
        let (mut ppgtt, init) = Gtt::<16, 32>::new_ppgtt(&kbl(), &PpgttInit { pml4: pml4.device_addr, scratch: sc.map(|g| g.device_addr) }).unwrap();
        ex.run_bind_step(&mut log, &mut gm, &init, false, &mut |_| true).unwrap();
        let batch = gm.alloc.alloc(ObjKind::Batch, PAGE).unwrap();
        let result = gm.alloc.alloc(ObjKind::Data, PAGE).unwrap();
        let bpages = [batch.device_addr];
        let bp = ppgtt.plan_bind(&mut gm, &req(&bpages, Placement::with_mode(Mode::Fixed(BATCH_VA)))).unwrap();
        ex.run_bind_step(&mut log, &mut gm, &bp, false, &mut |_| true).unwrap();
        let rpages = [result.device_addr];
        let rp = ppgtt.plan_bind(&mut gm, &req(&rpages, Placement::with_mode(Mode::Fixed(RESULT_VA[n])))).unwrap();
        ex.run_bind_step(&mut log, &mut gm, &rp, false, &mut |_| true).unwrap();
        // MI_STORE_DWORD_IMM (gen4 form: address low, address high, value) then MI_BATCH_BUFFER_END
        let va = RESULT_VA[n];
        for (i, d) in [MI_STORE_DWORD_IMM_GEN4, va as u32, (va >> 32) as u32, STORE_VALUE + n as u32, MI_BATCH_BUFFER_END].iter().enumerate() {
            gm.store32(batch.device_addr + 4 * i as u64, *d).unwrap();
        }
        let timeline_ggtt = hwsp_ggtt + 0x100 + 0x40 * n as u32;
        let id = rig.add_context(&mut gm, CtxObjects { image, image_ggtt, ring, ring_ggtt }, pml4.device_addr, timeline_ggtt, 0, true).unwrap();
        ctxs.push(Ctx { id, objs: CtxObjects { image, image_ggtt, ring, ring_ggtt }, gtt: ppgtt, timeline_ggtt, result_va: va });
    }
    World { chip, mem, ex, gm, log, ggtt, rig, hwsp_ggtt, ctxs }
}

trait Hw {
    fn play_gpu(&self, w_hwsp_cpu: u64, tl_cpu: u64, first_csb: u32, seqno: u32, complete: bool);
}
impl Hw for Mem {
    /// The hardware picks the submission up: entry `first_csb` promotes (idle -> active), and with `complete` entry `first_csb + 1`
    /// retires the port and the timeline reaches `seqno`.
    fn play_gpu(&self, hwsp: u64, tl_cpu: u64, first: u32, seqno: u32, complete: bool) {
        let entry = |i: u32| hwsp + (0x10 + 2 * (i % 6)) as u64 * 4; // I915_HWS_CSB_BUF0_INDEX + 2 * idx
        self.put(entry(first), 1); // GEN8_CTX_STATUS_IDLE_ACTIVE, intel_execlists_submission.c:150
        self.put(entry(first) + 4, 0);
        let mut last = first;
        if complete {
            self.put(entry(first + 1), 1 << 3); // GEN8_CTX_STATUS_ACTIVE_IDLE, :153
            self.put(entry(first + 1) + 4, 0);
            self.put(tl_cpu, seqno);
            last = first + 1;
        }
        self.put(hwsp + 0x1f * 4, last % 6); // I915_HWS_CSB_WRITE_INDEX
    }
}

fn batch() -> BatchSpec {
    BatchSpec { offset: BATCH_VA, flags: 0, nopreempt: false }
}

fn tl_cpu(w: &World, c: &Ctx) -> u64 {
    w.hwsp_cpu() + (c.timeline_ggtt - w.hwsp_ggtt) as u64
}

// ---- bring-up -----------------------------------------------------------------------------------------------------------------

#[cfg(feature = "p3-exec")]
mod p3 {
    use super::*;

    #[test]
    fn bring_up_resets_the_csb_pointers_then_enables_execlists_in_linux_order() {
        let w = world(0);
        let h = w.hwsp_ggtt;
        assert_eq!(
            w.chip.all_writes().into_iter().filter(|(r, _)| *r != 0x101008).collect::<Vec<_>>(),
            [
                (0x23a0, 0xffff_0505), // RING_CONTEXT_STATUS_PTR: both pointers at csb_size - 1 = 5, mask 0xffff (reset_csb_pointers, :2804-2844)
                (0x23a0, 0xffff_0505), // "once more for luck and our trusty paranoia"
                (0x2098, 0xffff_ffff), // RING_HWSTAM
                (0x229c, 0x8000_8000), // RING_MODE: _MASKED_BIT_ENABLE(GFX_RUN_LIST_ENABLE = BIT(15))
                (0x209c, 0x0100_0000), // RING_MI_MODE: _MASKED_BIT_DISABLE(STOP_RING = BIT(8))
                (0x2080, h),           // RING_HWS_PGA = the status page's GGTT offset
            ]
        );
        let hwsp = w.rig_hwsp();
        assert_eq!(w.dword_at_cpu(&hwsp, 0x1f * 4), 5, "CSB write pointer dword = csb_size - 1");
        for i in 0..12 {
            assert_eq!(w.dword_at_cpu(&hwsp, (0x10 + i) * 4), u32::MAX, "CSB entries poisoned with -1");
        }
        assert!(w.log.has("[I915-STEP begun id=0x3001 name=ENGINE-ENABLE seq="));
        assert!(w.log.has("[I915-STEP done id=0x3001 name=ENGINE-ENABLE seq="));
        assert_eq!(w.ex.stats().refused, 0);
    }

    #[test]
    fn an_mi_store_dword_batch_produces_the_expected_image_ring_and_elsp_writes() {
        let mut w = world(1);
        let c = &w.ctxs[0];
        let (id, image, ring, tl_ggtt) = (c.id, c.objs.image, c.objs.ring, c.timeline_ggtt);
        let tail_cpu = image.cpu_mapping + (LRC_STATE_OFFSET + 4 * CTX_RING_TAIL as u32) as u64;
        // the image tail MUST be in memory before the first ELSP write (execlists_submit_ports: wmb() before writel)
        let m = w.mem.clone();
        w.chip.0.borrow_mut().on_write = Some(Box::new(move |_, reg, _| {
            if reg == 0x2230 {
                assert_ne!(m.get(tail_cpu), 0, "the ELSP was written before the image tail was stored");
            }
        }));
        let before = w.chip.all_writes().len();
        let (rq, sent) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, id, &batch(), 0).unwrap();
        assert!(sent);
        assert_eq!(rq.seqno, 2, "a timeline with an initial breadcrumb advances by two");

        // ELSP: port 1 (empty) upper+lower, then port 0 upper+lower (execlists_submit_ports, :905-939)
        let elsp: Vec<u32> = w.chip.all_writes()[before..].iter().filter(|(r, _)| *r == 0x2230).map(|(_, v)| *v).collect();
        let desc = w.rig.engine.context_descriptor(id).unwrap();
        const CTX_DESC_FORCE_RESTORE: u32 = 1 << 2; // intel_lrc.h: BIT_ULL(2), set on the first submission of a fresh context and consumed by it
        assert_eq!(elsp, [0, 0, (desc >> 32) as u32, desc as u32 | CTX_DESC_FORCE_RESTORE]);
        assert_eq!(elsp[3] & !0xfff, c.objs.image_ggtt, "the descriptor's LRCA is the context image's GGTT offset");
        assert_eq!(elsp[3] & 0x1, 1, "GEN8_CTX_VALID");

        // context image: tail published, regs copied into page 1
        let ring_tail = w.rig.engine.context_ring(id).unwrap().tail();
        assert_eq!(w.mem.get(tail_cpu), ring_tail);
        assert_ne!(ring_tail, 0);
        let regs = w.rig.engine.context_regs(id).unwrap();
        assert_eq!(w.mem.get(image.cpu_mapping + LRC_STATE_OFFSET as u64 + 4 * (CTX_RING_TAIL as u64 - 1)), regs.get(CTX_RING_TAIL - 1), "register-state page mirrored");

        // ring: ... MI_BATCH_BUFFER_START(batch VA) ... the init breadcrumb (timeline slot, seqno-1) and the fini breadcrumb (seqno)
        let rd = |i: u64| w.mem.get(ring.cpu_mapping + 4 * i);
        let dwords: Vec<u32> = (0..64).map(rd).collect();
        let p = dwords.windows(3).position(|x| x[1] == BATCH_VA as u32 && x[2] == (BATCH_VA >> 32) as u32).expect("MI_BATCH_BUFFER_START with the batch address");
        assert_eq!(dwords[p], 0x1880_0001 | 1 << 8, "MI_BATCH_BUFFER_START_GEN8 (MI_INSTR(0x31, 1)) | non-secure (gen8_emit_bb_start)");
        assert!(dwords.contains(&tl_ggtt), "the timeline's HWSP slot is addressed by the breadcrumbs");
        assert!(dwords.contains(&(rq.seqno - 1)) && dwords.contains(&rq.seqno), "init breadcrumb stores seqno-1, fini stores seqno");
        assert!(w.mem.flushes().iter().any(|(c, b)| *c == ring.cpu_mapping && *b == 4096), "the ring was flushed to the device");
        assert!(w.log.has("[I915-STEP begun id=0x3003 name=ELSP-SUBMIT seq="));
        assert_eq!(w.ex.stats().refused, 0);
    }

    #[test]
    fn the_hardware_completing_retires_the_request_and_the_marker_facts_are_recorded() {
        let mut w = world(1);
        let (id, tl) = (w.ctxs[0].id, tl_cpu(&w, &w.ctxs[0]));
        let (rq, _) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, id, &batch(), 0).unwrap();
        w.mem.play_gpu(w.hwsp_cpu(), tl, 0, rq.seqno, true);
        let mut clock = Tick::new(1);
        let rep = w.rig.wait_idle(&mut w.ex, &mut w.gm, &mut w.log, || clock.now()).unwrap();
        assert_eq!((rep.csb_events, rep.csb_completions, rep.retired), (2, 1, 1));
        assert!(w.rig.proof.csb_complete);
        assert_eq!(w.rig.proof.seqno, 2);
        assert!(w.rig.engine.requests().is_empty());
        let hwsp = w.rig_hwsp();
        assert_eq!(w.dword_at_cpu(&hwsp, 0x10 * 4), u32::MAX, "consumed CSB entries are poisoned again");
        assert_eq!(w.dword_at_cpu(&hwsp, 0x11 * 4), u32::MAX);
    }

    #[test]
    fn two_contexts_alternate_through_the_elsp_and_are_isolated_both_ways() {
        let mut w = world(2);
        let (a, b) = (w.ctxs[0].id, w.ctxs[1].id);
        let (tla, tlb) = (tl_cpu(&w, &w.ctxs[0]), tl_cpu(&w, &w.ctxs[1]));
        let (ra, sent_a) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, a, &batch(), 0).unwrap();
        let (rb, sent_b) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, b, &batch(), 0).unwrap();
        assert!(sent_a && !sent_b, "B waits: a previous submission is unacknowledged (__execlists_submission_tasklet)");
        w.mem.play_gpu(w.hwsp_cpu(), tla, 0, ra.seqno, true);
        let hw = w.hwsp_cpu();
        // service retires A and, in the same pass, submits B
        let rep = w.rig.service(&mut w.ex, &mut w.gm, &mut w.log, 1).unwrap();
        assert_eq!((rep.retired, rep.submitted), (1, 1));
        let elsp: Vec<u32> = w.chip.writes_to(0x2230);
        assert_eq!(elsp.len(), 8, "two submissions of four writes");
        let lrca_b = w.ctxs[1].objs.image_ggtt;
        assert_eq!(elsp[7] & !0xfff, lrca_b, "the second ELSP names B's context image");
        assert_ne!(elsp[3] & !0xfff, elsp[7] & !0xfff, "the two contexts have distinct images");
        w.mem.play_gpu(hw, tlb, 2, rb.seqno, true);
        let mut clock = Tick::new(1);
        w.rig.wait_idle(&mut w.ex, &mut w.gm, &mut w.log, || clock.now()).unwrap();
        assert!(w.rig.engine.requests().is_empty());
        // isolation, both ways: A's address space cannot reach B's result page and vice versa
        let (a_denies_b, b_denies_a) = (stale_access_denied(&w.ctxs[0].gtt, w.ctxs[1].result_va, 4), stale_access_denied(&w.ctxs[1].gtt, w.ctxs[0].result_va, 4));
        assert!(a_denies_b && b_denies_a);
        assert!(!stale_access_denied(&w.ctxs[0].gtt, w.ctxs[0].result_va, 4), "its own page is reachable");
        assert!(!stale_access_denied(&w.ctxs[1].gtt, w.ctxs[1].result_va, 4));
        assert_ne!(w.ctxs[0].result_va, w.ctxs[1].result_va);
    }

    #[test]
    fn a_stuck_seqno_is_declared_hung_by_name_and_the_engine_reset_plan_is_executed() {
        let mut w = world(1);
        let (id, tl) = (w.ctxs[0].id, tl_cpu(&w, &w.ctxs[0]));
        let (rq, _) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, id, &batch(), 0).unwrap();
        // the hardware accepts the submission (promote) and then never advances the timeline
        w.mem.play_gpu(w.hwsp_cpu(), tl, 0, rq.seqno, false);
        let mut clock = Tick::new(100);
        let e = w.rig.wait_idle(&mut w.ex, &mut w.gm, &mut w.log, || clock.now()).unwrap_err();
        assert_eq!(e.name(), "ENGINE-HUNG", "{e:?}");
        assert!(!w.rig.engine.requests().is_empty(), "nothing completed");

        // recover: ENGINE-RESET
        let before = w.chip.all_writes().len();
        let out = w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::Engines, &[]).unwrap();
        assert_eq!((out.attempts, out.proceeded_past_not_ready), (1, false));
        let ws: Vec<(u32, u32)> = w.chip.all_writes()[before..].to_vec();
        assert_eq!(
            ws,
            [
                (0x20d0, 0x0001_0001), // RESET_CTL(RCS0) <- _MASKED_BIT_ENABLE(REQUEST_RESET): gen8_engine_reset_prepare, intel_reset.c:576-584
                (0x941c, 0x2),         // GEN6_GDRST <- GEN6_GRDOM_RENDER (bit 1), intel_reset.c:296 ...
                (0x941c, 0x2),         //  ... issued twice: `loops = GRAPHICS_VER_FULL < IP_VER(12, 70) ? 2 : 1` (:288)
                (0x20d0, 0x0001_0000), // RESET_CTL <- _MASKED_BIT_DISABLE(REQUEST_RESET): gen8_engine_reset_cancel, :601-606
                (0x23a0, 0xffff_0505), // reset_csb_pointers
                (0x23a0, 0xffff_0505),
                (0x2098, 0xffff_ffff), // enable_execlists again
                (0x229c, 0x8000_8000),
                (0x209c, 0x0100_0000),
                (0x2080, w.hwsp_ggtt),
            ]
        );
        assert_eq!(w.rig.epoch, 1);
        assert!(w.rig.engine.requests().is_empty(), "the lost request was cancelled (execlists_reset_cancel)");
        assert!(w.rig.engine.execlists().is_idle());
        assert!(w.log.has("[I915-STEP begun id=0x3004 name=ENGINE-RESET"));
        assert!(w.log.has("[I915-STEP done id=0x3004 name=ENGINE-RESET"));
        assert!(w.chip.0.borrow().relax_calls < 50, "the handshake polls yield and stop");

        // post-reset resubmission completes on the SAME context
        w.mem.put(tl, 0); // the status page is the hardware's again
        let (rq2, sent) = w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, id, &batch(), 10_000).unwrap();
        assert!(sent);
        assert_eq!(rq2.seqno, 4);
        w.mem.play_gpu(w.hwsp_cpu(), tl, 0, rq2.seqno, true);
        let mut clock = Tick::new(1);
        w.rig.wait_idle(&mut w.ex, &mut w.gm, &mut w.log, || clock.now()).unwrap();
        assert!(w.rig.proof.recovered);

        // the marker prints only once isolation is proven too
        let mut log = Lines::default();
        assert!(!w.rig.emit_marker(&mut log), "no isolation proof yet");
        assert_eq!(log.0, ["[I915-EXEC INCOMPLETE seqno=4 csb=complete isolation=UNPROVEN reset_epoch=1 recovery=pass]"]);
        w.rig.note_isolation(true, true);
        let mut log = Lines::default();
        assert!(w.rig.emit_marker(&mut log));
        assert_eq!(log.0, ["[I915-EXEC seqno=4 csb=complete isolation=bothways reset_epoch=1 recovery=pass]"]);
    }

    #[test]
    fn an_unacknowledged_elsp_is_declared_by_name_too() {
        let mut w = world(1);
        let id = w.ctxs[0].id;
        w.rig.submit_batch(&mut w.ex, &mut w.gm, &mut w.log, id, &batch(), 0).unwrap();
        let mut clock = Tick::new(100);
        let e = w.rig.wait_idle(&mut w.ex, &mut w.gm, &mut w.log, || clock.now()).unwrap_err();
        assert_eq!(e.name(), "ELSP-NOT-ACKED", "{e:?}");
    }

    #[test]
    fn a_full_reset_prepares_every_engine_and_writes_the_full_domain_twice() {
        let mut w = world(1);
        let before = w.chip.all_writes().len();
        let engines = all_engines();
        assert_eq!(engines.len(), 4);
        let out = w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::AllEngines, &engines).unwrap();
        assert_eq!(out.attempts, 1);
        let ws: Vec<(u32, u32)> = w.chip.all_writes()[before..].to_vec();
        let resets: Vec<(u32, u32)> = ws.iter().copied().filter(|(r, _)| *r & 0xfff == 0x0d0 || *r == 0x941c).collect();
        assert_eq!(
            resets,
            [
                (0x20d0, 0x0001_0001),
                (0x220d0, 0x0001_0001),
                (0x120d0, 0x0001_0001),
                (0x1a0d0, 0x0001_0001),
                (0x941c, 0x1), // GEN6_GRDOM_FULL (intel_gt_regs.h:664)
                (0x941c, 0x1),
                (0x20d0, 0x0001_0000),
                (0x220d0, 0x0001_0000),
                (0x120d0, 0x0001_0000),
                (0x1a0d0, 0x0001_0000),
            ]
        );
        assert_eq!(w.rig.epoch, 1);
        assert!(w.log.has("[I915-STEP done id=0x3005 name=FULL-RESET"));
    }

    #[test]
    fn a_reset_whose_engine_never_becomes_ready_stops_at_its_bound_and_cancels() {
        let mut w = world(1);
        w.chip.0.borrow_mut().ready_never = true;
        let before = w.chip.all_writes().len();
        let e = w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::Engines, &[]).unwrap_err();
        assert_eq!(e.name(), "RESET-READY-TIMEOUT");
        let ws: Vec<(u32, u32)> = w.chip.all_writes()[before..].to_vec();
        assert_eq!(ws, [(0x20d0, 0x0001_0001), (0x20d0, 0x0001_0000)], "request, then the withdrawal - and no GDRST on a refused handshake");
        assert!(w.log.has("[I915-STEP failed id=0x3004 name=ENGINE-RESET"));
        assert_eq!(w.rig.epoch, 0, "a reset that did not happen is not counted");
        assert!(w.chip.0.borrow().relax_calls < 200_000);
    }

    #[test]
    fn a_gdrst_that_never_acks_stops_at_its_bound() {
        let mut w = world(1);
        w.chip.0.borrow_mut().gdrst_stuck = true;
        let e = w.rig.recover(&mut w.ex, &mut w.gm, &mut w.log, ResetScope::Engines, &[]).unwrap_err();
        assert_eq!(e.name(), "GDRST-ACK-TIMEOUT");
        assert_eq!(w.chip.writes_to(0x941c).len(), 1, "one write, then the wait gave up (the planner does not write again on a timeout)");
    }

    #[test]
    fn the_exec_marker_needs_every_fact() {
        let mut w = world(1);
        let mut log = Lines::default();
        assert!(!w.rig.emit_marker(&mut log));
        assert!(log.0[0].starts_with("[I915-EXEC INCOMPLETE seqno=0 csb=none isolation=UNPROVEN reset_epoch=0 recovery=none"));
        w.rig.proof.seqno = 2;
        w.rig.proof.csb_complete = true;
        w.rig.note_isolation(true, false);
        w.rig.epoch = 1;
        w.rig.proof.recovered = true;
        let mut log = Lines::default();
        assert!(!w.rig.emit_marker(&mut log), "isolation in only one direction is not isolation");
        w.rig.note_isolation(true, true);
        let mut log = Lines::default();
        assert!(w.rig.emit_marker(&mut log));
        assert_eq!(log.0, ["[I915-EXEC seqno=2 csb=complete isolation=bothways reset_epoch=1 recovery=pass]"]);
        for mutate in [
            (|r: &mut EngineRig<2, 8>| r.proof.seqno = 0) as fn(&mut EngineRig<2, 8>),
            |r| r.proof.csb_complete = false,
            |r| r.epoch = 0,
            |r| r.proof.recovered = false,
        ] {
            mutate(&mut w.rig);
            let mut log = Lines::default();
            assert!(!w.rig.emit_marker(&mut log));
            w.rig.proof.seqno = 2;
            w.rig.proof.csb_complete = true;
            w.rig.epoch = 1;
            w.rig.proof.recovered = true;
        }
    }
}

// ---- with the feature OFF (the shipped default) every one of those writes is refused by name -------------------------------------

/// An executor with the SHIPPED grant and an engine over unbound objects (no GGTT is open, so nothing is bound): enough to reach every
/// register write the P3 paths make.
fn shipped_world() -> (Chip, Executor<Chip>, Gm, Lines, EngineRig<2, 8>, u32, u32) {
    static FW: [i915_common_core::Reg; 3] = i915_uncore_core::WRITE_ALLOWLIST;
    let chip = Chip::new();
    let (_mem, mut gm) = memory();
    let ex = Executor::with_grant(chip.clone(), &kbl(), &FW, None).unwrap();
    let hwsp = gm.alloc.alloc_hwsp().unwrap();
    let rig = EngineRig::<2, 8>::new(&rcs(), 9, config(0x1000), hwsp, 0x1000).unwrap();
    let image = gm.alloc.alloc_context_image(rcs().context_size).unwrap();
    let ring = gm.alloc.alloc_ring(4096).unwrap();
    (chip, ex, gm, Lines::default(), rig, image.device_addr as u32, ring.device_addr as u32)
}

#[test]
fn refused_without_the_feature_engine_bring_up_writes_nothing_and_names_the_first_register() {
    let (chip, mut ex, mut gm, mut log, mut rig, ..) = shipped_world();
    let e = rig.bring_up(&mut ex, &mut gm, &mut log).unwrap_err();
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert!(chip.all_writes().is_empty());
    assert_eq!(ex.stats().first_refusal.unwrap().offset, 0x23a0, "RING_CONTEXT_STATUS_PTR is the first register the plan writes");
    assert!(log.has("[I915-STEP failed id=0x3001 name=ENGINE-ENABLE seq=1 why=WRITE-NOT-ALLOWED]"));
}

#[test]
fn refused_without_the_feature_an_elsp_submission_never_reaches_the_chip() {
    let (chip, mut ex, mut gm, mut log, mut rig, ..) = shipped_world();
    let image = *gm.alloc.grants().find(|g| g.kind == ObjKind::ContextImage).unwrap();
    let ring = *gm.alloc.grants().find(|g| g.kind == ObjKind::Ring).unwrap();
    let id = rig.add_context(&mut gm, CtxObjects { image, image_ggtt: 0x10_0000, ring, ring_ggtt: 0x20_0000 }, 0x5000, 0x1000 + 0x100, 0, true).unwrap();
    let e = rig.submit_batch(&mut ex, &mut gm, &mut log, id, &batch(), 0).unwrap_err();
    assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    assert!(chip.writes_to(0x2230).is_empty(), "no ELSP write");
    assert!(chip.all_writes().is_empty());
    assert_eq!(ex.stats().first_refusal.unwrap().offset, 0x2230);
    assert!(log.has("[I915-STEP failed id=0x3003 name=ELSP-SUBMIT"));
}

#[test]
fn refused_without_the_feature_a_reset_does_not_touch_reset_ctl_or_gdrst() {
    let (chip, mut ex, mut gm, mut log, mut rig, ..) = shipped_world();
    for (scope, engines) in [(ResetScope::Engines, vec![]), (ResetScope::AllEngines, all_engines())] {
        let e = rig.recover(&mut ex, &mut gm, &mut log, scope, &engines).unwrap_err();
        assert_eq!(e.name(), "WRITE-NOT-ALLOWED");
    }
    assert!(chip.all_writes().is_empty());
    assert_eq!(ex.stats().refused, 2);
    assert_eq!(ex.stats().first_refusal.unwrap().offset, 0x20d0);
    assert_eq!(rig.epoch, 0);
    assert!(chip.writes_to(0x941c).is_empty());
}

#[cfg(not(feature = "p3-exec"))]
#[test]
fn the_default_build_executor_refuses_the_p3_writes_with_no_override() {
    let (chip, mut ex, mut gm, mut log, ..) = {
        let (chip, mem, ex, gm) = built();
        let _ = mem;
        (chip, ex, gm, Lines::default())
    };
    let hwsp = gm.alloc.alloc_hwsp().unwrap();
    let mut rig = EngineRig::<2, 8>::new(&rcs(), 9, config(0x1000), hwsp, 0x1000).unwrap();
    assert_eq!(ex.allow_list().len(), 3 + if cfg!(feature = "p2-memory") { 1 } else { 0 });
    assert_eq!(rig.bring_up(&mut ex, &mut gm, &mut log).unwrap_err().name(), "WRITE-NOT-ALLOWED");
    assert!(chip.all_writes().is_empty());
}
