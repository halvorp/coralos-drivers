// SPDX-License-Identifier: GPL-2.0-only
//! After a reset: which lost request was GUILTY, and the REPLAY of the innocent ones.
//!
//! Linux charges only the request the hung engine was executing (`execlists_reset_active` -> `active_context` / `active_request`,
//! gt/intel_execlists_submission.c:3035, :3053; `__i915_request_reset(rq, stalled)`, gt/intel_reset.c:125). Every other request that
//! was in flight on a reset engine is unwound and submitted again (`__unwind_incomplete_requests`, gt/intel_execlists_submission.c:373,
//! from `execlists_reset_rewind` :3117): a second client sharing the engine does not lose its work to the first one's hang.
//!
//! Here the rig's reset completes every context's timeline (`EngineRig::recover` -> the scrub), so a lost innocent request is BUILT
//! AGAIN on its context from the batch it was submitted with, under the timeline's next seqno, and gem-core's record of it moves to
//! that seqno ([`i915_gem_core::Gem::request_replayed`]): the request keeps its id, objects, out-fence and in-fence gate. A request
//! that cannot be replayed - its context was banned by this hang, its batch is unknown, or the engine refuses to build it - fails
//! with `-EIO` exactly as before.
use super::service::{Hw, Inflight};
use super::UapiService;
use crate::exec::dma::CpuMem;
use crate::log::{emit, Log};
use core::cmp::Ordering;
use coralos_hal::{Dma, Mmio};
use i915_gt_core::reset::ResetScope;
use i915_submit_core::request::seqno_passed;
use i915_wire::errno;

/// Oldest first on one timeline (`i915_seqno_passed` order, so a wrapped seqno still sorts after the one before it).
fn timeline_order(a: &Inflight, b: &Inflight) -> Ordering {
    (a.engine, a.ectx).cmp(&(b.engine, b.ectx)).then_with(|| (a.seqno.wrapping_sub(b.seqno) as i32).cmp(&0))
}

impl UapiService {
    /// The request engine `e` was executing when it was declared hung, as `(engine context, seqno)`: walking its ports in execution
    /// order - the in-flight ones from the active one on, then the pending ones (an ELSP written but never acknowledged) - the oldest
    /// incomplete request of the first port whose context still has one. `None`: no port held a context with an incomplete request.
    /// Read BEFORE the reset, which clears the ports.
    pub(super) fn hung_request(&self, e: usize) -> Option<(u32, u32)> {
        let eng = &self.rigs.get(e)?.engine;
        let x = eng.execlists();
        x.active_ports().chain(x.pending().iter().flatten().copied()).find_map(|port| {
            let hw = eng.hw_seqno(port.ctx).ok()?;
            eng.requests().iter().find(|r| r.ctx == port.ctx && !seqno_passed(hw, r.seqno)).map(|r| (r.ctx, r.seqno))
        })
    }

    /// Settle every request the reset of engine `e` (`scope`) lost: the guilty one (`hung`, or the oldest ungated request on `e` when
    /// no port named one) is charged and fails `-EIO` first; every other one is replayed, or fails `-EIO` by name when it cannot be.
    /// The caller has taken every completion before the reset (`poll`), so no request that already ran is replayed.
    pub(super) fn settle_lost<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, e: usize, scope: ResetScope, hung: Option<(u32, u32)>) {
        let lost = |f: &Inflight| scope == ResetScope::AllEngines || f.engine == e;
        // no port named one: the OLDEST request on `e` that was not waiting on a fence (a gated request never reached the engine)
        let gated = |s: &Self, f: &Inflight| s.gates.iter().any(|g| g.engine == f.engine && g.ectx == f.ectx && g.seqno == f.seqno);
        let guilty = hung.or_else(|| {
            let on_e = || self.inflight.iter().filter(|f| f.engine == e);
            on_e().filter(|f| !gated(self, f)).min_by_key(|f| f.req.0).or_else(|| on_e().min_by_key(|f| f.req.0)).map(|f| (f.ectx, f.seqno))
        });
        // the guilty request first: charged (a ban marks the rest of its GEM context's queue -EIO in gem-core, on every engine, so
        // none of that queue is replayed below) and failed
        if let Some(at) = guilty.and_then(|(c, q)| self.inflight.iter().position(|f| f.engine == e && f.ectx == c && f.seqno == q)) {
            let f = self.inflight.remove(at);
            let _ = self.gem.request_reset(f.engine, f.ectx, f.seqno, true, hw.now_ms);
            self.fail_lost(f);
        }
        // each timeline's requests are replayed oldest first: its ring executes in order (no allocation: an unstable sort)
        self.inflight.sort_unstable_by(timeline_order);
        let mut i = 0;
        while i < self.inflight.len() {
            let f = &self.inflight[i];
            if !lost(f) {
                i += 1;
                continue;
            }
            let (eng, ectx, seqno) = (f.engine, f.ectx, f.seqno);
            let replayed = match (self.gem.request_error(eng, ectx, seqno), f.batch) {
                (Ok(None), Some(batch)) => match self.rigs[eng].build_request(hw.gm, ectx, &batch) {
                    Ok(rq) => self.gem.request_replayed(eng, ectx, seqno, rq.seqno).map(|_| rq.seqno).map_err(|err| err.name()),
                    Err(err) => Err(err.name()),
                },
                (Ok(Some(_)), _) => Err("CONTEXT-BANNED"),
                (Ok(None), None) => Err("BATCH-UNKNOWN"),
                (Err(err), _) => Err(err.name()),
            };
            match replayed {
                Ok(new) => {
                    emit(hw.log, format_args!("[I915-UAPI REPLAY engine={} ectx={} seqno={}->{}]", eng, ectx, seqno, new));
                    self.inflight[i].seqno = new;
                    // its in-fence gate still holds: the new request waits for it as the old one did
                    if let Some(g) = self.gates.iter_mut().find(|g| g.engine == eng && g.ectx == ectx && g.seqno == seqno) {
                        g.seqno = new;
                        if let Err(err) = self.rigs[eng].engine.set_gated(ectx, new, true) {
                            emit(hw.log, format_args!("[I915-UAPI gate failed name={} engine={} ectx={} seqno={}]", err.name(), eng, ectx, new));
                        }
                    }
                    i += 1;
                }
                Err(why) => {
                    emit(hw.log, format_args!("[I915-UAPI REPLAY REFUSED why={} engine={} ectx={} seqno={}]", why, eng, ectx, seqno));
                    let f = self.inflight.remove(i);
                    self.fail_lost(f);
                }
            }
        }
    }

    /// A lost request that is not replayed: gem-core retires it, its fence fails `-EIO`, and its gate (if any) gives back its fences.
    fn fail_lost(&mut self, f: Inflight) {
        let _ = self.gem.observe_seqno(f.engine, f.ectx, f.seqno);
        self.sync.signal_fence(f.fence, -errno::EIO);
        self.sync.put_fence(f.fence);
        if let Some(g) = self.gates.iter().position(|g| g.engine == f.engine && g.ectx == f.ectx && g.seqno == f.seqno) {
            for fence in self.gates.swap_remove(g).fences {
                self.sync.put_fence(fence);
            }
        }
    }
}
