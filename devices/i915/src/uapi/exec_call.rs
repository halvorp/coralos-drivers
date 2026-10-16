// SPDX-License-Identifier: GPL-2.0-only
//! EXECBUFFER2 and the engine's life around it: submission, gating on unsignalled fences, completion, retirement, hang recovery.
//!
//! A submission is: decode the flattened request (uapi-core) -> make sure the context has its hardware context -> plan
//! (`Gem::execbuffer2`: validate, place, relocate, build the request in the ring, register fences) -> apply the recorded effects ->
//! kick the engine. The request gets ONE fence of its own (made before the call); the out-syncobjs and the out sync_file are views of
//! it, and retirement ([`UapiService::poll`]) signals it.
//!
//! IN-ORDER ENGINE: the render engine runs requests in submission order, so a request that waits on a fence that is still pending
//! HOLDS the engine ([`crate::exec::engine::EngineRig::hold`]) - it and everything queued behind it stay in their rings until the fence
//! signals. A fence is always made by an earlier submission, a syncobj signal or a sync_file import, so the producers run first or are
//! the client's own call; a wait-before-signal across two requests of ONE engine would deadlock the engine and is NOT supported
//! (the Linux scheduler would reorder them). It is documented, not hidden.
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
        let sid = SessionId(inc);
        let req = decode_execbuf2(p, &ExecLimits::DEFAULT)?;
        // D18: a write into a scanout image the display holds (COMMITTED / SCANNING_OUT) or the authority never began is refused before
        // anything is reserved
        #[cfg(feature = "p4-present")]
        self.check_scanout_writes(hw.log, sid, req.objects.iter().map(|o| (o.handle, o.flags)))?;
        let ctx_id = req.context_id();
        self.ensure_hw_ctx(hw, inc, ctx_id)?;
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

        let busy = !self.inflight.is_empty();
        let held = !self.gates.is_empty();
        let gate = Cell::new(gate_cells);
        let taken = RefCell::new(Vec::new());
        let (res, fx, sink_err, eng_err) = {
            let UapiService { gem, sync, rig, pool, objs, .. } = &mut *self;
            let hwx = RefCell::new(Hwx { ex: &mut *hw.ex, gm: &mut *hw.gm, log: &mut *hw.log });
            let pool_cell = RefCell::new(&mut *pool);
            let view = SyncView { sync: &*sync, session: inc };
            let mut pages = Pages { pool: &pool_cell, objs, hw: &hwx };
            let mut tables = Tables { pool: &pool_cell, taken: &taken, hw: &hwx };
            let mut eng = Eng { rig, hw: &hwx, held, gate: &gate, err: None };
            let mut sink = Sink { hw: &hwx, fx: Vec::new(), err: None, busy, gate: &gate, sync: &*sync, session: inc };
            let r = gem.execbuffer2(sid, &view, &req, &mut pages, &mut tables, &mut eng, &mut sink, hw.now_ms);
            (r, sink.fx, sink.err, eng.err)
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
        self.inflight.push(Inflight { req: done.request, ectx: done.engine_ctx, seqno: done.seqno, fence }); // reserved above
        if acc.gate.is_empty() {
            if self.gates.is_empty() {
                self.rig.hold = false;
                match self.rig.kick(hw.ex, hw.gm, hw.log, hw.now_ms) {
                    Ok(started) => acc.submitted |= started,
                    Err(e) => emit(hw.log, format_args!("[I915-UAPI kick failed name={}]", e.name())),
                }
            }
        } else {
            let fences = core::mem::take(&mut acc.gate);
            self.gates.push(Gate { fences }); // reserved above
            self.rig.hold = true;
        }
        let (submitted, out_fence, updates) = (done.submitted || acc.submitted, acc.out_fence, core::mem::take(&mut acc.updates));
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

    /// Recompute whether the engine is held after gates were dropped (a session died, a fence signalled).
    pub(crate) fn refresh_gate_state(&mut self) {
        self.rig.hold = !self.gates.is_empty();
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
        // gates first: a fence that signalled lets the engine go on
        let mut i = 0;
        while i < self.gates.len() {
            if self.gates[i].fences.iter().all(|f| self.sync.fence_status(*f) != FenceStatus::Pending) {
                let g = self.gates.swap_remove(i);
                for f in g.fences {
                    self.sync.put_fence(f);
                }
            } else {
                i += 1;
            }
        }
        self.refresh_gate_state();
        if !self.hwctx.is_empty() {
            if let Err(e) = self.rig.service(hw.ex, hw.gm, hw.log, hw.now_ms) {
                emit(hw.log, format_args!("[I915-UAPI service failed name={}]", e.name()));
            }
            if let Err(e) = self.rig.check_hang(hw.now_ms) {
                emit(hw.log, format_args!("[I915-UAPI HANG name={}]", e.name()));
                self.recover_hang(hw);
            }
            self.observe_and_complete(hw);
        }
        #[cfg(feature = "p4-present")]
        self.advance_scanout(hw);
        self.poll_waiters(hw);
        self.trim_pool(hw);
    }

    fn observe_and_complete<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        for h in 0..self.hwctx.len() {
            let ectx = self.hwctx[h].ectx;
            if let Ok(v) = self.rig.engine.hw_seqno(ectx) {
                let _ = self.gem.observe_seqno(0, ectx, v);
            }
        }
        let mut i = 0;
        while i < self.inflight.len() {
            let f = &self.inflight[i];
            let hw_seq = self.rig.engine.hw_seqno(f.ectx).unwrap_or(0);
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

    /// The engine declared a hang: reset it, fail every request in flight with `-EIO`, charge the guilty context (the head request)
    /// and let gem-core retire them. A request that was in flight is LOST; the clients learn it from the fence error and from
    /// GET-RESET-STATS.
    fn recover_hang<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        let engines = match engines_for(&self.gem.device().desc.info) {
            Ok(e) => e,
            Err(_) => return,
        };
        if let Err(e) = self.rig.recover(hw.ex, hw.gm, hw.log, ResetScope::Engines, engines.as_slice()) {
            emit(hw.log, format_args!("[I915-UAPI recovery failed name={}]", e.name()));
            return;
        }
        let mut first = true;
        for f in core::mem::take(&mut self.inflight) {
            let _ = self.gem.request_reset(0, f.ectx, f.seqno, first, hw.now_ms);
            first = false;
            let _ = self.gem.observe_seqno(0, f.ectx, f.seqno); // lost: gem-core retires it
            self.sync.signal_fence(f.fence, -errno::EIO);
            self.sync.put_fence(f.fence);
        }
    }
}
