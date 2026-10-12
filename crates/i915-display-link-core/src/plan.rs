// SPDX-License-Identifier: GPL-2.0-only
//! The effect vocabulary every planner in this crate speaks. A planner never touches a register: it returns
//! [`Effect`]s, `devices/i915` (the only crate that executes effects) applies them, and anything it READ is fed back
//! into the planner's next step.
//!
//! Each [`Effect`] is the direct image of one Linux display accessor:
//!  * `Write`       = `intel_de_write()` (intel_de.h)
//!  * `Rmw`         = `intel_de_rmw(display, reg, clear, set)` (intel_de.h)
//!  * `PostingRead` = `intel_de_posting_read()` (intel_de.h)
//!  * `Poll`        = `intel_de_wait_for_{set,clear}_ms()` / `poll_timeout_us()` — BOUNDED by [`PollPlan::max_polls`]
//!  * `DelayUs`     = `udelay()` / `usleep_range()` / `msleep()` / `fsleep()`
//!
//! The one bounded-wait rule (WAVE2-BRIEF "every bound you declare MUST be enforced"): a poll is only ever judged through
//! [`PollPlan::judge`], which stops the wait at `max_polls` reads whatever the clock is doing. A host with no usable
//! clock therefore still terminates.
use alloc::vec::Vec;
use i915_common_core::Reg;

/// EXCEPTION to NO-POLL-BLOCK-ON-WAKE (named, bounded): these are register waits for which the hardware raises NO wakeup the
/// task could `recv` on (panel power sequencing: Linux's `intel_de_wait_*` / `poll_timeout_us` sleeps and re-reads; AUX
/// SEND_BUSY: Linux's interrupt is not wired here). The executor MUST sleep or `yield_now()` between reads and MUST NEVER
/// `spin_loop()` -- `delay_us` is that sleep -- and every wait is capped by `max_polls`. *delete-when:* the AUX/GMBUS
/// interrupt is routed to `i915.device` (IRQ contract), at which point the AUX wait becomes a block on that wakeup.
///
/// One bounded register wait: read `reg` until `value & mask == want`, at most `max_polls` reads, `interval_us` apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollPlan {
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
    pub interval_us: u32,
    /// Hard cap on reads. `poll_timeout_us(op, cond, sleep_us, timeout_us)` reads once, then once per `sleep_us` until
    /// `timeout_us` has passed: `timeout_us / sleep_us + 1` reads.
    pub max_polls: u32,
    /// What the wait is for, printed in the timeout (Linux's `drm_err()` text).
    pub what: &'static str,
}

/// The verdict of one read of a [`PollPlan`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollStep {
    /// The condition holds.
    Satisfied,
    /// Not yet; wait `interval_us` and read again.
    Again,
    /// `max_polls` reads were made and the condition never held.
    TimedOut(PollTimeout),
}

/// A bounded wait ran out. Names the wait, the register and the last value read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollTimeout {
    pub what: &'static str,
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
    pub got: u32,
    pub polls: u32,
}

impl PollPlan {
    /// A wait of `timeout_us` polled every `interval_us` (`poll_timeout_us`, linux/iopoll.h semantics). An `interval_us`
    /// of 0 would make the cap infinite, so it is treated as 1.
    pub const fn with_timeout(reg: Reg, mask: u32, want: u32, interval_us: u32, timeout_us: u32, what: &'static str) -> PollPlan {
        let interval_us = if interval_us == 0 { 1 } else { interval_us };
        PollPlan { reg, mask, want, interval_us, max_polls: timeout_us / interval_us + 1, what }
    }

    /// Judge the `polls_done`-th read (1-based) that returned `value`.
    pub fn judge(&self, polls_done: u32, value: u32) -> PollStep {
        if value & self.mask == self.want {
            PollStep::Satisfied
        } else if polls_done >= self.max_polls {
            PollStep::TimedOut(PollTimeout { what: self.what, reg: self.reg, mask: self.mask, want: self.want, got: value, polls: polls_done })
        } else {
            PollStep::Again
        }
    }
}

impl PollPlan {
    /// The reference executor loop: read until [`PollPlan::judge`] says stop. `read` returns the register, `delay_us`
    /// sleeps or yields (never spins). Returns the satisfying value, or the timeout. Reads are capped at `max_polls` whatever `delay_us` does.
    pub fn run(&self, mut read: impl FnMut() -> u32, mut delay_us: impl FnMut(u32)) -> Result<u32, PollTimeout> {
        let mut polls = 0u32;
        loop {
            let v = read();
            polls += 1;
            match self.judge(polls, v) {
                PollStep::Satisfied => return Ok(v),
                PollStep::Again => delay_us(self.interval_us),
                PollStep::TimedOut(t) => return Err(t),
            }
        }
    }
}

/// One thing the executor must do, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Write { reg: Reg, value: u32 },
    /// Read, clear `clear`, set `set`, write back.
    Rmw { reg: Reg, clear: u32, set: u32 },
    /// Read; if any of `bits` is set write back with them cleared, otherwise write nothing
    /// (`tmp = read; if (tmp & BIT) write(tmp & ~BIT)`, intel_backlight.c:358-363).
    ClearIfSet { reg: Reg, bits: u32 },
    PostingRead { reg: Reg },
    Poll(PollPlan),
    DelayUs(u32),
}

/// An ordered list of effects.
pub type Plan = Vec<Effect>;

/// The registers a plan WRITES (Write, Rmw, ClearIfSet), in order, for the device's write allow-list check.
pub fn written_regs(plan: &[Effect]) -> Vec<Reg> {
    plan.iter()
        .filter_map(|e| match *e {
            Effect::Write { reg, .. } | Effect::Rmw { reg, .. } | Effect::ClearIfSet { reg, .. } => Some(reg),
            _ => None,
        })
        .collect()
}
