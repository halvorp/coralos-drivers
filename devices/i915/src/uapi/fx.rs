// SPDX-License-Identifier: GPL-2.0-only
//! Applying what the planners asked for. gem-core never touches memory or a register: it emits [`Effect`]s, and this is the one place
//! that carries them out - free pages, flush a cache line, patch a relocation, create or drop a fence, write the ELSP, drop a hardware
//! context or an address space's pages. [`UapiService::with_sink`] runs a planner call with the adapters and then applies its effects
//! in order.
use super::adapt::{Hwx, Sink};
use super::fail::Fail;
use super::pool::Run;
use super::service::{bind_ggtt_pages, Hw, HwCtx, VmRec, HWSP_TIMELINE_BASE, HWSP_TIMELINE_SLOTS, HWSP_TIMELINE_STRIDE};
use super::UapiService;
use crate::exec::dma::{CpuMem, Grant, ObjKind};
use crate::exec::engine::CtxObjects;
use crate::log::Log;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{Effect, Gem, SessionId, VmId};
use i915_syncobj_core::FenceId;
use i915_wire::errno;

/// What applying one submission's effects produced.
#[derive(Default)]
pub(crate) struct Acc {
    /// The request's own fence (made before the effects run); signalled by retirement.
    pub req_fence: Option<FenceId>,
    pub out_fence: Option<u32>,
    /// Fences still pending that the request waits on.
    pub gate: Vec<FenceId>,
    /// `(index in the request's relocation table, presumed_offset)` of every relocation patched.
    pub updates: Vec<(u32, u64)>,
    pub submitted: bool,
}

impl UapiService {
    /// Run a planner call that takes only an effect sink, then apply its effects. Returns the call's result and the first failure the
    /// sink or the effects hit (a bind that could not be written, a table that could not grow).
    pub(crate) fn with_sink<B: Mmio, D: Dma, M: CpuMem, L: Log, R>(
        &mut self,
        hw: &mut Hw<'_, B, D, M, L>,
        session: u64,
        f: impl FnOnce(&mut Gem, &mut Sink<'_, '_, B, D, M, L>) -> R,
    ) -> (R, Option<Fail>) {
        let busy = !self.inflight.is_empty();
        let gate = Cell::new(false);
        let (r, fx, err) = {
            let UapiService { gem, sync, .. } = &mut *self;
            let hwx = RefCell::new(Hwx { ex: &mut *hw.ex, gm: &mut *hw.gm, log: &mut *hw.log });
            let mut sink = Sink { hw: &hwx, fx: Vec::new(), err: None, busy, gate: &gate, sync: &*sync, session };
            let r = f(gem, &mut sink);
            (r, sink.fx, sink.err)
        };
        let mut acc = Acc::default();
        let aerr = self.apply_effects(hw, session, fx, &mut acc).err();
        self.drop_acc(acc);
        (r, err.or(aerr))
    }

    pub(crate) fn drop_acc(&mut self, acc: Acc) {
        if let Some(f) = acc.req_fence {
            self.sync.put_fence(f);
        }
        for f in acc.gate {
            self.sync.put_fence(f);
        }
    }

    /// Carry out `fx` in order. Every effect is attempted even after a failure (a `FreePages` must not be skipped); the first failure
    /// is returned.
    pub(crate) fn apply_effects<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, session: u64, fx: Vec<Effect>, acc: &mut Acc) -> Result<(), Fail> {
        let mut first: Option<Fail> = None;
        let mut note = |r: Result<(), Fail>| {
            if let Err(e) = r {
                first.get_or_insert(e);
            }
        };
        for e in fx {
            match e {
                Effect::FreePages { obj } => {
                    if let Some(p) = self.objs.take(obj) {
                        self.pool.release(p.run);
                    }
                }
                Effect::ObjectFreed { .. } | Effect::AwaitRequest { .. } | Effect::RevokeMmap { .. } => {}
                Effect::Clflush { obj } => {
                    if let Some(p) = self.objs.get(obj) {
                        let (mut at, mut left) = (p.run.dev, p.run.bytes());
                        while left > 0 {
                            let n = left.min(1 << 20);
                            note(hw.gm.flush(at, n as u32).map_err(Fail::from));
                            at += n as u64;
                            left -= n;
                        }
                    }
                }
                Effect::PatchReloc { owner, byte_offset, value, width, reloc_index, presumed } => {
                    let r = match self.objs.get(owner) {
                        None => Err(Fail::new(errno::EFAULT, "EXEC-RELOC-OWNER-NO-PAGES")),
                        Some(p) => {
                            let at = p.pages.get((byte_offset / 4096) as usize).map(|pg| pg + byte_offset % 4096);
                            match at {
                                None => Err(Fail::new(errno::EINVAL, "EXEC-RELOC-OUT-OF-OBJECT")),
                                Some(at) => {
                                    let w = if width == 8 { hw.gm.store64(at, value) } else { hw.gm.store32(at, value as u32) };
                                    w.and_then(|()| hw.gm.flush(at, 8)).map_err(Fail::from)
                                }
                            }
                        }
                    };
                    if r.is_ok() {
                        if acc.updates.try_reserve(1).is_ok() {
                            acc.updates.push((reloc_index, presumed));
                        } else {
                            note(Err(Fail::oom("reloc-updates")));
                        }
                    }
                    note(r);
                }
                Effect::AwaitSyncobj { handle, point, .. } => match self.sync.await_fence(session, handle, point) {
                    Ok(f) => self.gate_on(f, acc, &mut note),
                    Err(_) => {} // "a point might have been signaled already and garbage collected" (gem/i915_gem_execbuffer.c:2872-2880)
                },
                Effect::AwaitInFence { token, .. } => match self.sync.file_fence(session, token) {
                    Ok(f) => {
                        self.sync.get_fence(f);
                        self.gate_on(f, acc, &mut note);
                    }
                    Err(e) => note(Err(e.into())),
                },
                Effect::SignalSyncobj { handle, point, .. } => match acc.req_fence {
                    Some(f) => note(self.sync.attach_out(session, handle, point, f).map_err(Fail::from)),
                    None => note(Err(Fail::new(errno::EIO, "NO-REQUEST-FENCE"))),
                },
                Effect::OutFence { .. } => match acc.req_fence {
                    Some(f) => match self.sync.file_from_fence(session, f) {
                        Ok(t) => acc.out_fence = Some(t),
                        Err(e) => note(Err(e.into())),
                    },
                    None => note(Err(Fail::new(errno::EIO, "NO-REQUEST-FENCE"))),
                },
                Effect::Submit { plan, .. } => match self.rig.run_submission(hw.ex, hw.gm, hw.log, &plan) {
                    Ok(()) => acc.submitted = true,
                    Err(e) => note(Err(e.into())),
                },
                Effect::ContextFreed { ctx_slot } => self.free_hw_ctx(hw, ctx_slot),
                Effect::VmFreed { vm } => self.free_vm_pages(vm),
            }
        }
        first.map_or(Ok(()), Err)
    }

    /// Keep `f` (a reference the caller owns) in the gate list if it is still pending, else drop it.
    fn gate_on(&mut self, f: FenceId, acc: &mut Acc, note: &mut impl FnMut(Result<(), Fail>)) {
        if self.sync.fence_status(f) == i915_syncobj_core::FenceStatus::Pending {
            if acc.gate.try_reserve(1).is_ok() {
                acc.gate.push(f);
                return;
            }
            note(Err(Fail::oom("gate")));
        }
        self.sync.put_fence(f);
    }

    // ---- address spaces ---------------------------------------------------------------------------------------------------------

    /// Attribute the pages of a just-created address space to its `VmId` (found by its PML4). The caller reserved room in `vms`.
    pub(crate) fn record_vm(&mut self, base: Run, pml4: u64) {
        for i in 0..self.gem.vm_slots() {
            if self.gem.vm_pml4(VmId(i as u32)) == Some(pml4) && !self.vms.iter().any(|v| v.vm == VmId(i as u32)) {
                self.vms.push(VmRec { vm: VmId(i as u32), base, tables: Vec::new() });
                return;
            }
        }
        // not found: the space was never created; give the pages back
        self.pool.release(base);
    }

    pub(crate) fn free_vm_pages(&mut self, vm: VmId) {
        if let Some(i) = self.vms.iter().position(|v| v.vm == vm) {
            let rec = self.vms.swap_remove(i);
            self.pool.release(rec.base);
            for t in rec.tables {
                self.pool.release(t);
            }
        }
    }

    /// Page-table pages the planner took while planning for `vm`.
    pub(crate) fn adopt_tables(&mut self, vm: VmId, taken: Vec<Run>) {
        let mut taken = taken.into_iter();
        if let Some(rec) = self.vms.iter_mut().find(|v| v.vm == vm) {
            for r in taken.by_ref() {
                if rec.tables.try_reserve(1).is_err() {
                    self.pool.release(r); // cannot record it: back to the pool rather than lost (the space may then reuse it; see report)
                    continue;
                }
                rec.tables.push(r);
            }
        }
        for r in taken {
            self.pool.release(r);
        }
    }

    // ---- hardware contexts --------------------------------------------------------------------------------------------------------

    /// Make sure the GEM context `(session, ctx_id)` has its execlist context on the render engine; create and attach it if not.
    pub(crate) fn ensure_hw_ctx<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, session: u64, ctx_id: u32) -> Result<(), Fail> {
        let sid = SessionId(session);
        let slot = self.gem.context_slot_of(sid, ctx_id)?;
        if self.hwctx.iter().any(|h| h.slot == slot) {
            return Ok(());
        }
        let vm = self.gem.context_vm(sid, ctx_id)?;
        let pml4 = self.gem.vm_pml4(vm).ok_or(Fail::new(errno::EIO, "VM-GONE"))?;
        let tl_slot = self.tl_used.iter().position(|u| !u).ok_or(Fail::new(errno::ENOMEM, "HWSP-TIMELINE-SLOTS-EXHAUSTED"))?;
        self.hwctx.try_reserve(1).map_err(|_| Fail::oom("hw-contexts"))?;
        let image_pages = (self.rig.engine.info().context_size as usize).div_ceil(4096);
        let image = self.pool.alloc(hw.gm, image_pages)?;
        let ring = match self.pool.alloc(hw.gm, 1) {
            Ok(r) => r,
            Err(e) => {
                self.pool.release(image);
                return Err(e);
            }
        };
        let undo = |me: &mut UapiService, hw: &mut Hw<'_, B, D, M, L>, bound: &[VmaIdPair]| {
            for (vma, _) in bound {
                if let Ok(p) = me.ggtt.plan_unbind(*vma) {
                    let _ = hw.ex.run_bind_step(hw.log, hw.gm, &p, true, &mut |_| true);
                }
            }
            me.pool.release(image);
            me.pool.release(ring);
        };
        let ig = Grant { kind: ObjKind::ContextImage, cpu_mapping: image.cpu, device_addr: image.dev, len: image.bytes() };
        let rg = Grant { kind: ObjKind::Ring, cpu_mapping: ring.cpu, device_addr: ring.dev, len: ring.bytes() };
        let (image_ggtt, image_vma) = match self.bind_run(hw, &ig) {
            Ok(x) => x,
            Err(e) => {
                undo(self, hw, &[]);
                return Err(e);
            }
        };
        let (ring_ggtt, ring_vma) = match self.bind_run(hw, &rg) {
            Ok(x) => x,
            Err(e) => {
                undo(self, hw, &[(image_vma, ())]);
                return Err(e);
            }
        };
        let objs = CtxObjects { image: ig, image_ggtt, ring: rg, ring_ggtt };
        let timeline_ggtt = self.hwsp_ggtt + HWSP_TIMELINE_BASE + tl_slot as u32 * HWSP_TIMELINE_STRIDE;
        let ectx = match self.rig.add_context(hw.gm, objs, pml4, timeline_ggtt, 0, true) {
            Ok(id) => id,
            Err(e) => {
                undo(self, hw, &[(image_vma, ()), (ring_vma, ())]);
                return Err(e.into());
            }
        };
        if let Err(e) = self.gem.attach_engine_context(sid, ctx_id, 0, 0, ectx) {
            let _ = self.rig.remove_context(ectx);
            undo(self, hw, &[(image_vma, ()), (ring_vma, ())]);
            return Err(e.into());
        }
        self.tl_used[tl_slot] = true;
        self.hwctx.push(HwCtx { slot, ectx, image, ring, image_vma, ring_vma, tl_slot });
        Ok(())
    }

    fn bind_run<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, g: &Grant) -> Result<(u32, i915_memory_core::VmaId), Fail> {
        let pages: Vec<u64> = {
            let mut v = Vec::new();
            v.try_reserve_exact(g.pages()).map_err(|_| Fail::oom("ggtt-pages"))?;
            (0..g.pages()).for_each(|i| v.push(g.page(i)));
            v
        };
        bind_ggtt_pages(hw, &mut self.ggtt, &pages)
    }

    /// `ContextFreed`: drop the engine context, unbind its image and ring, give the pages back.
    pub(crate) fn free_hw_ctx<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, slot: u32) {
        let Some(i) = self.hwctx.iter().position(|h| h.slot == slot) else { return };
        let h = self.hwctx[i];
        if self.rig.remove_context(h.ectx).is_err() {
            return; // still has a request in flight: the next retire frees it
        }
        for vma in [h.image_vma, h.ring_vma] {
            if let Ok(p) = self.ggtt.plan_unbind(vma) {
                let _ = hw.ex.run_bind_step(hw.log, hw.gm, &p, true, &mut |_| true);
            }
        }
        self.pool.release(h.image);
        self.pool.release(h.ring);
        if h.tl_slot < HWSP_TIMELINE_SLOTS {
            self.tl_used[h.tl_slot] = false;
        }
        self.hwctx.swap_remove(i);
    }
}

type VmaIdPair = (i915_memory_core::VmaId, ());

