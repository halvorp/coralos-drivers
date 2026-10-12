// SPDX-License-Identifier: GPL-2.0-only
//! Activity tracking: which unretired requests touch which objects, what the hardware has completed (HWSP seqno observations),
//! and the three questions asked of it - GEM_BUSY, GEM_WAIT and request retirement. A request is COMPLETE when its context's
//! timeline seqno has passed it (`i915_request_completed`, i915_request.h:612; `seqno_passed` of `i915-submit-core`); it is
//! RETIRED when [`Gem::retire`] removes it, which is what lets a closed object or context finally be freed.
//! Ported from gem/i915_gem_busy.c (v7.0-rc6), gem/i915_gem_wait.c, i915_request.c.
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::{ObjId, RequestId, SessionId};
use crate::table::{self, Quota};
use alloc::vec::Vec;
use crate::seam::EffectSink;
use i915_submit_core::request::seqno_passed;

#[derive(Clone, Debug)]
pub(crate) struct ActiveReq {
    pub id: RequestId,
    /// Index in the device's user-visible engine order.
    pub engine: u8,
    /// The `i915_submit_core` context id (its timeline is what completes the request).
    pub ectx: u32,
    pub seqno: u32,
    /// The GEM context slot that made it.
    pub ctx: u32,
    /// The objects the request touches and whether it writes each; sized to the submission (a per-request list, not a table cap).
    pub objs: Vec<(ObjId, bool)>,
    /// `i915_request_set_error_once`: `-EIO` for a guilty request, `-EAGAIN` for an innocent one (gt/intel_reset.c:134-138).
    pub error: Option<i32>,
}

impl ActiveReq {
    pub(crate) fn touches(&self, obj: ObjId) -> Option<bool> {
        self.objs.iter().find(|(o, _)| *o == obj).map(|(_, w)| *w)
    }
}

pub(crate) struct Active {
    pub reqs: Vec<Option<ActiveReq>>,
    /// Last HWSP seqno observed per `(engine, engine context)`.
    pub(crate) hw: Vec<(u8, u32, u32)>,
    pub next_id: u64,
}

impl Active {
    pub(crate) const fn new() -> Active {
        Active { reqs: Vec::new(), hw: Vec::new(), next_id: 1 }
    }
    pub(crate) fn hw_seqno(&self, engine: u8, ectx: u32) -> u32 {
        self.hw.iter().find(|(e, c, _)| *e == engine && *c == ectx).map_or(0, |(_, _, v)| *v)
    }
    pub(crate) fn observe(&mut self, engine: u8, ectx: u32, value: u32, q: &mut Quota) -> Result<(), GemError> {
        if let Some(h) = self.hw.iter_mut().find(|(e, c, _)| *e == engine && *c == ectx) {
            h.2 = value;
            return Ok(());
        }
        table::push(&mut self.hw, (engine, ectx, value), q, "hw-seqnos")?;
        Ok(())
    }
    pub(crate) fn completed(&self, r: &ActiveReq) -> bool {
        seqno_passed(self.hw_seqno(r.engine, r.ectx), r.seqno)
    }
    /// A free slot for a request: reuse one or grow the table (charged, fallible). The slot stays empty until the request exists.
    pub(crate) fn reserve_slot(&mut self, q: &mut Quota) -> Result<usize, GemError> {
        match self.reqs.iter().position(|r| r.is_none()) {
            Some(i) => Ok(i),
            None => table::push(&mut self.reqs, None, q, "requests"),
        }
    }
    pub(crate) fn refs_object(&self, obj: ObjId) -> bool {
        self.reqs.iter().flatten().any(|r| r.touches(obj).is_some())
    }
    pub(crate) fn refs_context(&self, ctx: u32) -> bool {
        self.reqs.iter().flatten().any(|r| r.ctx == ctx)
    }
    pub(crate) fn find(&self, engine: u8, ectx: u32, seqno: u32) -> Option<usize> {
        self.reqs.iter().position(|r| matches!(r, Some(r) if r.engine == engine && r.ectx == ectx && r.seqno == seqno))
    }
    /// Drop the observation of a hardware context that no longer exists.
    pub(crate) fn forget(&mut self, engine: u8, ectx: u32) {
        self.hw.retain(|(e, c, _)| !(*e == engine && *c == ectx));
    }
    pub(crate) fn len(&self) -> usize {
        self.reqs.iter().flatten().count()
    }
}

/// The outcome of `GEM_WAIT` before any blocking: nothing to wait for, or the caller must block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Every request touching the object is complete (`dma_resv_wait_timeout` returned at once): the ioctl returns 0.
    Idle,
    /// `pending` requests are incomplete; block until they complete (call [`Gem::observe_seqno`] / [`Gem::wait_plan`] again) or
    /// `timeout` runs out, then report through [`wait_finish`].
    Blocked { pending: u32, timeout: WaitTimeout },
}

/// `to_wait_timeout` (gem/i915_gem_wait.c:197-207).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitTimeout {
    /// `timeout_ns < 0`: `MAX_SCHEDULE_TIMEOUT`.
    Forever,
    Ns(u64),
}

/// The `timeout_ns` accounting after the wait returned (gem/i915_gem_wait.c:255-273). `still_busy` says the wait ended by
/// timeout (`-ETIME`). `jiffy_ns` is the kernel's tick length (`nsecs_to_jiffies(n) == 0` iff `n < jiffy_ns`).
/// Returns the value to write back to `args->timeout_ns` and the ioctl result.
pub fn wait_finish(timeout_ns: i64, elapsed_ns: u64, still_busy: bool, jiffy_ns: u64) -> (i64, Result<(), GemError>) {
    let mut ret = if still_busy { Err(GemError::WaitTimeout) } else { Ok(()) };
    let mut out = timeout_ns;
    if timeout_ns > 0 {
        out = timeout_ns.saturating_sub(elapsed_ns.min(i64::MAX as u64) as i64); // gem/i915_gem_wait.c:256
        if out < 0 {
            out = 0; // gem/i915_gem_wait.c:257-258
        }
        if ret == Err(GemError::WaitTimeout) && (out as u64) < jiffy_ns {
            out = 0; // gem/i915_gem_wait.c:267-268
        }
        if ret == Err(GemError::WaitTimeout) && out != 0 {
            ret = Err(GemError::WaitRestart); // gem/i915_gem_wait.c:271-272
        }
    }
    (out, ret)
}

impl Gem {
    /// The executor read the HWSP seqno dword of `engine_ctx`'s timeline on engine `engine` (index in the device's engine order).
    pub fn observe_seqno(&mut self, engine: usize, engine_ctx: u32, value: u32) -> Result<(), GemError> {
        if engine >= self.dev.engines().len() {
            return Err(GemError::EngineIndex { index: engine });
        }
        self.active.observe(engine as u8, engine_ctx, value, &mut self.quota)
    }

    /// `i915_gem_busy_ioctl` (gem/i915_gem_busy.c:112-165): bit `16 + class` per engine class reading the object, and in the low
    /// half `class + 1` of the class that writes it (which also sets that class's read bit, gem/i915_gem_busy.c:22-38). Complete requests count as idle.
    pub fn busy(&self, session: SessionId, handle: u32) -> Result<u32, GemError> {
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let engines = self.dev.engines();
        let mut busy = 0u32;
        for r in self.active.reqs.iter().flatten() {
            let Some(write) = r.touches(id) else { continue };
            if self.active.completed(r) {
                continue; // __busy_set_if_active: `i915_request_completed(rq)` -> 0, gem/i915_gem_busy.c:87-88
            }
            let class = engines.as_slice().get(r.engine as usize).map_or(0xffff, |e| e.class);
            let read_flag = if class == 0xffff { 0xffff_0000 } else { 0x1_0000u32 << class }; // __busy_read_flag, gem/i915_gem_busy.c:13-20
            busy |= if write {
                if class == 0xffff {
                    0xffff_ffff // __busy_write_id, gem/i915_gem_busy.c:22-38
                } else {
                    (class as u32 + 1) | read_flag
                }
            } else {
                read_flag
            };
        }
        Ok(busy)
    }

    /// `i915_gem_wait_ioctl` up to the point of blocking (gem/i915_gem_wait.c:233-253): flags must be zero; a zero timeout is
    /// the busy ioctl and answers `-ETIME` at once if anything is incomplete (`i915_gem_object_wait`, gem/i915_gem_wait.c:184).
    pub fn wait_plan(&self, session: SessionId, handle: u32, flags: u32, timeout_ns: i64) -> Result<Wait, GemError> {
        if flags != 0 {
            return Err(GemError::WaitFlags { flags }); // gem/i915_gem_wait.c:240-241
        }
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?; // gem/i915_gem_wait.c:243-245
        let pending = self.pending_on(id);
        if pending == 0 {
            return Ok(Wait::Idle);
        }
        match timeout_ns {
            0 => Err(GemError::WaitTimeout), // to_wait_timeout(0) == 0 -> -ETIME, gem/i915_gem_wait.c:184, gem/i915_gem_wait.c:201-202
            t if t < 0 => Ok(Wait::Blocked { pending, timeout: WaitTimeout::Forever }), // gem/i915_gem_wait.c:199-200
            t => Ok(Wait::Blocked { pending, timeout: WaitTimeout::Ns(t as u64) }),
        }
    }

    /// Incomplete requests touching `id` (the fences `i915_gem_object_wait_reservation` would wait on, I915_WAIT_ALL).
    pub(crate) fn pending_on(&self, id: ObjId) -> u32 {
        self.active.reqs.iter().flatten().filter(|r| r.touches(id).is_some() && !self.active.completed(r)).count() as u32
    }

    /// Is any incomplete request using the object?
    pub(crate) fn object_active(&self, id: ObjId) -> bool {
        self.pending_on(id) != 0
    }

    /// The incomplete request ids touching `id`, for a caller that blocks on them.
    pub fn pending_requests(&self, id: ObjId, out: &mut [RequestId]) -> usize {
        let mut n = 0;
        for r in self.active.reqs.iter().flatten() {
            if r.touches(id).is_some() && !self.active.completed(r) && n < out.len() {
                out[n] = r.id;
                n += 1;
            }
        }
        n
    }

    /// `i915_request_retire` for every completed request (i915_request.c:358-418), then free what that unblocks: closed objects
    /// (`__i915_gem_free_object`), closed contexts, unreferenced address spaces. Returns how many requests retired.
    pub fn retire(&mut self, sink: &mut dyn EffectSink) -> usize {
        let mut n = 0;
        for i in 0..self.active.reqs.len() {
            let done = matches!(&self.active.reqs[i], Some(r) if self.active.completed(r));
            if done {
                self.active.reqs[i] = None;
                n += 1;
            }
        }
        if n != 0 {
            self.sweep(sink);
        }
        n
    }

    /// Hardware contexts whose HWSP seqno is being tracked.
    pub fn observed_seqnos(&self) -> usize {
        self.active.hw.len()
    }

    /// Requests not yet retired.
    pub fn active_requests(&self) -> usize {
        self.active.len()
    }
}
