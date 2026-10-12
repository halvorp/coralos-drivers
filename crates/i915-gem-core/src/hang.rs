// SPDX-License-Identifier: GPL-2.0-only
//! Hang accounting: which context a reset blames, when it is banned, and what the client's score does. Ported from
//! gt/intel_reset.c (`client_mark_guilty` gt/intel_reset.c:36, `mark_guilty` gt/intel_reset.c:63, `mark_innocent` gt/intel_reset.c:114, `__i915_request_reset` gt/intel_reset.c:125) and
//! gem/i915_gem_context.c (`client_is_banned` gem/i915_gem_context.c:2289, `i915_gem_context_reset_stats_ioctl` gem/i915_gem_context.c:2634), v7.0-rc6.
//!
//! THE BOUNDS. Linux bans a context on its THIRD hang inside `CONTEXT_FAST_HANG_MS` ("3 hangs within 120s? Banned!": the two-slot
//! `hang_timestamp` history finds a recent timestamp in slot 0 only then), or on its first if it is not RECOVERABLE. A client that hangs once per 121 s would never be banned; this port adds
//! [`MAX_GUILTY_BEFORE_BAN`] (`limits.rs`) so the loop is finite. A client whose ban score reaches `CLIENT_SCORE_BANNED` can create
//! no more contexts (`ClientBanned`).
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::SessionId;
use crate::limits::*;

/// What a reset did to the blamed context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetVerdict {
    /// The context is (now) banned: no further submissions (`-EIO`, gem/i915_gem_execbuffer.c:2558-2559).
    pub banned: bool,
    /// Other unretired requests of a newly banned context that were failed with `-EIO`.
    pub cancelled: u32,
}

/// `struct drm_i915_reset_stats` (i915_drm.h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetStats {
    pub reset_count: u32,
    pub batch_active: u32,
    pub batch_pending: u32,
}

impl Gem {
    /// `__i915_request_reset(rq, guilty)` (gt/intel_reset.c:125-): the engine reset found `(engine, engine_ctx, seqno)` guilty or
    /// innocent. `now_ms` is the caller's monotonic clock in milliseconds.
    pub fn request_reset(&mut self, engine: usize, engine_ctx: u32, seqno: u32, guilty: bool, now_ms: u64) -> Result<ResetVerdict, GemError> {
        let ri = self.active.find(engine as u8, engine_ctx, seqno).ok_or(GemError::RequestUnknown { engine, engine_ctx, seqno })?;
        let cslot = self.active.reqs[ri].as_ref().expect("found").ctx as usize;
        if !guilty {
            // mark_innocent (gt/intel_reset.c:114-123) and -EAGAIN (gt/intel_reset.c:138)
            if let Some(r) = self.active.reqs[ri].as_mut() {
                r.error.get_or_insert(crate::error::errno::EAGAIN);
            }
            if let Some(c) = self.contexts[cslot].as_mut() {
                c.active += 1;
            }
            return Ok(ResetVerdict { banned: self.contexts[cslot].is_some_and(|c| c.banned), cancelled: 0 });
        }
        if let Some(r) = self.active.reqs[ri].as_mut() {
            r.error.get_or_insert(crate::error::errno::EIO); // gt/intel_reset.c:134
        }
        let banned = self.mark_guilty(cslot, now_ms);
        let mut cancelled = 0;
        if banned {
            for (i, r) in self.active.reqs.iter_mut().enumerate() {
                if let Some(r) = r {
                    if r.ctx as usize == cslot && i != ri && r.error.is_none() {
                        r.error = Some(crate::error::errno::EIO); // __i915_request_skip of a banned context's queue
                        cancelled += 1;
                    }
                }
            }
        }
        Ok(ResetVerdict { banned, cancelled })
    }

    /// `mark_guilty` (gt/intel_reset.c:63-112). Returns whether the context is banned.
    fn mark_guilty(&mut self, cslot: usize, now_ms: u64) -> bool {
        let Some(mut ctx) = self.contexts[cslot] else { return true };
        if ctx.closed {
            return true; // intel_context_is_closed: gt/intel_reset.c:70-71
        }
        ctx.guilty += 1; // gt/intel_reset.c:81
        let mut banned;
        if !ctx.bannable {
            banned = false; // "cool contexts are too cool to be banned": gt/intel_reset.c:84-87
        } else {
            // Record the timestamp for the last N hangs (gt/intel_reset.c:93-97).
            let prev_hang = ctx.hang_ts[0];
            for i in 0..HANG_HISTORY - 1 {
                ctx.hang_ts[i] = ctx.hang_ts[i + 1];
            }
            ctx.hang_ts[HANG_HISTORY - 1] = Some(now_ms);
            // "If we have hung N+1 times in rapid succession, we ban the context!" (gt/intel_reset.c:99-102)
            banned = !ctx.recoverable;
            if matches!(prev_hang, Some(p) if now_ms < p.saturating_add(CONTEXT_FAST_HANG_MS)) {
                banned = true;
            }
            if ctx.guilty >= MAX_GUILTY_BEFORE_BAN {
                banned = true; // PORT ADDITION, see the module docs
            }
            self.client_mark_guilty(ctx.session as usize, banned, now_ms);
        }
        if banned {
            ctx.banned = true;
        }
        self.contexts[cslot] = Some(ctx);
        banned
    }

    /// `client_mark_guilty` (gt/intel_reset.c:36-61).
    fn client_mark_guilty(&mut self, session_slot: usize, banned: bool, now_ms: u64) {
        let Some(s) = self.sessions[session_slot].as_mut() else { return };
        let mut score = if banned { CLIENT_SCORE_CONTEXT_BAN } else { 0 };
        let prev = s.hang_timestamp.replace(now_ms); // xchg(&file_priv->hang_timestamp, jiffies), gt/intel_reset.c:49
        if matches!(prev, Some(p) if now_ms < p.saturating_add(CLIENT_FAST_HANG_MS)) {
            score += CLIENT_SCORE_HANG_FAST; // gt/intel_reset.c:50-51
        }
        s.ban_score = s.ban_score.saturating_add(score); // gt/intel_reset.c:53-54
    }

    /// A full GPU reset happened (`i915_reset_count(&i915->gpu_error)` advances).
    pub fn note_gpu_reset(&mut self) {
        self.reset_count = self.reset_count.saturating_add(1);
    }

    /// `i915_gem_context_reset_stats_ioctl` (gem/i915_gem_context.c:2634-2670).
    pub fn reset_stats(&self, session: SessionId, ctx_id: u32, flags: u32, pad: u32) -> Result<ResetStats, GemError> {
        if flags != 0 || pad != 0 {
            return Err(GemError::ParamSize { param: 0, size: flags | pad }); // gem/i915_gem_context.c:2642-2643
        }
        let slot = self.session_slot(session)?;
        let c = self.context_slot(slot, ctx_id)?;
        let ctx = self.contexts[c].expect("looked up");
        let reset_count = if self.session(slot).caps.sys_admin { self.reset_count } else { 0 }; // gem/i915_gem_context.c:2656-2658
        Ok(ResetStats { reset_count, batch_active: ctx.guilty, batch_pending: ctx.active }) // gem/i915_gem_context.c:2660-2661
    }

    /// Is the context banned?
    pub fn context_banned(&self, session: SessionId, ctx_id: u32) -> Result<bool, GemError> {
        let slot = self.session_slot(session)?;
        let c = self.context_slot(slot, ctx_id)?;
        Ok(self.contexts[c].expect("looked up").banned)
    }

    /// The client's ban score (`file_priv->ban_score`).
    pub fn ban_score(&self, session: SessionId) -> Result<u32, GemError> {
        Ok(self.session(self.session_slot(session)?).ban_score)
    }
}
