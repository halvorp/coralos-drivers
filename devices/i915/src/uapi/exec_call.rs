// SPDX-License-Identifier: GPL-2.0-only
//! EXECBUFFER2 and the engine's life around it: submission, gating on unsignalled fences, completion, retirement, hang recovery.
//!
//! A submission is: decode the flattened request (uapi-core) -> make sure the context has its hardware context -> plan
//! (`Gem::execbuffer2`: validate, place, relocate, build the request in the ring, register fences) -> apply the recorded effects ->
//! kick the engine. The request gets ONE fence of its own (made before the call); the out-syncobjs and the out sync_file are views of
//! it, and retirement ([`UapiService::poll`]) signals it.
//!
//! Gated requests: a request that waits on a fence still
//! pending is built into its ring at once but marked NOT READY in the engine (`Engine::set_gated`). The dequeue skips it and every later
//! request of the same context (a ring runs in order) and goes on publishing READY requests of other contexts, so the request that will
//! signal the fence is never held back by its consumer; `RING_TAIL` is published only up to a context's last READY request. When every
//! fence of the gate has signalled, [`UapiService::poll`] marks the request READY and the same poll's service publishes it.
use super::adapt::{Eng, Hwx, Pages, Sink, SyncView, Tables};
use super::fail::Fail;
use super::fx::Acc;
use super::service::{Done, Gate, Hw, Inflight, Waiter, WaitKind};
use super::UapiService;
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use coralos_hal::{Dma, Mmio};
use i915_gem_core::SessionId;
use i915_gt_core::engine::engines_for;
use i915_gt_core::reset::ResetScope;
use i915_submit_core::request::seqno_passed;
use i915_syncobj_core::FenceStatus;
use i915_uapi_core::{decode_execbuf2, ExecLimits};
use i915_wire::codec::Writer;
use i915_wire::msg::exec::{ExecRepHead, RelocUpdate, REP_OUT_FENCE, REP_SUBMITTED};
use i915_wire::msg::{self, Body};
use i915_wire::{errno, Header, Status};

impl UapiService {
    pub(crate) fn call_exec<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, inc: u64, p: &[u8], out: &mut [u8]) -> Result<Done, Fail> {
        if self.wedged {
            return Err(Fail::new(errno::EIO, "GT-WEDGED")); // intel_gt_is_wedged: a submission to a wedged GT fails at once
        }
        let sid = SessionId(inc);
        let req = decode_execbuf2(p, &ExecLimits::DEFAULT)?;
        // the video engines are routed to the media seat alone: the engine choice below sees this session's view
        self.gem.set_media_seat(self.holds_media_seat(inc));
        // D18: a write into a scanout image the display holds (COMMITTED / SCANNING_OUT) or the authority never began is refused before
        // anything is reserved
        #[cfg(feature = "p4-present")]
        self.check_scanout_writes(hw.log, sid, req.objects.iter().map(|o| (o.handle, o.flags)))?;
        let ctx_id = req.context_id();
        // the engine the planner will choose, decided the same way first, so the context's hardware context on it exists (lazily made)
        let engine = self.gem.exec_engine(sid, &SyncView { sync: &self.sync, session: inc }, &req)?;
        self.ensure_hw_ctx(hw, inc, ctx_id, engine)?;
        let vm = self.gem.context_vm(sid, ctx_id)?;

        // everything this submission will need to remember, reserved BEFORE the engine is asked for a request
        self.inflight.try_reserve(1).map_err(|_| Fail::oom("requests"))?;
        self.gates.try_reserve(1).map_err(|_| Fail::oom("gates"))?;
        let mut acc = Acc::default();
        acc.req_fence = Some(self.sync.new_fence()?);

        // implicit fences imported onto objects this submission uses (DMA_BUF_IOCTL_IMPORT_SYNC_FILE)
        let mut gate_cells = false;
        for o in req.objects.iter() {
            let Ok(obj) = self.gem.lookup_object(sid, o.handle) else { continue };
            let mut i = 0;
            while i < self.implicit.len() {
                let (io, f, _) = self.implicit[i];
                if io != obj {
                    i += 1;
                } else if self.sync.fence_status(f) == FenceStatus::Pending {
                    if acc.gate.try_reserve(1).is_err() {
                        self.drop_acc(acc);
                        return Err(Fail::oom("gate"));
                    }
                    self.sync.get_fence(f);
                    acc.gate.push(f);
                    gate_cells = true;
                    i += 1;
                } else {
                    let (_, f, _) = self.implicit.swap_remove(i);
                    self.sync.put_fence(f);
                }
            }
        }

        let mut uses = Vec::new();
        let have_uses = self.gem.vma_uses(&mut uses).is_ok();
        let gate = Cell::new(gate_cells);
        let taken = RefCell::new(Vec::new());
        let (res, fx, sink_err, eng_err, batch) = {
            let UapiService { gem, sync, rigs, pool, objs, .. } = &mut *self;
            let hwx = RefCell::new(Hwx { ex: &mut *hw.ex, gm: &mut *hw.gm, log: &mut *hw.log });
            let pool_cell = RefCell::new(&mut *pool);
            let view = SyncView { sync: &*sync, session: inc };
            let mut pages = Pages { pool: &pool_cell, objs, hw: &hwx };
            let mut tables = Tables { pool: &pool_cell, taken: &taken, hw: &hwx };
            let rigs = RefCell::new(rigs.as_mut_slice());
            let mut eng = Eng { rigs: &rigs, engine: engine.0, hw: &hwx, gate: &gate, built: None, batch: None, err: None };
            let uses = have_uses.then_some(uses.as_slice());
            let mut sink = Sink { hw: &hwx, fx: Vec::new(), err: None, gate: &gate, sync: &*sync, session: inc, rigs: &rigs, uses, now_ms: hw.now_ms };
            let r = gem.execbuffer2(sid, &view, &req, &mut pages, &mut tables, &mut eng, &mut sink, hw.now_ms);
            (r, sink.fx, sink.err, eng.err, eng.batch)
        };
        self.adopt_tables(vm, taken.into_inner());

        let done = match res {
            Ok(d) => d,
            Err(e) => {
                let mut f = eng_err.map_or(Fail::from(e), Fail::from);
                if let Some(s) = sink_err {
                    f = s;
                }
                let _ = self.apply_effects(hw, inc, fx, &mut acc);
                self.drop_acc(acc);
                return Err(f);
            }
        };
        let fx: Vec<_> = if sink_err.is_some() { fx.into_iter().filter(|e| !matches!(e, i915_gem_core::Effect::Submit { .. })).collect() } else { fx };
        let applied = self.apply_effects(hw, inc, fx, &mut acc);

        // the request is registered in gem-core now: it is tracked whatever else went wrong
        let fence = acc.req_fence.take().expect("made above");
        let e = done.engine;
        self.inflight.push(Inflight { req: done.request, engine: e, ectx: done.engine_ctx, seqno: done.seqno, fence, batch }); // reserved above
        // the request's gate is exactly the fences still pending now (the planner's view may have been one fence earlier); a request
        // the planner's dequeue already handed to a port cannot be gated any more (it had no pending fence then)
        let gated = !acc.gate.is_empty();
        if let Err(err) = self.rigs[e].engine.set_gated(done.engine_ctx, done.seqno, gated) {
            if gated {
                emit(hw.log, format_args!("[I915-UAPI gate failed name={} engine={} ectx={} seqno={}]", err.name(), e, done.engine_ctx, done.seqno));
            }
        }
        if gated {
            let fences = core::mem::take(&mut acc.gate);
            self.gates.push(Gate { fences, engine: e, ectx: done.engine_ctx, seqno: done.seqno }); // reserved above
        }
        // whatever is READY goes now - other contexts, and this request unless it waits on a fence
        if let Err(err) = self.rigs[e].kick(hw.ex, hw.gm, hw.log, hw.now_ms) {
            emit(hw.log, format_args!("[I915-UAPI kick failed name={} engine={}]", err.name(), e));
        }
        let mine = self.rigs[e].engine.requests().iter().find(|r| r.ctx == done.engine_ctx && r.seqno == done.seqno).map_or(true, |r| r.submitted);
        let (submitted, out_fence, updates) = (mine, acc.out_fence, core::mem::take(&mut acc.updates));
        self.drop_acc(acc);
        if let Some(e) = sink_err.or(applied.err()) {
            return Err(e);
        }
        // reply: head, canonical offsets, relocation updates
        let need = msg::REPLY_HEAD_BYTES + ExecRepHead::SIZE + done.offsets.len() * 8 + updates.len() * RelocUpdate::SIZE;
        if out.len() < need {
            return Err(Fail::new(errno::EFAULT, "REPLY-TOO-LARGE"));
        }
        let mut w = Writer::new(out);
        msg::encode_reply_head(&mut w, Status::Ok, 0)?;
        let flags = if submitted { REP_SUBMITTED } else { 0 } | if out_fence.is_some() { REP_OUT_FENCE } else { 0 };
        ExecRepHead { request_id: done.request.0, seqno: done.seqno, flags, out_fence: out_fence.unwrap_or(0), n_offsets: done.offsets.len() as u32, n_reloc_updates: updates.len() as u32, ..Default::default() }.encode(&mut w)?;
        for o in &done.offsets {
            w.u64(*o)?;
        }
        for (index, presumed_offset) in &updates {
            RelocUpdate { index: *index, presumed_offset: *presumed_offset, ..Default::default() }.encode(&mut w)?;
        }
        Ok(Done::Len(w.pos()))
    }


    /// Park a blocking call: its reply comes out of [`UapiService::take_ready`] later.
    pub(crate) fn park(&mut self, hdr: Header, kind: WaitKind, now_ms: u64, deadline_ms: Option<u64>) -> Result<(), Fail> {
        self.waiters.try_reserve(1).map_err(|_| Fail::oom("parked-calls"))?;
        self.waiters.push(Waiter { hdr, kind, start_ms: now_ms, deadline_ms });
        Ok(())
    }

    /// The next reply a parked call produced, as a whole frame. Call until `None`.
    pub fn take_ready(&mut self) -> Option<Vec<u8>> {
        if self.ready.is_empty() {
            None
        } else {
            Some(self.ready.remove(0))
        }
    }

    /// Advance the device: service the engine (CSB, seqnos, retirement), signal the fences of completed requests, retire in gem-core,
    /// release gated requests whose fences signalled, declare and recover a hang, and answer parked calls that can now be answered.
    /// Cheap when idle. Call it from the device's loop whenever the engine interrupts and at least every few milliseconds.
    pub fn poll<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        if !self.hwctx.is_empty() {
            // every engine is serviced and its completions taken BEFORE any engine is judged: a request whose breadcrumb already
            // landed is complete, never lost to a reset and replayed a second time (on this engine or, after an escalation, another)
            for e in 0..self.rigs.len() {
                if let Err(err) = self.rigs[e].service(hw.ex, hw.gm, hw.log, hw.now_ms) {
                    emit(hw.log, format_args!("[I915-UAPI service failed name={} engine={}]", err.name(), e));
                }
            }
            self.observe_and_complete(hw);
            // each engine is judged on its own: a hang on one is recovered without touching the other's work
            let mut recovered = false;
            for e in 0..self.rigs.len() {
                if let Err(err) = self.rigs[e].check_hang(hw.now_ms) {
                    emit(hw.log, format_args!("[I915-UAPI HANG name={} engine={}]", err.name(), e));
                    self.recover_hang(hw, e);
                    recovered = true;
                }
            }
            if recovered {
                self.observe_and_complete(hw); // gem-core retires what the reset failed
            }
        }
        // then the gates: a request whose fences all signalled - on any engine, this tick's completions included - becomes READY and
        // is dequeued now, not one timer period later (a decode on the video engine feeding the compositor's request on the render engine)
        let mut i = 0;
        while i < self.gates.len() {
            if self.gates[i].fences.iter().all(|f| self.sync.fence_status(*f) != FenceStatus::Pending) {
                let g = self.gates.swap_remove(i);
                for f in g.fences {
                    self.sync.put_fence(f);
                }
                // READY now: the kick below publishes it (a request a reset already cancelled is gone: nothing to open)
                if let Some(r) = self.rigs.get_mut(g.engine) {
                    let _ = r.engine.set_gated(g.ectx, g.seqno, false);
                }
            } else {
                i += 1;
            }
        }
        for r in self.rigs.iter_mut() {
            if let Err(err) = r.kick(hw.ex, hw.gm, hw.log, hw.now_ms) {
                emit(hw.log, format_args!("[I915-UAPI kick failed name={}]", err.name()));
            }
        }
        #[cfg(feature = "p4-present")]
        self.advance_scanout(hw);
        self.poll_waiters(hw);
        self.trim_pool(hw);
    }

    fn observe_and_complete<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        for h in 0..self.hwctx.len() {
            let (e, ectx) = (self.hwctx[h].engine, self.hwctx[h].ectx);
            if let Some(Ok(v)) = self.rigs.get(e).map(|r| r.engine.hw_seqno(ectx)) {
                let _ = self.gem.observe_seqno(e, ectx, v);
            }
        }
        let mut i = 0;
        while i < self.inflight.len() {
            let f = &self.inflight[i];
            let hw_seq = self.rigs.get(f.engine).and_then(|r| r.engine.hw_seqno(f.ectx).ok()).unwrap_or(0);
            if seqno_passed(hw_seq, f.seqno) {
                let f = self.inflight.swap_remove(i);
                self.sync.signal_fence(f.fence, 0);
                self.sync.put_fence(f.fence);
            } else {
                i += 1;
            }
        }
        let (_, err) = self.with_sink(hw, 0, |gem, sink| gem.retire(sink));
        if let Some(e) = err {
            emit(hw.log, format_args!("[I915-UAPI retire failed name={}]", e.name));
        }
    }

    /// THE RESET COORDINATOR. Engine `e` declared a hang. Gen8/9 reset an engine on its own (its `GEN6_GRDOM_*` bit,
    /// gt/intel_reset.c `gen8_reset_engines`): engine `e` is reset ALONE and only its requests are lost, so the other engines' work
    /// (the compositor's on the render engine while the video engine hung) runs on. Only when that engine reset fails is the whole GT
    /// reset (`GEN6_GRDOM_FULL`): then every other rig resumes from it too and EVERY engine's in-flight work is lost. Of the lost
    /// requests only the one the hung engine was executing is GUILTY (charged for GET-RESET-STATS, failed `-EIO`); the others are
    /// innocent and replayed ([`super::replay`]). A failed GT reset leaves the engines as they are and says so (WEDGED).
    fn recover_hang<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, e: usize) {
        let engines = match engines_for(&self.gem.device().desc.info) {
            Ok(v) => v,
            Err(_) => return,
        };
        // the request the engine was executing, read before the reset clears the ports: the one guilty request
        let hung = self.hung_request(e);
        let scope = match self.rigs[e].recover(hw.ex, hw.gm, hw.log, ResetScope::Engines, engines.as_slice()) {
            Ok(_) => ResetScope::Engines,
            Err(err) => {
                emit(hw.log, format_args!("[I915-UAPI engine reset failed name={} engine={} - resetting the whole GT]", err.name(), e));
                if let Err(err) = self.rigs[e].recover(hw.ex, hw.gm, hw.log, ResetScope::AllEngines, engines.as_slice()) {
                    // the GT did not come back: WEDGED (intel_gt_set_wedged). Every fence fails now and nothing is submitted again.
                    emit(hw.log, format_args!("[I915-UAPI WEDGED name={} engine={} - every request fails -EIO from now on]", err.name(), e));
                    self.wedged = true;
                    for f in core::mem::take(&mut self.inflight) {
                        let _ = self.gem.request_reset(f.engine, f.ectx, f.seqno, f.engine == e, hw.now_ms);
                        let _ = self.gem.observe_seqno(f.engine, f.ectx, f.seqno);
                        self.sync.signal_fence(f.fence, -errno::EIO);
                        self.sync.put_fence(f.fence);
                    }
                    for g in core::mem::take(&mut self.gates) {
                        for f in g.fences {
                            self.sync.put_fence(f);
                        }
                    }
                    return;
                }
                for o in (0..self.rigs.len()).filter(|&o| o != e) {
                    if let Err(err) = self.rigs[o].resume_after_gt_reset(hw.ex, hw.gm, hw.log) {
                        emit(hw.log, format_args!("[I915-UAPI GT-reset resume failed name={} engine={}]", err.name(), o));
                    }
                }
                ResetScope::AllEngines
            }
        };
        self.settle_lost(hw, e, scope, hung);
        emit(hw.log, format_args!("[I915-UAPI RESET engine={} scope={}]", e, if scope == ResetScope::AllEngines { "gt" } else { "engine" }));
    }
}
