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
use super::pool::{PagePool, Run};
use super::Fail;
use crate::exec::dma::{CpuMem, GpuMem};
use crate::exec::engine::EngineRig;
use crate::exec::{ExecError, Executor};
use crate::log::Log;
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};
use coralos_hal::{Dma, Mmio};
use i915_gem_core::{Effect, EffectSink, EngineSet, ObjId, PageRequest, PageSource, PagesRefused, SyncTables, VmId};
use i915_memory_core::{BindPlan, MemError, TablePage, TablePageRequest, TablePageSource, UnbindPlan};
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

/// Request construction and the ELSP dequeue on the one routed engine.
pub struct Eng<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub rig: &'a mut EngineRig,
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
    /// An earlier queued request still waits on a fence.
    pub held: bool,
    /// THIS submission has a fence that is not signalled yet (set by the sink as it sees the await effects).
    pub gate: &'a Cell<bool>,
    /// The real error behind a `SubmitError` the trait could not carry.
    pub err: Option<ExecError>,
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> EngineSet for Eng<'_, '_, B, D, M, L> {
    fn build_request(&mut self, _class: u16, _instance: u16, engine_ctx: u32, batch: &BatchSpec) -> Result<RequestRef, SubmitError> {
        let mut hw = self.hw.borrow_mut();
        self.rig.build_request(hw.gm, engine_ctx, batch).map_err(|e| match e {
            ExecError::Submit(s) => s,
            other => {
                self.err = Some(other);
                SubmitError::OutOfMemory { what: "ring access" }
            }
        })
    }
    fn dequeue(&mut self, _class: u16, _instance: u16, now: u64) -> Result<Option<SubmissionPlan>, SubmitError> {
        if self.held || self.gate.get() || self.rig.hold {
            return Ok(None); // queued behind a fence: the service kicks it when the fence signals
        }
        self.rig.engine.dequeue(now)
    }
}

/// Everything the planner asked for that is not a PPGTT write, in order.
pub struct Sink<'a, 'h, B: Mmio, D: Dma, M: CpuMem, L: Log> {
    pub hw: &'a RefCell<Hwx<'h, B, D, M, L>>,
    pub fx: Vec<Effect>,
    pub err: Option<Fail>,
    /// Requests are in flight: an `AwaitRetire` victim cannot be proven retired, so a bind that needs one waits and is refused.
    pub busy: bool,
    pub gate: &'a Cell<bool>,
    pub sync: &'a Sync,
    pub session: u64,
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> Sink<'_, '_, B, D, M, L> {
    fn run(&mut self, plan: &BindPlan<'_>) {
        let busy = self.busy;
        let mut hw = self.hw.borrow_mut();
        let Hwx { ex, gm, log } = &mut *hw;
        if let Err(e) = ex.run_bind_step(*log, *gm, plan, false, &mut |_| !busy) {
            if self.err.is_none() {
                self.err = Some(Fail::from(e));
            }
        }
    }
}

impl<B: Mmio, D: Dma, M: CpuMem, L: Log> EffectSink for Sink<'_, '_, B, D, M, L> {
    fn bind(&mut self, _vm: VmId, _obj: ObjId, plan: &BindPlan<'_>) {
        self.run(plan);
    }
    fn unbind(&mut self, _vm: VmId, plan: &UnbindPlan) {
        self.run(plan);
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
