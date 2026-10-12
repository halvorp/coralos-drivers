// SPDX-License-Identifier: GPL-2.0-only
//! Adapters: run each port crate's plan type over the [`Executor`]. They are thin on purpose - the crates' effect types differ only
//! cosmetically (see the contract-gap note in `exec/mod.rs`) and none of them is edited here.
//!
//! | plan type | crate | entry point |
//! |---|---|---|
//! | `Effect{Write,Rmw,Poll,PostingRead,DelayUs}` | i915-gt-core | [`Executor::run_gt_effects`] |
//! | `ResetPlanner` (a coroutine) | i915-gt-core | [`Executor::run_reset`] |
//! | `Step{Mmio,Hwsp}` (`CsbResetPlan`) | i915-submit-core | [`Executor::run_csb_steps`] |
//! | `WritePlan` / `Transaction` / `PcodeOp` | i915-display-clock-core | [`Executor::run_clock_plan`], [`Executor::run_clock_transaction`] |
//! | `Effect{Write,Rmw,ClearIfSet,PostingRead,Poll,DelayUs}` | i915-display-link-core | [`Executor::run_link_effects`] |
//!
//! Each runner takes the forcewake domains its registers need ([`Executor::awake`]) and stops at the FIRST refusal. The display
//! runners exist so the executor is the one place display plans will meet hardware too; no display register is on any
//! increment's allow-list, so today they refuse every write by name (tested).
use super::core::{Executor, DEFAULT_MAX_POLLS};
use super::ExecError;
use coralos_hal::Mmio;
use i915_common_core::Reg;
use i915_display_clock_core::pcode::{AttemptOutcome, PcodeOp, PcodeSession, SessionStep};
use i915_display_clock_core::plan::{PollBudget, PollVerdict, Step as ClockStep, Transaction, WritePlan as ClockPlan};
use i915_gt_core::reset::{Input, Output, ResetPlanner};
use i915_gt_core::{Effect, PollPlan};
use i915_submit_core::execlists::{HwspStore, Step as SubmitStep};

/// Most coroutine steps [`Executor::run_reset`] makes: far above what the planner's own `max_polls` bounds allow for a correct run,
/// so it only fires if the planner stops terminating.
pub const MAX_RESET_STEPS: u32 = 2_000_000;

/// Where `HwspStore`s go: the engine's status page, reached by the driver that owns its grant.
pub trait HwspSink {
    fn store(&mut self, s: HwspStore) -> Result<(), ExecError>;
}

impl<B: Mmio> Executor<B> {
    // ---- i915-gt-core --------------------------------------------------------------------------------------------------------

    /// The registers a gt-core effect list touches.
    pub fn gt_regs(effects: &[Effect]) -> impl Iterator<Item = Reg> + '_ {
        effects.iter().filter_map(|e| match *e {
            Effect::Write(w) => Some(w.reg),
            Effect::Rmw(r) => Some(r.reg),
            Effect::Poll(p) => Some(p.reg),
            Effect::PostingRead(r) => Some(r),
            Effect::DelayUs(_) => None,
        })
    }

    /// Run a gt-core effect list in order, with the forcewake its registers need held around the whole list.
    pub fn run_gt_effects(&mut self, effects: &[Effect]) -> Result<(), ExecError> {
        let d = self.domains_for(Self::gt_regs(effects));
        self.awake(d, |ex| ex.gt_effects_awake(effects))
    }

    /// The same, for a caller that already holds forcewake (the engine driver, whose step spans several lists).
    pub fn gt_effects_awake(&mut self, effects: &[Effect]) -> Result<(), ExecError> {
        for e in effects {
            match *e {
                Effect::Write(w) => self.wr(w.reg, w.value)?,
                Effect::Rmw(p) => {
                    let old = self.rd(p.reg)?;
                    if let Some(v) = p.result(old) {
                        self.wr(p.reg, v)?;
                    }
                }
                Effect::Poll(p) => self.gt_poll(&p)?,
                Effect::PostingRead(r) => self.posting_read(r)?,
                Effect::DelayUs(us) => self.delay_us(us),
            }
        }
        Ok(())
    }

    fn gt_poll(&mut self, p: &PollPlan) -> Result<(), ExecError> {
        self.poll(p.reg, p.mask, p.want, p.timeout_us, p.max_polls, p.what).map(|_| ())
    }

    /// Drive a `ResetPlanner` to completion. The planner is a coroutine (read / write / ONE poll read / delay / done); the executor
    /// answers each output with the real access. A wait is bounded by the planner's `max_polls` (which it counts and enforces) AND by
    /// the plan's microsecond budget, kept here as the armed deadline: when it passes, the planner is told `PollTimedOut` with the
    /// last value read. `forcewake_all` is honoured. The coroutine is also capped at [`MAX_RESET_STEPS`] outputs.
    pub fn run_reset(&mut self, planner: &mut ResetPlanner) -> Result<crate::exec::engine::ResetOutcome, ExecError> {
        let all = if planner.forcewake_all() { self.uncore.domains() } else { 0 };
        self.awake(all, |ex| ex.reset_loop(planner))
    }

    fn reset_loop(&mut self, planner: &mut ResetPlanner) -> Result<crate::exec::engine::ResetOutcome, ExecError> {
        let mut input = Input::Begin;
        let mut waiting: Option<Reg> = None;
        for _ in 0..MAX_RESET_STEPS {
            match planner.step(input)? {
                Output::Read(reg) => {
                    waiting = None;
                    input = Input::Read(self.rd(reg)?);
                }
                Output::Write(w) => {
                    waiting = None;
                    self.wr(w.reg, w.value)?;
                    input = Input::Wrote;
                }
                Output::Poll(p) => {
                    if waiting != Some(p.reg) {
                        self.arm_deadline_us(p.timeout_us);
                        waiting = Some(p.reg);
                    } else {
                        self.relax(); // a repeated poll yields; the first read of a wait does not
                    }
                    let v = self.rd(p.reg)?;
                    input = if !p.satisfied_by(v) && self.deadline_expired() { Input::PollTimedOut(v) } else { Input::Polled(v) };
                }
                Output::DelayUs(us) => {
                    waiting = None;
                    self.delay_us(us);
                    input = Input::Delayed;
                }
                Output::Done(done) => return Ok(crate::exec::engine::ResetOutcome { attempts: done.attempts, proceeded_past_not_ready: done.proceeded_past_not_ready }),
            }
        }
        Err(ExecError::Bound { what: "reset coroutine steps", cap: MAX_RESET_STEPS })
    }

    // ---- i915-submit-core ----------------------------------------------------------------------------------------------------

    /// Run `Step`s in order: `Mmio` effects through the executor, `Hwsp` stores through `sink`. Caller holds forcewake.
    pub fn run_csb_steps(&mut self, steps: &[SubmitStep], sink: &mut impl HwspSink) -> Result<(), ExecError> {
        for s in steps {
            match *s {
                SubmitStep::Mmio(e) => self.gt_effects_awake(&[e])?,
                SubmitStep::Hwsp(h) => sink.store(h)?,
            }
        }
        Ok(())
    }

    // ---- i915-display-clock-core ---------------------------------------------------------------------------------------------

    fn clock_regs(plan: &ClockPlan) -> impl Iterator<Item = Reg> + '_ {
        plan.iter().flat_map(|s| {
            let (a, b) = match *s {
                ClockStep::Rmw { reg, .. } | ClockStep::Write { reg, .. } | ClockStep::PostingRead { reg } => (Some(reg), None),
                ClockStep::Poll(p) => (Some(p.reg), None),
                ClockStep::Pcode(PcodeOp::Request(r)) => (Some(r.attempt_writes()[0].0), Some(r.attempt_writes()[2].0)),
                ClockStep::Pcode(PcodeOp::Write(w)) => (Some(w.attempt_writes()[0].0), Some(w.attempt_writes()[2].0)),
            };
            a.into_iter().chain(b)
        })
    }

    /// Run a display-clock `WritePlan` in order. A poll uses the plan's own `PollBudget` (both bounds); `OnTimeout::Tolerate`
    /// continues, `Fail` stops with the planner's `ClockError`. A pcode op runs its bounded `PcodeSession`.
    pub fn run_clock_plan(&mut self, plan: &ClockPlan) -> Result<(), ExecError> {
        let d = self.domains_for(Self::clock_regs(plan));
        self.awake(d, |ex| ex.clock_plan_awake(plan))
    }

    fn clock_plan_awake(&mut self, plan: &ClockPlan) -> Result<(), ExecError> {
        for s in plan.iter() {
            match *s {
                ClockStep::Rmw { reg, clear, set } => {
                    self.rmw(reg, clear, set)?;
                }
                ClockStep::Write { reg, value } => self.wr(reg, value)?,
                ClockStep::PostingRead { reg } => self.posting_read(reg)?,
                ClockStep::Poll(p) => {
                    let mut budget = PollBudget::new(p);
                    self.arm_deadline_us(p.timeout_us);
                    loop {
                        let v = self.rd(p.reg)?;
                        let elapsed = if self.deadline_expired() { p.timeout_us } else { 0 };
                        match budget.observe(v, elapsed) {
                            PollVerdict::Met { .. } | PollVerdict::Tolerated(_) => break,
                            PollVerdict::Pending => self.relax(),
                            PollVerdict::Failed(e) => return Err(ExecError::Clock(e)),
                        }
                    }
                }
                ClockStep::Pcode(op) => self.pcode(op)?,
            }
        }
        Ok(())
    }

    /// A forward plan, and on ITS failure the rollback (best effort: a rollback error does not replace the original). Returns the
    /// forward error, if any.
    pub fn run_clock_transaction(&mut self, tx: &Transaction) -> Result<(), ExecError> {
        match self.run_clock_plan(&tx.forward) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = self.run_clock_plan(&tx.rollback);
                Err(e)
            }
        }
    }

    /// `intel_pcode_request` / `intel_pcode_write`: send, wait for READY to clear, read the reply, let the session decide (resend,
    /// acknowledge or fail) - bounded by the session's `max_attempts` and its deadline.
    fn pcode(&mut self, op: PcodeOp) -> Result<(), ExecError> {
        let (writes, poll) = match op {
            PcodeOp::Request(r) => (r.attempt_writes(), r.ready_poll()),
            PcodeOp::Write(w) => (w.attempt_writes(), w.ready_poll()),
        };
        let (data_reg, mailbox_reg) = (writes[0].0, writes[2].0);
        let mut session = match op {
            PcodeOp::Request(r) => Some(PcodeSession::new(r)),
            PcodeOp::Write(_) => None,
        };
        // This executor's clock is whole milliseconds and every pass sleeps at least one (the poll's `relax` / the resend's `relax`), so
        // a pass is charged 1000 us against the session's deadline; the session ALSO caps resends at `max_attempts`.
        let mut elapsed = 0u32;
        loop {
            elapsed = elapsed.saturating_add(1000);
            let busy = self.rd(mailbox_reg)?;
            let outcome = if busy & poll.mask != 0 {
                AttemptOutcome::Busy { mailbox: busy }
            } else {
                for (reg, v) in writes {
                    self.wr(reg, v)?;
                }
                match self.poll(mailbox_reg, poll.mask, poll.want, poll.timeout_us, poll.max_polls, "PCODE mailbox ready") {
                    Ok(mailbox) => {
                        let data = self.rd(data_reg)?;
                        if let PcodeOp::Write(w) = op {
                            return w.judge(mailbox).map_err(ExecError::Clock);
                        }
                        AttemptOutcome::Replied { mailbox, data }
                    }
                    Err(ExecError::PollTimeout { got, .. }) => AttemptOutcome::MailboxTimeout { got },
                    Err(e) => return Err(e),
                }
            };
            let Some(s) = session.as_mut() else {
                // a plain write that found the mailbox busy or never ready: one attempt, no resend (`intel_pcode_write`)
                return Err(ExecError::PollTimeout { what: "PCODE mailbox ready", reg: mailbox_reg, mask: poll.mask, want: poll.want, got: 0, polls: 1 });
            };
            match s.after_attempt(outcome, elapsed) {
                SessionStep::Acknowledged { .. } => return Ok(()),
                SessionStep::Resend => self.relax(),
                SessionStep::Fail(e) => return Err(ExecError::Clock(e)),
            }
        }
    }
}

/// `DEFAULT_MAX_POLLS` re-exported for callers that build a gt-core `PollPlan` of their own.
pub const POLLS: u32 = DEFAULT_MAX_POLLS;
