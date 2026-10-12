// SPDX-License-Identifier: GPL-2.0-only
//! The effect-plan vocabulary: [`WritePlan`] (ordered register effects), [`PollPlan`] (a bounded wait), [`Transaction`]
//! (a forward plan with its matching rollback).
//!
//! Nothing here touches a register. A planner returns a [`Transaction`]; `devices/i915` is the only crate that EXECUTES one,
//! through its `WriteGuard`, and it drives each [`PollPlan`] with a [`PollBudget`] so the bound is enforced in ONE place.
//!
//! Linux does these as inline `intel_de_rmw` / `intel_de_wait_for_set_ms` calls; the translation keeps each call as one step
//! in the same order (the order is the protocol) and cites the Linux line on the planner that emits it.
use crate::error::{Bound, ClockError, Handshake};
use crate::pcode::PcodeOp;
use i915_common_core::Reg;

/// KEPT FIXED (D10): a plan is a HARDWARE SEQUENCE whose longest form is known (15 steps); it holds no client-driven population.
/// Capacity of one plan. The longest planned sequence (a CDCLK change that also moves the VCO) is 15 steps; 32 leaves room
/// and a longer plan is refused with [`ClockError::PlanFull`], never truncated.
pub const PLAN_CAP: usize = 32;

/// Default sample budget of one poll. 5 ms (the longest planned wait) of BAR0 reads cannot be more than a few thousand
/// samples; this bound exists for a host whose clock does not advance (forcewake's `max_polls` lesson, 2026-10-12).
pub const DEFAULT_MAX_POLLS: u32 = 100_000;

/// One register effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// `intel_de_rmw(reg, clear, set)`: `new = (old & !clear) | set`.
    Rmw { reg: Reg, clear: u32, set: u32 },
    /// `intel_de_write(reg, value)`.
    Write { reg: Reg, value: u32 },
    /// `intel_de_posting_read(reg)`: a read whose only purpose is to flush the preceding write.
    PostingRead { reg: Reg },
    /// `intel_de_wait_for_set_ms` / `_clear_ms`.
    Poll(PollPlan),
    /// The PCU mailbox: a request-until-acknowledged or a plain write (see `crate::pcode`).
    Pcode(PcodeOp),
}

/// What a poll that ran out of budget means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnTimeout {
    /// Abort the transaction (the executor then runs the rollback).
    Fail,
    /// Linux only logs it (`hsw_wait_for_power_well_disable`); the sequence continues.
    Tolerate,
}

/// A bounded wait for `(read(reg) & mask) == want`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollPlan {
    pub what: Handshake,
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
    /// Wall-clock bound, microseconds (Linux passes milliseconds; 0 means one sample).
    pub timeout_us: u32,
    /// Sample bound. ENFORCED by [`PollBudget`]; see [`DEFAULT_MAX_POLLS`].
    pub max_polls: u32,
    pub on_timeout: OnTimeout,
}

impl PollPlan {
    /// `intel_de_wait_for_set_ms(reg, bits, ms)`.
    pub const fn wait_set(what: Handshake, reg: Reg, bits: u32, timeout_ms: u32, on_timeout: OnTimeout) -> PollPlan {
        PollPlan { what, reg, mask: bits, want: bits, timeout_us: timeout_ms * 1000, max_polls: DEFAULT_MAX_POLLS, on_timeout }
    }
    /// `intel_de_wait_for_clear_ms(reg, bits, ms)`.
    pub const fn wait_clear(what: Handshake, reg: Reg, bits: u32, timeout_ms: u32, on_timeout: OnTimeout) -> PollPlan {
        PollPlan { what, reg, mask: bits, want: 0, timeout_us: timeout_ms * 1000, max_polls: DEFAULT_MAX_POLLS, on_timeout }
    }
    pub const fn with_max_polls(mut self, max_polls: u32) -> PollPlan {
        self.max_polls = max_polls;
        self
    }
    pub const fn satisfied(&self, value: u32) -> bool {
        value & self.mask == self.want
    }
}

/// The verdict after one sample.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollVerdict {
    /// The condition held on this sample.
    Met { polls: u32 },
    /// Not yet, and both budgets have room: sample again.
    Pending,
    /// Out of budget under [`OnTimeout::Tolerate`]: carry on, but the error is the record.
    Tolerated(ClockError),
    /// Out of budget under [`OnTimeout::Fail`].
    Failed(ClockError),
}

/// The executor's side of a [`PollPlan`]: counts samples and checks BOTH bounds.
#[derive(Clone, Copy, Debug)]
pub struct PollBudget {
    plan: PollPlan,
    polls: u32,
}

impl PollBudget {
    pub const fn new(plan: PollPlan) -> PollBudget {
        PollBudget { plan, polls: 0 }
    }
    pub const fn polls(&self) -> u32 {
        self.polls
    }
    /// Feed one sample of the register and the time elapsed since the poll began. The condition is checked BEFORE either
    /// bound, as `__wait_for` does ("Guarantee COND check prior to timeout", i915_wait_util.h:13-19, :28).
    pub fn observe(&mut self, value: u32, elapsed_us: u32) -> PollVerdict {
        self.polls = self.polls.saturating_add(1);
        if self.plan.satisfied(value) {
            return PollVerdict::Met { polls: self.polls };
        }
        let bound = if self.polls >= self.plan.max_polls {
            Bound::Polls
        } else if elapsed_us >= self.plan.timeout_us {
            Bound::Time
        } else {
            return PollVerdict::Pending;
        };
        let err = ClockError::HandshakeTimeout {
            what: self.plan.what,
            bound,
            reg: self.plan.reg,
            mask: self.plan.mask,
            want: self.plan.want,
            got: value & self.plan.mask,
            polls: self.polls,
            elapsed_us,
        };
        match self.plan.on_timeout {
            OnTimeout::Fail => PollVerdict::Failed(err),
            OnTimeout::Tolerate => PollVerdict::Tolerated(err),
        }
    }
}

/// An ordered list of [`Step`]s of fixed capacity.
#[derive(Clone, Copy, Debug)]
pub struct WritePlan {
    steps: [Option<Step>; PLAN_CAP],
    len: usize,
}

impl WritePlan {
    pub const fn new() -> WritePlan {
        WritePlan { steps: [None; PLAN_CAP], len: 0 }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, s: Step) -> Result<(), ClockError> {
        if self.len >= PLAN_CAP {
            return Err(ClockError::PlanFull { cap: PLAN_CAP });
        }
        self.steps[self.len] = Some(s);
        self.len += 1;
        Ok(())
    }
    /// Append every step of `other`, in order.
    pub fn extend(&mut self, other: &WritePlan) -> Result<(), ClockError> {
        for s in other.iter() {
            self.push(*s)?;
        }
        Ok(())
    }
    pub fn iter(&self) -> impl Iterator<Item = &Step> {
        self.steps[..self.len].iter().flatten()
    }
    pub fn get(&self, i: usize) -> Option<&Step> {
        self.steps[..self.len].get(i).and_then(|s| s.as_ref())
    }
    // Convenience builders; each is one Linux call.
    pub fn rmw(&mut self, reg: u32, clear: u32, set: u32) -> Result<(), ClockError> {
        self.push(Step::Rmw { reg: Reg::new(reg), clear, set })
    }
    pub fn write(&mut self, reg: u32, value: u32) -> Result<(), ClockError> {
        self.push(Step::Write { reg: Reg::new(reg), value })
    }
    pub fn posting_read(&mut self, reg: u32) -> Result<(), ClockError> {
        self.push(Step::PostingRead { reg: Reg::new(reg) })
    }
    pub fn poll(&mut self, p: PollPlan) -> Result<(), ClockError> {
        self.push(Step::Poll(p))
    }
    pub fn pcode(&mut self, op: PcodeOp) -> Result<(), ClockError> {
        self.push(Step::Pcode(op))
    }
}

impl Default for WritePlan {
    fn default() -> Self {
        WritePlan::new()
    }
}

impl PartialEq for WritePlan {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.iter().zip(other.iter()).all(|(a, b)| a == b)
    }
}
impl Eq for WritePlan {}

/// A forward plan and the plan that undoes it.
///
/// `rollback` is meant to be run when the forward plan FAILED part-way (a poll timed out): it does not assume the forward plan
/// completed, so each rollback step is safe to run whether or not its forward counterpart ran. It is built from the register
/// snapshot the planner was given, so it restores what was there, not what this crate guesses was there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Transaction {
    pub forward: WritePlan,
    pub rollback: WritePlan,
}

impl Transaction {
    pub const fn new() -> Transaction {
        Transaction { forward: WritePlan::new(), rollback: WritePlan::new() }
    }
    /// Run `next` AFTER this transaction; its rollback runs BEFORE this one's (reverse order).
    pub fn then(&mut self, next: &Transaction) -> Result<(), ClockError> {
        let mut rb = next.rollback;
        rb.extend(&self.rollback)?;
        let mut fwd = self.forward;
        fwd.extend(&next.forward)?;
        self.forward = fwd;
        self.rollback = rb;
        Ok(())
    }
}

impl Default for Transaction {
    fn default() -> Self {
        Transaction::new()
    }
}
