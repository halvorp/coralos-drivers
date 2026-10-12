// SPDX-License-Identifier: GPL-2.0-only
//! The scheduling half of the engine: `execlists_dequeue` (without priority, preemption or timeslicing), `execlists_update_context`,
//! `execlists_submit_ports`, the schedule-in/out accounting, CSB consumption, request retirement and the stall bound.
//! (gt/intel_execlists_submission.c:472-733, :905-939, :1270-1615; i915_request.c:358-418.)
//!
//! NOT ported, by name: priority queues and `need_preempt` / `timeslice_expired` / `defer_active` (every request is equal, the
//! queue is FIFO in creation order), virtual engines, `GVT-g` single-port contexts, the capture-on-reset path
//! (`execlists_capture`), and `reset_rewind` (rewinding the ring head to the oldest incomplete request after a reset).
//! `ring_set_paused(1)` is therefore never issued; the status page's `I915_GEM_HWS_PREEMPT` dword stays 0 and the preemption
//! semaphore in each fini breadcrumb never blocks.
use crate::engine::Engine;
use crate::error::SubmitError;
use crate::execlists::{elsp_writes, plan_enable_execlists, plan_reset_csb_pointers, CsbOutcome, CsbResetPlan, Port, NUM_PORTS};
use crate::lrc_reg::*;
use crate::request::{ccid_for_tag, seqno_passed, tag_for_ccid};
use crate::ring::Ring;
use i915_gt_core::{Effect, FixedVec, WritePlan};

/// A store into the context image the executor performs before the ELSP writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageStore {
    pub ctx: u32,
    /// Dword index from the start of the context object (`LRC_STATE_OFFSET / 4 + CTX_RING_TAIL`).
    pub dword: u32,
    pub value: u32,
}

/// One submission: the stores that complete the context images, then the four ELSP writes (port 1's upper and lower
/// descriptor dwords, then port 0's). The ELSP writes are raw (`writel`) in Linux, not `uncore_write`: the caller holds the
/// engine's forcewake as the GT power management does on Gen9 (gt/intel_gt_pm.c:179).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubmissionPlan {
    pub ports: [Option<Port>; NUM_PORTS],
    pub descs: [u64; NUM_PORTS],
    /// Image stores, in the order `execlists_submit_ports` makes them (port 1 first). Each precedes the ELSP writes (`wmb()`).
    pub image_stores: FixedVec<ImageStore, NUM_PORTS>,
    pub elsp: [WritePlan; 4],
}

/// The stall watchdog state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watch {
    marker: Option<(u32, u32, bool)>,
    since: u64,
    pending_since: Option<u64>,
}

impl Watch {
    pub const fn new() -> Watch {
        Watch { marker: None, since: 0, pending_since: None }
    }
}

impl Default for Watch {
    fn default() -> Self {
        Watch::new()
    }
}

/// `WaIdleLiteRestore` (execlists_update_context, :683-702): a submission whose tail does not move forward from the previous one
/// (`intel_ring_direction(ring, tail, prev) <= 0`, :702) must reload the whole context instead of lite-restoring. Unreachable through
/// FIFO scheduling (each request is listed at most twice before it completes, and the NOOPs make the second listing advance);
/// it fires when a rewind (preemption, timeslicing) re-lists a request, so it is a function the tests can drive directly.
pub fn forces_restore(ring: &Ring, tail: u32, prev: u32) -> bool {
    ring.direction(tail, prev) <= 0
}

impl Engine {
    /// `completed(rq)` for a port: the context's timeline has reached the port's seqno.
    fn port_completed(&self, p: &Port) -> bool {
        match self.ctx(p.ctx) {
            Ok(c) => seqno_passed(c.hw_seqno, p.seqno),
            Err(_) => true,
        }
    }

    fn request_index(&self, p: &Port) -> Result<usize, SubmitError> {
        self.reqs.as_slice().iter().position(|r| r.ctx == p.ctx && r.seqno == p.seqno).ok_or(SubmitError::NoSuchRequest { ctx: p.ctx, seqno: p.seqno })
    }

    /// `execlists_schedule_in` (:472-541): count the port against its context; the first one assigns a hardware context id.
    fn schedule_in(&mut self, ctx: u32) -> Result<(), SubmitError> {
        let first = self.ctx(ctx)?.inflight == 0;
        let ccid = if first { Some(ccid_for_tag(self.tags.alloc()?)) } else { None };
        let c = self.ctx_mut(ctx)?;
        if let Some(ccid) = ccid {
            c.ccid = ccid;
        }
        c.inflight += 1;
        Ok(())
    }

    /// `execlists_schedule_out` (:586-660): the last port to leave returns the context's tag.
    fn schedule_out(&mut self, ctx: u32) -> Result<(), SubmitError> {
        let c = self.ctx_mut(ctx)?;
        c.inflight = c.inflight.saturating_sub(1);
        if c.inflight == 0 {
            if let Some(tag) = tag_for_ccid(c.ccid) {
                self.tags.release(tag);
            }
        }
        Ok(())
    }

    /// `execlists_update_context` (:672-721): publish the request's tail in the context image and build the descriptor. A tail
    /// that does not advance (`WaIdleLiteRestore`) forces a full context restore.
    fn update_context(&mut self, p: &Port, image: &mut FixedVec<ImageStore, NUM_PORTS>) -> Result<u64, SubmitError> {
        let ri = self.request_index(p)?;
        let rq_tail = self.reqs.as_slice()[ri].tail;
        let rq_wa_tail = self.reqs.as_slice()[ri].wa_tail;
        let c = self.ctx_mut(p.ctx)?;
        let mut desc = (c.ccid as u64) << GEN8_CTX_ID_SHIFT | c.desc_lo as u64;
        let prev = c.ring.tail();
        let tail = c.ring.set_tail(rq_tail)?;
        if forces_restore(&c.ring, tail, prev) {
            desc |= CTX_DESC_FORCE_RESTORE;
        }
        c.regs.set(CTX_RING_TAIL, tail)?;
        c.desc_lo &= !(CTX_DESC_FORCE_RESTORE as u32);
        let _ = image.push(ImageStore { ctx: p.ctx, dword: LRC_STATE_OFFSET / 4 + CTX_RING_TAIL as u32, value: tail });
        self.reqs.as_mut_slice()[ri].tail = rq_wa_tail; // rq->tail = rq->wa_tail
        Ok(desc)
    }

    /// `execlists_dequeue` (:1270-1615), FIFO and without preemption: choose up to two ports and submit them. Returns `None` when
    /// there is nothing to do: a previous submission is still unacknowledged, two contexts are already in flight, or the queue is
    /// empty. `now` is the caller's clock, recorded for the unacknowledged-submission bound.
    pub fn dequeue(&mut self, now: u64) -> Result<Option<SubmissionPlan>, SubmitError> {
        if self.execlists.has_pending() {
            return Ok(None); // __execlists_submission_tasklet only dequeues with !pending[0] (:2477-2478)
        }
        // active = execlists->active; skip completed ports (:1315)
        let inflight = *self.execlists.inflight();
        let mut ai = self.execlists.active_index();
        while let Some(Some(p)) = inflight.get(ai) {
            if self.port_completed(p) {
                ai += 1;
            } else {
                break;
            }
        }
        let mut last = inflight.get(ai).copied().flatten();
        if last.is_some() && inflight.get(ai + 1).copied().flatten().is_some() {
            return Ok(None); // "if (active[1]) ... return" (:1398): wait for the next CS event
        }
        // Phase 1: choose the ports without touching state.
        let mut ports: [Option<Port>; NUM_PORTS] = [None; NUM_PORTS];
        let mut n = 0usize;
        let mut submit = false;
        let mut consumed_upto = 0usize; // requests at table indices below this are handed to the hardware
        for (i, rq) in self.reqs.as_slice().iter().enumerate() {
            if rq.submitted {
                continue;
            }
            let cand = Port { ctx: rq.ctx, seqno: rq.seqno };
            if let Some(l) = last {
                if l.ctx != rq.ctx {
                    // !can_merge_rq: needs the next port
                    if n == NUM_PORTS - 1 {
                        break; // port == last_port: done (:1512)
                    }
                    ports[n] = Some(l);
                    n += 1;
                }
            }
            last = Some(cand);
            submit = true;
            consumed_upto = i + 1;
        }
        ports[n] = last;
        if !submit {
            return Ok(None);
        }
        // `memcmp(active, pending, ...)`: identical to what is already running means no submission (:1599-1602).
        let same = (0..=n).all(|k| ports[k] == inflight.get(ai + k).copied().flatten());
        if same {
            return Ok(None);
        }
        // Everything that can refuse is checked before anything changes.
        let mut need_tags = 0u32;
        for p in ports.iter().flatten() {
            self.request_index(p)?;
            let c = self.ctx(p.ctx)?;
            if c.inflight == 0 && !ports.iter().flatten().take_while(|q| *q != p).any(|q| q.ctx == p.ctx) {
                need_tags += 1;
            }
            let rq_tail = self.reqs.as_slice()[self.request_index(p)?].tail;
            c.ring.check_tail(rq_tail)?;
        }
        if need_tags > self.tags.free_tags() {
            return Err(SubmitError::TagsExhausted);
        }
        // Phase 2: schedule in (last port first, :1604), mark submitted, write the ports (last port first).
        for k in (0..NUM_PORTS).rev() {
            if let Some(p) = ports[k] {
                self.schedule_in(p.ctx)?;
            }
        }
        self.execlists.set_pending(ports)?;
        for rq in self.reqs.as_mut_slice().iter_mut().take(consumed_upto) {
            rq.submitted = true;
        }
        let mut image = FixedVec::new("image stores", ImageStore { ctx: 0, dword: 0, value: 0 });
        let mut descs = [0u64; NUM_PORTS];
        for k in (0..NUM_PORTS).rev() {
            descs[k] = match ports[k] {
                Some(p) => self.update_context(&p, &mut image)?,
                None => 0,
            };
        }
        self.watch.pending_since = Some(now);
        Ok(Some(SubmissionPlan { ports, descs, image_stores: image, elsp: elsp_writes(self.info.mmio_base, descs) }))
    }

    /// Consume the context status buffer (`process_csb` + `post_process_csb`, :1814-2052): ports that leave flight are scheduled
    /// out. On an error the engine has lost track of the hardware: call [`Engine::reset_tracking`].
    pub fn process_csb(&mut self, csb_write: u32, hwsp: impl FnMut(u32) -> u64, mmio: impl FnMut(u32) -> u64) -> Result<CsbOutcome, SubmitError> {
        let out = self.execlists.process_csb(csb_write, hwsp, mmio)?;
        for p in out.inactive.as_slice() {
            self.schedule_out(p.ctx)?;
        }
        if !self.execlists.has_pending() {
            self.watch.pending_since = None;
        }
        Ok(out)
    }

    /// The executor read the timeline's HWSP seqno dword for `ctx`.
    pub fn observe_seqno(&mut self, ctx: u32, value: u32) -> Result<(), SubmitError> {
        self.ctx_mut(ctx)?.hw_seqno = value;
        Ok(())
    }

    /// `i915_request_retire` for every completed request of `ctx`, oldest first (i915_request.c:358-418): the ring head moves to
    /// each request's `postfix`. Returns how many retired. The ring's cached `space` is refreshed lazily by
    /// [`crate::ring::Ring::begin`], as in Linux.
    pub fn retire(&mut self, ctx: u32) -> Result<u32, SubmitError> {
        let hw = self.ctx(ctx)?.hw_seqno;
        let mut retired = 0;
        while let Some(i) = self.reqs.as_slice().iter().position(|r| r.ctx == ctx) {
            let rq = self.reqs.as_slice()[i];
            if !seqno_passed(hw, rq.seqno) {
                break;
            }
            self.reqs.remove(i);
            self.ctx_mut(ctx)?.ring.set_head(rq.postfix)?;
            retired += 1;
        }
        Ok(retired)
    }

    /// The stall bound. Call periodically with the caller's monotonic clock. Declares the engine hung, by name, when
    /// (a) the oldest submitted, incomplete request has neither started nor completed for `stall_bound` units, or (b) an ELSP
    /// submission has gone unacknowledged by a CSB event for `stall_bound` units. Progress is the oldest request changing or its
    /// "started" state flipping (`__i915_request_has_started`: the HWSP has reached `seqno - 1`).
    pub fn check_hang(&mut self, now: u64) -> Result<(), SubmitError> {
        let bound = self.cfg.stall_bound;
        if let Some(since) = self.watch.pending_since {
            if self.execlists.has_pending() {
                let waited = now.saturating_sub(since);
                if waited >= bound {
                    return Err(SubmitError::ElspNotAcked { waited, bound });
                }
            } else {
                self.watch.pending_since = None;
            }
        }
        let mut oldest = None;
        for rq in self.reqs.as_slice() {
            if !rq.submitted {
                continue;
            }
            let hw = self.ctx(rq.ctx)?.hw_seqno;
            if !seqno_passed(hw, rq.seqno) {
                oldest = Some((rq.ctx, rq.seqno, seqno_passed(hw, rq.seqno.wrapping_sub(1))));
                break;
            }
        }
        match oldest {
            None => self.watch.marker = None,
            Some(m) => {
                if self.watch.marker != Some(m) {
                    self.watch.marker = Some(m);
                    self.watch.since = now;
                } else {
                    let stalled = now.saturating_sub(self.watch.since);
                    if stalled >= bound {
                        return Err(SubmitError::Hung { ctx: m.0, seqno: m.1, stalled, bound });
                    }
                }
            }
        }
        Ok(())
    }

    /// After an engine reset (or a CSB error): schedule out every outstanding port, forget the stall state, and return the plan
    /// that puts the hardware's CSB pointers back to entry 0 (`reset_csb_pointers`). Unfinished requests keep their state, untouched:
    /// replaying or cancelling them (`execlists_reset_rewind`, `execlists_reset_cancel`) is NOT ported, so a caller that resets a
    /// hung engine must treat those requests as lost (Linux wedges the GT when a reset cannot recover).
    pub fn reset_tracking(&mut self) -> Result<CsbResetPlan, SubmitError> {
        let ports = self.execlists.reset();
        for p in ports.as_slice() {
            self.schedule_out(p.ctx)?;
        }
        self.watch = Watch::new();
        Ok(plan_reset_csb_pointers(self.info.mmio_base))
    }

    /// `execlists_resume`'s register writes (`enable_execlists`), for the executor to run once forcewake is held.
    pub fn plan_resume(&self) -> Result<FixedVec<Effect, 5>, SubmitError> {
        plan_enable_execlists(self.info.mmio_base, self.cfg.status_page_ggtt)
    }

    /// `reset_csb_pointers`, for engine bring-up (`execlists_sanitize`).
    pub fn plan_reset_csb_pointers(&self) -> CsbResetPlan {
        plan_reset_csb_pointers(self.info.mmio_base)
    }
}
