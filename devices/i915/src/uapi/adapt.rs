// SPDX-License-Identifier: GPL-2.0-only
//! The seams `i915-gem-core` plans against, implemented over the executor: object pages ([`Pages`]), page-table pages ([`Tables`]),
//! the engine's ring and dequeue ([`Eng`]), the effect receiver ([`Sink`]) and the read-only syncobj view ([`SyncView`]).
//!
//! WHY SO MANY ADAPTERS: `Gem::execbuffer2` takes the page source, the table source, the engine set and the effect sink as FOUR separate
//! `&mut`, and they all need the same hardware (the executor, the granted memory, the log). The hardware therefore lives in one
//! `RefCell` ([`Hwx`]) that every adapter borrows for the span of one method call; no method calls another adapter, so a borrow is
//! never held across a call into the planner.
//!
//! The sink does the two things that cannot wait (execute a PPGTT bind or unbind) and RECORDS every other effect: the service applies
//! them, in order, once the planner has returned and the pool, the syncobj table and the engine rig can be borrowed mutably again.
//!
//! RETIREMENT. A plan that unmaps a VMA the GPU may still use carries an `AwaitRetire` for it. The sink is the one waiter that can make
//! that wait end: the device has no engine interrupt and finds completion only by SERVICING the engine (CSB + seqno read + retire), so
//! [`Sink`] asks the per-VMA predicate [`vma_retired`] - every request that used the VMA (a [`VmaUse`] snapshot from gem-core) against
//! the seqno the hardware wrote, wrapping - and between asks parks [`COMPLETION_POLL_MS`] and services the engine, bounded by the
//! engine's stall bound (past it the work is hung; the refusal is `AWAIT-RETIRE-TIMEOUT`). A close never gets here with a busy object:
//! gem-core defers it until the retirement that frees it (`Gem::close_deferred`), so its unbind is proven on the first ask.
use super::pool::{PagePool, Run};
use super::service::COMPLETION_POLL_MS;
use super::Fail;
use crate::exec::dma::{CpuMem, GpuMem};
use crate::exec::engine::EngineRig;
use crate::exec::{ExecError, Executor};
use crate::log::Log;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{Effect, EffectSink, EngineSet, ObjId, PageRequest, PageSource, PagesRefused, SyncTables, VmId, VmaUse};
use i915_memory_core::{BindPlan, MemError, Op, TablePage, TablePageRequest, TablePageSource, UnbindPlan, VmaId};
use i915_submit_core::request::seqno_passed;
use i915_submit_core::{BatchSpec, RequestRef, SubmissionPlan, SubmitError};
use i915_syncobj_core::{FenceStatus, Sync};
use i915_uapi_core::SyncobjInfo;

/// The hardware every adapter shares.
pub struct Hwx<'a, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub ex: &'a mut Executor<B>,
    pub gm: &'a mut GpuMem<D, M>,
    pub log: &'a mut L,
}

/// Backing pages of one GEM object: one contiguous run, plus the page list the planner binds.
#[derive(Debug)]
pub struct ObjPages {
    pub id: ObjId,
    pub run: Run,
    pub pages: Vec<u64>,
}

/// Object pages by `ObjId::index()` (a slot, reused with a new generation).
#[derive(Debug, Default)]
pub struct ObjTable {
    pub slots: Vec<Option<ObjPages>>,
}

impl ObjTable {
    pub fn get(&self, id: ObjId) -> Option<&ObjPages> {
        self.slots.get(id.index() as usize)?.as_ref().filter(|p| p.id == id)
    }
    pub fn take(&mut self, id: ObjId) -> Option<ObjPages> {
        let s = self.slots.get_mut(id.index() as usize)?;
        if s.as_ref().is_some_and(|p| p.id == id) {
            s.take()
        } else {
            None
        }
    }
    pub fn live(&self) -> usize {
        self.slots.iter().flatten().count()
    }
}

pub struct Pages<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub pool: &'a RefCell<&'a mut PagePool>,
    pub objs: &'a mut ObjTable,
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> PageSource for Pages<'_, '_, B, D, M, L> {
    fn alloc_pages(&mut self, req: PageRequest) -> Result<(), PagesRefused> {
        if self.objs.get(req.obj).is_some() {
            return Ok(()); // idempotent for an object that already has them
        }
        let n = (req.bytes / 4096) as usize;
        let idx = req.obj.index() as usize;
        if self.objs.slots.len() <= idx {
            let extra = idx + 1 - self.objs.slots.len();
            self.objs.slots.try_reserve(extra).map_err(|_| PagesRefused)?;
            self.objs.slots.resize_with(idx + 1, || None);
        }
        let mut list: Vec<u64> = Vec::new();
        list.try_reserve_exact(n).map_err(|_| PagesRefused)?;
        let mut hw = self.hw.borrow_mut();
        let run = self.pool.borrow_mut().alloc(hw.gm, n).map_err(|_| PagesRefused)?;
        for i in 0..n {
            list.push(run.page(i));
        }
        self.objs.slots[idx] = Some(ObjPages { id: req.obj, run, pages: list });
        Ok(())
    }
    fn pages(&self, obj: ObjId) -> Option<&[u64]> {
        self.objs.get(obj).map(|p| p.pages.as_slice())
    }
}

/// Page-table pages for the memory planner. Every page handed out is also recorded in `taken` so the service can attribute it to the
/// address space it was planned for and free it with that space.
pub struct Tables<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub pool: &'a RefCell<&'a mut PagePool>,
    pub taken: &'a RefCell<Vec<Run>>,
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> TablePageSource for Tables<'_, '_, B, D, M, L> {
    fn table_page(&mut self, req: TablePageRequest) -> Result<TablePage, MemError> {
        let refuse = || MemError::TablePageRefused { level: req.level };
        let mut taken = self.taken.borrow_mut();
        taken.try_reserve(1).map_err(|_| refuse())?;
        let mut hw = self.hw.borrow_mut();
        let run = self.pool.borrow_mut().alloc(hw.gm, 1).map_err(|_| refuse())?;
        taken.push(run);
        Ok(TablePage { device_addr: run.dev })
    }
}

/// Request construction and the ELSP dequeue on the engine the planner chose.
pub struct Eng<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    /// Every engine rig by device engine index. Shared with the [`Sink`] (which services the engines while it waits for a victim);
    /// each method borrows it for its own span.
    pub rigs: &'a RefCell<&'a mut [EngineRig]>,
    /// The device engine index of this submission (what `Gem::exec_engine` resolved): the planner's `(class, instance)` maps to it.
    pub engine: usize,
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
    /// THIS submission has a fence that is not signalled yet (set by the sink as it sees the await effects): its request is marked
    /// gated in the engine before the dequeue, so the dequeue skips it (and its context's later requests) and publishes the rest.
    pub gate: &'a Cell<bool>,
    /// The request this submission built.
    pub built: Option<RequestRef>,
    /// The batch it was built for: kept with the request, so an innocent request a reset lost is built again
    /// ([`super::replay`]).
    pub batch: Option<BatchSpec>,
    /// The real error behind a `SubmitError` the trait could not carry.
    pub err: Option<ExecError>,
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> EngineSet for Eng<'_, '_, B, D, M, L> {
    fn build_request(&mut self, _class: u16, _instance: u16, engine_ctx: u32, batch: &BatchSpec) -> Result<RequestRef, SubmitError> {
        let mut hw = self.hw.borrow_mut();
        let mut rigs = self.rigs.borrow_mut();
        let rig = rigs.get_mut(self.engine).ok_or(SubmitError::NoSuchContext { id: engine_ctx })?;
        let r = rig.build_request(hw.gm, engine_ctx, batch);
        if let Ok(b) = r {
            self.built = Some(b);
            self.batch = Some(*batch);
        }
        r.map_err(|e| match e {
            ExecError::Submit(s) => s,
            other => {
                self.err = Some(other);
                SubmitError::OutOfMemory { what: "ring access" }
            }
        })
    }
    fn dequeue(&mut self, _class: u16, _instance: u16, now: u64) -> Result<Option<SubmissionPlan>, SubmitError> {
        let mut rigs = self.rigs.borrow_mut();
        let Some(rig) = rigs.get_mut(self.engine) else { return Ok(None) };
        if let (true, Some(b)) = (self.gate.get(), self.built) {
            rig.engine.set_gated(b.ctx, b.seqno, true)?; // waits on a fence: not READY until the service opens its gate
        }
        rig.engine.dequeue(now)
    }
}

/// Everything the planner asked for that is not a PPGTT write, in order.
pub struct Sink<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
    pub fx: Vec<Effect>,
    pub err: Option<Fail>,
    pub gate: &'a Cell<bool>,
    pub sync: &'a Sync,
    pub session: u64,
    /// The engines, serviced between asks while a victim retires (shared with [`Eng`] in an EXECBUFFER2).
    pub rigs: &'a RefCell<&'a mut [EngineRig]>,
    /// Every unretired use of a VMA when the planner call began (`Gem::vma_uses`). `None` = the snapshot could not be taken (no
    /// memory): a victim is then retired only once the engine has no request at all - conservative, never unsound.
    pub uses: Option<&'a [VmaUse]>,
    /// The device's clock at the start of the call (ms); a wait adds the time it parked.
    pub now_ms: u64,
}

/// Has VMA `vma` of address space `vm` retired? Only when EVERY listed use of it has completed: the request's seqno has been passed
/// by the seqno the hardware wrote on that request's timeline (`hw_seqno(engine, engine_ctx)`), by the WRAPPING comparison
/// (`i915_seqno_passed`, i915_request.h; `seqno_passed`). A use on an engine this device does not service (`hw_seqno` answers None:
/// no rig at that index) or on a timeline it cannot read is never proven complete. The ELSP is covered by the same test: a request still in a port
/// has not written its breadcrumb. The display's scanout is not a VMA use: it holds the OBJECT (a `prime_export` hold) through its
/// own GGTT binding, so a PPGTT unbind never touches what is on the glass and the pages outlive it.
pub fn vma_retired(uses: &[VmaUse], vm: VmId, vma: VmaId, hw_seqno: impl Fn(u8, u32) -> Option<u32>) -> bool {
    uses.iter().filter(|u| u.vm == vm && u.vma == vma).all(|u| hw_seqno(u.engine, u.engine_ctx).is_some_and(|hw| seqno_passed(hw, u.seqno)))
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> Sink<'_, '_, B, D, M, L> {
    /// The predicate as of now (no progress): [`vma_retired`] over the snapshot and the engine's last observed seqnos.
    fn retired_now(&self, vm: VmId, vma: VmaId) -> bool {
        let rigs = self.rigs.borrow();
        match self.uses {
            Some(u) => vma_retired(u, vm, vma, |e, ctx| rigs.get(e as usize).and_then(|r| r.engine.hw_seqno(ctx).ok())),
            None => rigs.iter().all(|r| r.engine.requests().is_empty()),
        }
    }

    fn first_pending(&self, vm: VmId, plan: &BindPlan<'_>) -> Option<VmaId> {
        plan.ops().iter().find_map(|op| match *op {
            Op::AwaitRetire { vma } if !self.retired_now(vm, vma) => Some(vma),
            _ => None,
        })
    }

    /// Wait until every victim of `plan` has retired, servicing the engine between asks (park [`COMPLETION_POLL_MS`], then CSB + seqno
    /// read + retire + kick): the predicate can only change because this wait drives the engine. Bounded by the engine's stall bound;
    /// past it the request is hung (the next poll's hang check recovers it) and the bind is refused by name.
    fn await_victims(&mut self, vm: VmId, plan: &BindPlan<'_>) -> Result<(), ExecError> {
        let max = self.rigs.borrow().iter().map(|r| r.stall_bound_ms).max().unwrap_or(0).div_ceil(COMPLETION_POLL_MS).max(1) as u32;
        let mut polls = 0u32;
        while let Some(vma) = self.first_pending(vm, plan) {
            if polls >= max {
                return Err(ExecError::AwaitRetireTimeout { vma: vma.0, polls });
            }
            polls += 1;
            let mut hw = self.hw.borrow_mut();
            let Hwx { ex, gm, log } = &mut *hw;
            ex.park_ms(COMPLETION_POLL_MS);
            let now = self.now_ms.saturating_add(polls as u64 * COMPLETION_POLL_MS);
            for r in self.rigs.borrow_mut().iter_mut() {
                r.service(ex, gm, *log, now)?;
            }
        }
        Ok(())
    }

    fn run(&mut self, vm: VmId, plan: &BindPlan<'_>) {
        let r = self.await_victims(vm, plan).and_then(|()| {
            let hwc = self.hw;
            let mut hw = hwc.borrow_mut();
            let Hwx { ex, gm, log } = &mut *hw;
            ex.run_bind_step(*log, *gm, plan, false, &mut |v| self.retired_now(vm, v)).map(|_| ())
        });
        if let Err(e) = r {
            if self.err.is_none() {
                self.err = Some(Fail::from(e));
            }
        }
    }
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> EffectSink for Sink<'_, '_, B, D, M, L> {
    fn bind(&mut self, vm: VmId, _obj: ObjId, plan: &BindPlan<'_>) {
        self.run(vm, plan);
    }
    fn unbind(&mut self, vm: VmId, plan: &UnbindPlan) {
        self.run(vm, plan);
    }
    fn effect(&mut self, e: Effect) {
        // a submission that must wait for a fence that is still pending is gated (the dequeue that follows reads this)
        let pending = match e {
            Effect::AwaitSyncobj { handle, point, .. } => self.sync.wait_status(self.session, handle, point) == Some(FenceStatus::Pending),
            Effect::AwaitInFence { token, .. } => self.sync.file_status(self.session, token) == Ok(FenceStatus::Pending),
            _ => false,
        };
        if pending {
            self.gate.set(true);
        }
        if self.fx.try_reserve(1).is_err() {
            if self.err.is_none() {
                self.err = Some(Fail::oom("effects"));
            }
            return;
        }
        self.fx.push(e);
    }
}

/// `SyncTables` over the syncobj table, read-only, for one session.
pub struct SyncView<'a> {
    pub sync: &'a Sync,
    pub session: u64,
}

impl SyncTables for SyncView<'_> {
    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo> {
        let (has_fence, timeline) = self.sync.info(self.session, handle)?;
        let chain_seqno = if timeline {
            let mut q = [0u64];
            self.sync.query(self.session, &[handle], i915_syncobj_core::QUERY_LAST_SUBMITTED, &mut q).ok().map(|()| q[0])
        } else {
            None
        };
        Some(SyncobjInfo { has_fence, chain_seqno })
    }
    fn sync_file(&self, token: u32) -> bool {
        self.sync.file_exists(self.session, token)
    }
}
