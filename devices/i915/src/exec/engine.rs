// SPDX-License-Identifier: GPL-2.0-only
//! INCREMENT P3 - execution: an execlists engine ([`i915_submit_core::Engine`]) driven over the [`Executor`] and granted memory.
//!
//! The engine crate owns the MODEL (rings, timelines, context register state, ports, CSB tracker, hang watch); [`EngineRig`] owns
//! the MEMORY - the context image, the ring and the status page (HWSP) as DMA grants bound into the GGTT - and moves bytes between
//! the two. Nothing here decides; it executes the engine's plans in the engine's order:
//!
//! 1. `bring_up`: `reset_csb_pointers` (status-page stores + `RING_CONTEXT_STATUS_PTR`) then `enable_execlists`.
//! 2. `add_context`: the engine builds the register state; the rig copies it into page 1 of the context object.
//! 3. `submit_batch`: the ring is loaded, `build_request` fills it, only changed dwords are stored back, then `dequeue` yields the
//!    `SubmissionPlan` - image stores first (the engine's `wmb()`), then the four ELSP writes.
//! 4. `service`: CSB write pointer and entries are read from the status page (the MMIO mirror only for a poisoned entry), consumed
//!    entries are poisoned, the timeline seqno dword of each context is read, requests retire.
//! 5. `check_hang`: the engine's stall bound declares `ENGINE-HUNG` / `ELSP-NOT-ACKED` by name.
//! 6. `recover`: the reset planner runs (`RESET_CTL` handshake, `GEN6_GDRST` twice), the CSB tracker and registers are put back,
//!    lost requests are cancelled the way `execlists_reset_cancel` does (marked complete, retired), contexts are scrubbed
//!    (`lrc_reset`), execlists are re-enabled. Requests that were in flight are LOST (submit-core does not port replay).
//!
//! EVERY risky sequence is a `[I915-STEP begun ...]` step (plan section 5). The ELSP writes and the reset are the two the device
//! cannot take back.
use super::core::{Executor, DEFAULT_MAX_POLLS};
use super::dma::{CpuMem, GpuMem, Grant};
use super::journal::StepId;
use super::plans::HwspSink;
use super::ExecError;
use crate::log::{emit, Log};
use coralos_hal::{Dma, Mmio};
use i915_common_core::Reg;
use i915_gt_core::engine::EngineInfo;
use i915_gt_core::reset::{ResetPlanner, ResetScope};
use i915_gt_core::Effect;
use i915_submit_core::execlists::{csb_entry_dword, csb_entry_mmio, decode_csb, HwspStore, Step};
use i915_submit_core::lrc_reg::{GEN8_CSB_ENTRIES, I915_HWS_CSB_WRITE_INDEX, LRC_STATE_OFFSET};
use i915_submit_core::lrc_state::PAGE_DWORDS;
use i915_submit_core::{BatchSpec, ContextSpec, Engine, EngineConfig, RequestRef, SubmissionPlan};

/// The ring every context here uses: `SZ_4K`, `ce->ring_size`'s default (gt/intel_context.c:403).
pub const RING_BYTES: u32 = 4096;
const RING_DWORDS: usize = (RING_BYTES / 4) as usize;
/// Most `service` passes [`EngineRig::wait_idle`] makes before it gives up (a bound beside the engine's own stall bound).
pub const WAIT_IDLE_PASSES: u32 = 100_000;

/// A finished reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetOutcome {
    pub attempts: u32,
    pub proceeded_past_not_ready: bool,
}

/// The memory behind one context: the image object and the ring, with their GGTT offsets (what the engine was told).
#[derive(Clone, Copy, Debug)]
pub struct CtxObjects {
    pub image: Grant,
    pub image_ggtt: u32,
    pub ring: Grant,
    pub ring_ggtt: u32,
}

/// What `service` found.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ServiceReport {
    /// CSB entries consumed.
    pub csb_events: u32,
    /// Of those, ones that were not a promote (a port completed or went idle).
    pub csb_completions: u32,
    /// Requests retired.
    pub retired: u32,
    /// Submissions started by the dequeue that follows a completion.
    pub submitted: u32,
}

/// The facts `[I915-EXEC ...]` certifies (see [`EngineRig::emit_marker`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecProof {
    /// The highest seqno the hardware reported on any timeline.
    pub seqno: u32,
    pub csb_complete: bool,
    pub isolation_both_ways: bool,
    /// A request completed AFTER the most recent reset.
    pub recovered: bool,
}

/// One engine and the memory it runs in.
pub struct EngineRig<const C: usize, const R: usize> {
    pub engine: Engine<C, R>,
    hwsp: Grant,
    hwsp_ggtt: u32,
    objs: [Option<CtxObjects>; C],
    /// Resets executed (engine or full).
    pub epoch: u32,
    pub proof: ExecProof,
}

/// The status page as a [`HwspSink`].
struct Hwsp<'a, D: Dma, M: CpuMem> {
    gm: &'a mut GpuMem<D, M>,
    page: u64,
}

impl<D: Dma, M: CpuMem> HwspSink for Hwsp<'_, D, M> {
    fn store(&mut self, s: HwspStore) -> Result<(), ExecError> {
        match s {
            HwspStore::Dword { dword, value } => self.gm.store32(self.page + dword as u64 * 4, value)?,
            HwspStore::PoisonCsb { entries } => {
                for i in 0..entries * 2 {
                    self.gm.store32(self.page + (csb_entry_dword(0) + i) as u64 * 4, u32::MAX)?;
                }
            }
        }
        self.gm.flush(self.page, 4096)
    }
}

impl<const C: usize, const R: usize> EngineRig<C, R> {
    /// `hwsp` is the engine's status page object and `hwsp_ggtt` its GGTT offset; `cfg.status_page_ggtt` must be that offset.
    pub fn new(info: &EngineInfo, gen: u8, cfg: EngineConfig, hwsp: Grant, hwsp_ggtt: u32) -> Result<Self, ExecError> {
        if cfg.status_page_ggtt != hwsp_ggtt || hwsp.len < 4096 {
            return Err(ExecError::Layout { what: "status page object vs EngineConfig::status_page_ggtt" });
        }
        Ok(EngineRig { engine: Engine::new(info, gen, cfg)?, hwsp, hwsp_ggtt, objs: [None; C], epoch: 0, proof: ExecProof::default() })
    }

    fn hwsp_sink<'a, D: Dma, M: CpuMem>(&self, gm: &'a mut GpuMem<D, M>) -> Hwsp<'a, D, M> {
        Hwsp { gm, page: self.hwsp.device_addr }
    }

    /// The status-page dword holding a timeline's seqno: `hwsp_ggtt` is a GGTT address inside the status page's binding.
    fn seqno_dword(&self, tl_ggtt: u32) -> Result<u64, ExecError> {
        let off = tl_ggtt.checked_sub(self.hwsp_ggtt).filter(|o| o % 4 == 0 && *o < 4096).ok_or(ExecError::Layout { what: "timeline seqno slot outside the status page" })?;
        Ok(self.hwsp.device_addr + off as u64)
    }

    /// Step `ENGINE-ENABLE`: `reset_csb_pointers`, then `enable_execlists`.
    pub fn bring_up<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L) -> Result<(), ExecError> {
        let csb = self.engine.plan_reset_csb_pointers();
        let resume = self.engine.plan_resume()?;
        let mut regs = [Reg::new(0); 16];
        let mut n = 0;
        for s in csb.steps.as_slice() {
            if let Step::Mmio(e) = s {
                for r in Executor::<B>::gt_regs(core::slice::from_ref(e)) {
                    regs[n] = r;
                    n += 1;
                }
            }
        }
        for r in Executor::<B>::gt_regs(resume.as_slice()) {
            regs[n] = r;
            n += 1;
        }
        let d = ex.domains_for(regs[..n].iter().copied());
        let mut sink = self.hwsp_sink(gm);
        ex.step(log, StepId::ENGINE_ENABLE, "ENGINE-ENABLE", |ex, _| {
            ex.awake(d, |ex| {
                ex.run_csb_steps(csb.steps.as_slice(), &mut sink)?;
                ex.gt_effects_awake(resume.as_slice())
            })
        })
    }

    /// Create a context over `objs` (its image and ring already bound in the GGTT). The register state the engine builds is copied
    /// into page 1 of the image (`LRC_STATE_OFFSET`); page 0 (the per-process HWSP) stays zero.
    pub fn add_context<D: Dma, M: CpuMem>(&mut self, gm: &mut GpuMem<D, M>, objs: CtxObjects, pml4: u64, timeline_ggtt: u32, rpcs: u32, initial_breadcrumb: bool) -> Result<u32, ExecError> {
        if objs.ring.len as u32 != RING_BYTES || (objs.image.len as u32) < LRC_STATE_OFFSET + 4096 {
            return Err(ExecError::Layout { what: "context image / ring object size" });
        }
        self.seqno_dword(timeline_ggtt)?; // the slot must be in the status page
        let spec = ContextSpec { lrca: objs.image_ggtt, ring_size: RING_BYTES, ring_ggtt: objs.ring_ggtt, pml4, rpcs, hwsp_ggtt: timeline_ggtt, initial_breadcrumb, default_state: None };
        let id = self.engine.create_context(&spec)?;
        self.objs[id as usize] = Some(objs);
        self.store_state(gm, id)?;
        Ok(id)
    }

    fn objs(&self, ctx: u32) -> Result<CtxObjects, ExecError> {
        self.objs.get(ctx as usize).copied().flatten().ok_or(ExecError::Submit(i915_submit_core::SubmitError::NoSuchContext { id: ctx }))
    }

    /// Copy the engine's register-state page into the context object.
    fn store_state<D: Dma, M: CpuMem>(&self, gm: &mut GpuMem<D, M>, ctx: u32) -> Result<(), ExecError> {
        let o = self.objs(ctx)?;
        let base = o.image.device_addr + LRC_STATE_OFFSET as u64;
        let regs = self.engine.context_regs(ctx)?;
        for (i, v) in regs.as_slice().iter().enumerate().take(PAGE_DWORDS) {
            gm.store32(base + i as u64 * 4, *v)?;
        }
        gm.flush(base, 4096)
    }

    /// Build one request on `ctx` (the batch at `batch.offset`) in its ring and, if the engine can, submit it. Returns the request
    /// and whether the ELSP was written.
    pub fn submit_batch<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, ctx: u32, batch: &BatchSpec, now: u64) -> Result<(RequestRef, bool), ExecError> {
        let o = self.objs(ctx)?;
        let mut mem = [0u32; RING_DWORDS];
        for (i, d) in mem.iter_mut().enumerate() {
            *d = gm.load32(o.ring.device_addr + i as u64 * 4)?;
        }
        let before = mem;
        let rq = self.engine.build_request(ctx, &mut mem, batch)?;
        for (i, d) in mem.iter().enumerate() {
            if *d != before[i] {
                gm.store32(o.ring.device_addr + i as u64 * 4, *d)?;
            }
        }
        gm.flush(o.ring.device_addr, RING_BYTES)?;
        let sent = self.kick(ex, gm, log, now)?;
        Ok((rq, sent))
    }

    /// `execlists_dequeue`, and if it yields a submission, execute it. Returns whether the ELSP was written.
    pub fn kick<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, now: u64) -> Result<bool, ExecError> {
        match self.engine.dequeue(now)? {
            None => Ok(false),
            Some(plan) => {
                self.run_submission(ex, gm, log, &plan)?;
                Ok(true)
            }
        }
    }

    /// Step `ELSP-SUBMIT`: the image stores, then the four ELSP writes (port 1 upper/lower, port 0 upper/lower), with the render
    /// forcewake held.
    pub fn run_submission<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, plan: &SubmissionPlan) -> Result<(), ExecError> {
        let d = ex.domains_for(plan.elsp.iter().map(|w| w.reg));
        ex.step(log, StepId::ELSP_SUBMIT, "ELSP-SUBMIT", |ex, _| {
            for st in plan.image_stores.as_slice() {
                let o = self.objs(st.ctx)?;
                let at = o.image.device_addr + st.dword as u64 * 4;
                gm.store32(at, st.value)?;
                gm.flush(at, 4)?;
            }
            ex.awake(d, |ex| {
                let effects = plan.elsp.map(Effect::Write);
                ex.gt_effects_awake(&effects)
            })
        })
    }

    /// Consume the CSB, read each context's seqno, retire, and start the next submission if one is ready.
    pub fn service<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, now: u64) -> Result<ServiceReport, ExecError> {
        let mut rep = ServiceReport::default();
        let page = self.hwsp.device_addr;
        let base = self.engine.info().mmio_base;
        let csb_write = gm.load32(page + I915_HWS_CSB_WRITE_INDEX as u64 * 4)?;
        let mirror: [Reg; 12] = core::array::from_fn(|i| Reg::new(csb_entry_mmio(base, (i / 2) as u32).offset() + 4 * (i as u32 % 2)));
        let d = ex.domains_for(mirror);
        let engine = &mut self.engine;
        let out = ex.awake(d, |ex| {
            let hwsp = |idx: u32| -> u64 {
                let at = page + csb_entry_dword(idx) as u64 * 4;
                match (gm.load32(at), gm.load32(at + 4)) {
                    (Ok(lo), Ok(hi)) => (hi as u64) << 32 | lo as u64,
                    _ => u64::MAX,
                }
            };
            let mut rd = |idx: u32| -> u64 {
                let r = csb_entry_mmio(base, idx);
                match (ex.rd(r), ex.rd(Reg::new(r.offset() + 4))) {
                    (Ok(lo), Ok(hi)) => (hi as u64) << 32 | lo as u64,
                    _ => u64::MAX,
                }
            };
            Ok(engine.process_csb(csb_write, hwsp, &mut rd)?)
        })?;
        for ev in out.events.as_slice() {
            // csb_read's WRITE_ONCE(*csb, -1): a later reuse of the entry is recognisable
            gm.store32(page + csb_entry_dword(ev.index) as u64 * 4, u32::MAX)?;
            gm.store32(page + csb_entry_dword(ev.index) as u64 * 4 + 4, u32::MAX)?;
            rep.csb_events += 1;
            if !ev.promote {
                rep.csb_completions += 1;
                if decode_csb(ev.raw).complete || decode_csb(ev.raw).active_to_idle || decode_csb(ev.raw).element_switch {
                    self.proof.csb_complete = true;
                }
            }
        }
        if rep.csb_events != 0 {
            gm.flush(page, 4096)?;
        }
        for ctx in 0..C as u32 {
            if self.objs[ctx as usize].is_none() {
                continue;
            }
            let at = self.seqno_dword(self.engine.context_timeline(ctx)?.hwsp_ggtt)?;
            let hw = gm.load32(at)?;
            self.engine.observe_seqno(ctx, hw)?;
            self.proof.seqno = self.proof.seqno.max(hw);
            let n = self.engine.retire(ctx)?;
            rep.retired += n;
            if n != 0 && self.epoch != 0 {
                self.proof.recovered = true;
            }
        }
        if self.kick(ex, gm, log, now)? {
            rep.submitted = 1;
        }
        Ok(rep)
    }

    /// The engine's stall bound, by name: `ENGINE-HUNG` or `ELSP-NOT-ACKED`.
    pub fn check_hang(&mut self, now: u64) -> Result<(), ExecError> {
        Ok(self.engine.check_hang(now)?)
    }

    /// Service until nothing is outstanding. `now()` is the caller's monotonic clock (the unit of `EngineConfig::stall_bound`).
    /// Stops with the hang verdict when the engine declares one, and after [`WAIT_IDLE_PASSES`] passes regardless.
    pub fn wait_idle<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, mut now: impl FnMut() -> u64) -> Result<ServiceReport, ExecError> {
        let mut total = ServiceReport::default();
        for _ in 0..WAIT_IDLE_PASSES {
            let t = now();
            let r = self.service(ex, gm, log, t)?;
            total.csb_events += r.csb_events;
            total.csb_completions += r.csb_completions;
            total.retired += r.retired;
            total.submitted += r.submitted;
            if self.engine.requests().is_empty() && self.engine.execlists().is_idle() {
                return Ok(total);
            }
            self.check_hang(now())?;
            ex.relax();
        }
        Err(ExecError::Bound { what: "wait_idle passes", cap: WAIT_IDLE_PASSES })
    }

    /// Recovery after a declared hang or a CSB error. `scope` is `Engines` (this engine only) or `AllEngines` (every engine of the
    /// GT, passed in `gt_engines`). One step: `ENGINE-RESET` / `FULL-RESET`.
    pub fn recover<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, ex: &mut Executor<B>, gm: &mut GpuMem<D, M>, log: &mut L, scope: ResetScope, gt_engines: &[EngineInfo]) -> Result<ResetOutcome, ExecError> {
        let (id, name) = if scope == ResetScope::AllEngines { (StepId::FULL_RESET, "FULL-RESET") } else { (StepId::ENGINE_RESET, "ENGINE-RESET") };
        let mine = [*self.engine.info()];
        let engines: &[EngineInfo] = if scope == ResetScope::AllEngines { gt_engines } else { &mine };
        let mut planner = ResetPlanner::new(scope, engines, DEFAULT_MAX_POLLS)?;
        ex.step(log, id, name, |ex, log| {
            let out = ex.run_reset(&mut planner)?;
            let csb = self.engine.reset_tracking()?;
            let regs = csb.steps.as_slice().iter().filter_map(|s| if let Step::Mmio(Effect::Write(w)) = s { Some(w.reg) } else { None });
            let d = ex.domains_for(regs);
            {
                let mut sink = self.hwsp_sink(gm);
                ex.awake(d, |ex| ex.run_csb_steps(csb.steps.as_slice(), &mut sink))?;
            }
            // execlists_reset_cancel: whatever was outstanding is lost; mark it complete on its timeline and retire it, then scrub.
            for ctx in 0..C as u32 {
                if self.objs[ctx as usize].is_none() {
                    continue;
                }
                let tl = *self.engine.context_timeline(ctx)?;
                gm.store32(self.seqno_dword(tl.hwsp_ggtt)?, tl.seqno())?;
                self.engine.observe_seqno(ctx, tl.seqno())?;
                self.engine.retire(ctx)?;
                self.engine.reset_context(ctx)?;
                self.store_state(gm, ctx)?;
            }
            gm.flush(self.hwsp.device_addr, 4096)?;
            let resume = self.engine.plan_resume()?;
            let d = ex.domains_for(Executor::<B>::gt_regs(resume.as_slice()));
            ex.awake(d, |ex| ex.gt_effects_awake(resume.as_slice()))?;
            self.epoch += 1;
            self.proof.recovered = false;
            let _ = log;
            Ok(out)
        })
    }

    /// Record the isolation proof: the planner's page-walk model denied context A's buffer to context B AND B's to A.
    pub fn note_isolation(&mut self, a_denied_to_b: bool, b_denied_to_a: bool) {
        self.proof.isolation_both_ways = a_denied_to_b && b_denied_to_a;
    }

    /// Print `[I915-EXEC seqno=N csb=complete isolation=bothways reset_epoch=E recovery=pass]` ONLY when every fact holds (a request
    /// completed, a CSB completion was consumed, isolation proven both ways, at least one reset executed and a request completed
    /// after it). Otherwise one `[I915-EXEC INCOMPLETE ...]` line with the facts as they are. Returns whether the marker printed.
    pub fn emit_marker<L: Log>(&self, log: &mut L) -> bool {
        let p = &self.proof;
        let ok = p.seqno != 0 && p.csb_complete && p.isolation_both_ways && self.epoch != 0 && p.recovered;
        if ok {
            emit(log, format_args!("[I915-EXEC seqno={} csb=complete isolation=bothways reset_epoch={} recovery=pass]", p.seqno, self.epoch));
        } else {
            emit(
                log,
                format_args!(
                    "[I915-EXEC INCOMPLETE seqno={} csb={} isolation={} reset_epoch={} recovery={}]",
                    p.seqno,
                    if p.csb_complete { "complete" } else { "none" },
                    if p.isolation_both_ways { "bothways" } else { "UNPROVEN" },
                    self.epoch,
                    if p.recovered { "pass" } else { "none" }
                ),
            );
        }
        ok
    }
}

/// `GEN8_CSB_ENTRIES` re-exported for hosts that model the status page.
pub const CSB_ENTRIES: u32 = GEN8_CSB_ENTRIES;
