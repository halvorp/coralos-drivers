// SPDX-License-Identifier: GPL-2.0-only
//! GPU reset for Gen8/Gen9 as a state machine (gt/intel_reset.c): the per-engine RESET_CTL ready-to-reset handshake
//! (`gen8_engine_reset_prepare`, :560-599), the `GEN6_GDRST` domain reset with its double write and settle delay
//! (`gen6_hw_domain_reset`, :266-316), the withdrawal of the request (`gen8_engine_reset_cancel`, :601-606), and the retry
//! policy around them (`gen8_reset_engines`, :608-672; `__intel_gt_reset`, :764-809).
//!
//! THE PLANNER DOES NOT TOUCH THE DEVICE. [`ResetPlanner::step`] is a coroutine: the caller (devices/i915) feeds it the result of
//! the previous action and it answers with the next [`Output`] - a read, a write, ONE poll read, a delay, or `Done`. Every wait
//! is bounded TWICE: by Linux's microsecond budget (carried in the [`PollPlan`] for the caller's clock, and reported back with
//! [`Input::PollTimedOut`]) and by `max_polls` reads, which THIS state machine counts and enforces - a caller with no clock, or a
//! register that never changes, still terminates with a NAMED [`GtError::ResetTimeout`]. (2026-10-12: forcewake's `max_polls`
//! was declared and never read; a test consumed 31 GB. The bound here has a test that fails when the check is removed.)
use crate::effect::{PollPlan, WritePlan};
use crate::engine::EngineInfo;
use crate::error::{GtError, ResetStage};
use crate::fixed::FixedVec;
use i915_common_core::reg::{masked_bit_disable, masked_bit_enable};
use i915_common_core::Reg;
use i915_gt_regs_core as r;

/// `RESET_MAX_RETRIES` (intel_reset.c:34): full-GT resets retry this many times; a single-engine reset gets one attempt
/// (`retries = engine_mask == ALL_ENGINES ? RESET_MAX_RETRIES : 1`, :766).
pub const RESET_MAX_RETRIES: u32 = 3; // intel_reset.c:34
/// The ready-to-reset wait: `__intel_wait_for_register_fw(uncore, reg, mask, ack, 700, 0, NULL)` (intel_reset.c:590-591).
pub const ENGINE_RESET_TIMEOUT_US: u32 = 700; // intel_reset.c:591
/// The GDRST ack wait: `__intel_wait_for_register_fw(uncore, GEN6_GDRST, mask, 0, 2000, 0, NULL)` (intel_reset.c:299-301).
pub const GDRST_ACK_TIMEOUT_US: u32 = 2000; // intel_reset.c:301
/// `udelay(50)` after the domain reset acks (intel_reset.c:313): the engine state is still volatile for a moment.
pub const GDRST_SETTLE_US: u32 = 50; // intel_reset.c:313
/// `loops = GRAPHICS_VER_FULL < IP_VER(12, 70) ? 2 : 1` (intel_reset.c:288): the reset is issued twice on every pre-MTL part.
pub const GDRST_LOOPS: u32 = 2; // intel_reset.c:288
/// The default read cap per wait: far above what 2 ms of BAR0 reads can be, so on silicon the microsecond budget fires first.
pub const DEFAULT_MAX_POLLS: u32 = 100_000;

/// What gets reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetScope {
    /// `ALL_ENGINES`: `GEN6_GRDOM_FULL`, up to [`RESET_MAX_RETRIES`] attempts.
    AllEngines,
    /// The listed engines only: the OR of their `reset_domain`s, one attempt.
    Engines,
}

/// What the caller reports back to the planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// First call.
    Begin,
    /// The value of the register the last [`Output::Read`] asked for.
    Read(u32),
    /// The last [`Output::Write`] was performed.
    Wrote,
    /// One read of the polled register (answering [`Output::Poll`]).
    Polled(u32),
    /// The caller's microsecond budget for the wait ran out; this was the last value it read.
    PollTimedOut(u32),
    /// The last [`Output::DelayUs`] elapsed.
    Delayed,
}

/// What the caller must do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// Read this register and answer [`Input::Read`].
    Read(Reg),
    /// Perform this write and answer [`Input::Wrote`].
    Write(WritePlan),
    /// Read `reg` ONCE and answer [`Input::Polled`] (or [`Input::PollTimedOut`] when `timeout_us` has elapsed); the planner asks
    /// again until the wait is satisfied or `max_polls` reads are spent.
    Poll(PollPlan),
    /// Wait this long and answer [`Input::Delayed`].
    DelayUs(u32),
    /// Finished.
    Done(ResetDone),
}

/// A successful reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetDone {
    /// How many attempts ran (1 = first try).
    pub attempts: u32,
    /// Some engine never became ready-to-reset on a retry and the reset went ahead anyway
    /// ("we rather take context corruption instead of failed reset", intel_reset.c:625-636).
    pub proceeded_past_not_ready: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PrepKind {
    /// RESET_CTL showed CAT_ERROR: set CAT_ERROR and wait for hardware to clear it (intel_reset.c:567-575).
    CatError,
    /// Ordinary: set REQUEST_RESET and wait for READY_TO_RESET (intel_reset.c:576-584).
    Request,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Init,
    PrepRead { i: usize },
    PrepWrite { i: usize, kind: PrepKind },
    PrepPoll { i: usize, kind: PrepKind, polls: u32 },
    GdrstWrite { pass: u32 },
    GdrstPoll { pass: u32, polls: u32 },
    Settle,
    Cancel { i: usize },
    Finished,
}

/// The reset state machine.
#[derive(Clone, Debug)]
pub struct ResetPlanner {
    engines: FixedVec<EngineInfo, 5>,
    scope: ResetScope,
    max_polls: u32,
    retries: u32,
    attempt: u32,
    state: State,
    /// The failure that ends the current attempt, once the cancel writes are done.
    pending: Option<GtError>,
    proceeded_past_not_ready: bool,
    hw_mask: u32,
}

impl ResetPlanner {
    /// `engines` is every engine of the GT (`for_each_engine_masked(engine, gt, engine_mask)` with `ALL_ENGINES`) or the engines
    /// being reset. `max_polls` bounds every wait; zero is refused (a wait that may not read can never succeed).
    pub fn new(scope: ResetScope, engines: &[EngineInfo], max_polls: u32) -> Result<ResetPlanner, GtError> {
        if max_polls == 0 {
            return Err(GtError::ZeroBound { what: "reset max_polls" });
        }
        if engines.is_empty() {
            return Err(GtError::NoSuchEngine { id: 0 });
        }
        let mut v = FixedVec::new("reset engines", engines[0]);
        let mut hw_mask = 0;
        for e in engines {
            v.push(*e)?;
            hw_mask |= e.reset_domain; // intel_reset.c:332
        }
        if scope == ResetScope::AllEngines {
            hw_mask = r::GEN6_GRDOM_FULL; // intel_reset.c:326
        }
        let retries = if scope == ResetScope::AllEngines { RESET_MAX_RETRIES } else { 1 }; // intel_reset.c:766
        Ok(ResetPlanner { engines: v, scope, max_polls, retries, attempt: 0, state: State::Init, pending: None, proceeded_past_not_ready: false, hw_mask })
    }

    /// The `GEN6_GDRST` mask this reset writes.
    pub fn hw_mask(&self) -> u32 {
        self.hw_mask
    }
    /// The forcewake domains held around the whole sequence (`intel_uncore_forcewake_get(FORCEWAKE_ALL)`, intel_reset.c:779).
    pub const fn forcewake_all(&self) -> bool {
        true
    }
    pub fn scope(&self) -> ResetScope {
        self.scope
    }
    pub fn attempts_allowed(&self) -> u32 {
        self.retries
    }

    fn out_of_order(expected: &'static str) -> GtError {
        GtError::ResetOutOfOrder { expected }
    }

    /// `reset_non_ready = retry >= 1` (intel_reset.c:613).
    fn reset_non_ready(&self) -> bool {
        self.attempt >= 1
    }

    fn engine_poll(&self, i: usize, kind: PrepKind) -> PollPlan {
        let reg = r::ring_reset_ctl(self.engines.as_slice()[i].mmio_base);
        let (mask, want) = match kind {
            PrepKind::CatError => (r::RESET_CTL_CAT_ERROR, 0),                              // ack = 0 (:580)
            PrepKind::Request => (r::RESET_CTL_READY_TO_RESET, r::RESET_CTL_READY_TO_RESET), // :583-584
        };
        PollPlan { reg, mask, want, timeout_us: ENGINE_RESET_TIMEOUT_US, max_polls: self.max_polls, what: "RESET_CTL ready-to-reset" }
    }

    fn gdrst_poll(&self) -> PollPlan {
        PollPlan { reg: r::GEN6_GDRST, mask: self.hw_mask, want: 0, timeout_us: GDRST_ACK_TIMEOUT_US, max_polls: self.max_polls, what: "GDRST domain ack" }
    }

    /// Begin cancelling: the REQUEST_RESET bit of every engine is withdrawn (`skip_reset:` label, intel_reset.c:660).
    fn begin_cancel(&mut self) -> Result<Output, GtError> {
        self.state = State::Cancel { i: 0 };
        self.cancel_step(0)
    }

    fn cancel_step(&mut self, i: usize) -> Result<Output, GtError> {
        if i < self.engines.len() {
            let reg = r::ring_reset_ctl(self.engines.as_slice()[i].mmio_base);
            self.state = State::Cancel { i: i + 1 };
            return Ok(Output::Write(WritePlan { reg, value: masked_bit_disable(r::RESET_CTL_REQUEST_RESET) })); // :603-605
        }
        // The attempt is over: retry, fail, or succeed.
        match self.pending.take() {
            Some(e) => {
                if self.attempt + 1 < self.retries {
                    self.attempt += 1;
                    self.state = State::Init;
                    self.start_attempt()
                } else {
                    self.state = State::Finished;
                    Err(e)
                }
            }
            None => {
                self.state = State::Finished;
                Ok(Output::Done(ResetDone { attempts: self.attempt + 1, proceeded_past_not_ready: self.proceeded_past_not_ready }))
            }
        }
    }

    fn start_attempt(&mut self) -> Result<Output, GtError> {
        self.state = State::PrepRead { i: 0 };
        self.prep_read(0)
    }

    fn prep_read(&mut self, i: usize) -> Result<Output, GtError> {
        if i < self.engines.len() {
            self.state = State::PrepRead { i };
            return Ok(Output::Read(r::ring_reset_ctl(self.engines.as_slice()[i].mmio_base)));
        }
        // every engine prepared (or forced past): the domain reset
        self.state = State::GdrstWrite { pass: 0 };
        Ok(Output::Write(WritePlan { reg: r::GEN6_GDRST, value: self.hw_mask })) // :296
    }

    /// A wait on engine `i` ran out. Linux: `if (ret && !reset_non_ready) goto skip_reset;` - otherwise carry on regardless.
    fn engine_wait_failed(&mut self, i: usize, kind: PrepKind, got: u32, polls: u32) -> Result<Output, GtError> {
        let p = self.engine_poll(i, kind);
        if self.reset_non_ready() {
            self.proceeded_past_not_ready = true;
            return self.prep_read(i + 1);
        }
        self.pending = Some(GtError::ResetTimeout { stage: ResetStage::EngineReadyToReset, reg: p.reg, mask: p.mask, want: p.want, got, polls });
        self.begin_cancel()
    }

    /// Drive the machine. See the module docs for the protocol; an input that does not answer the pending output is
    /// [`GtError::ResetOutOfOrder`] and ends the sequence.
    pub fn step(&mut self, input: Input) -> Result<Output, GtError> {
        match (self.state, input) {
            (State::Init, Input::Begin) => self.start_attempt(),
            (State::PrepRead { i }, Input::Read(ack)) => {
                if ack & r::RESET_CTL_CAT_ERROR != 0 {
                    // Catastrophic errors bypass the ready-for-reset sequence (HAS#396813, intel_reset.c:571-575).
                    self.state = State::PrepWrite { i, kind: PrepKind::CatError };
                    let reg = r::ring_reset_ctl(self.engines.as_slice()[i].mmio_base);
                    Ok(Output::Write(WritePlan { reg, value: masked_bit_enable(r::RESET_CTL_CAT_ERROR) }))
                } else if ack & r::RESET_CTL_READY_TO_RESET == 0 {
                    self.state = State::PrepWrite { i, kind: PrepKind::Request };
                    let reg = r::ring_reset_ctl(self.engines.as_slice()[i].mmio_base);
                    Ok(Output::Write(WritePlan { reg, value: masked_bit_enable(r::RESET_CTL_REQUEST_RESET) }))
                } else {
                    self.prep_read(i + 1) // already ready: `return 0` (:585-586)
                }
            }
            (State::PrepWrite { i, kind }, Input::Wrote) => {
                self.state = State::PrepPoll { i, kind, polls: 0 };
                Ok(Output::Poll(self.engine_poll(i, kind)))
            }
            (State::PrepPoll { i, kind, polls }, Input::Polled(v)) => {
                let polls = polls + 1;
                let p = self.engine_poll(i, kind);
                if p.satisfied_by(v) {
                    self.prep_read(i + 1)
                } else if polls >= self.max_polls {
                    self.engine_wait_failed(i, kind, v, polls)
                } else {
                    self.state = State::PrepPoll { i, kind, polls };
                    Ok(Output::Poll(p))
                }
            }
            (State::PrepPoll { i, kind, polls }, Input::PollTimedOut(v)) => self.engine_wait_failed(i, kind, v, polls),
            (State::GdrstWrite { pass }, Input::Wrote) => {
                self.state = State::GdrstPoll { pass, polls: 0 };
                Ok(Output::Poll(self.gdrst_poll()))
            }
            (State::GdrstPoll { pass, polls }, Input::Polled(v)) => {
                let polls = polls + 1;
                let p = self.gdrst_poll();
                if p.satisfied_by(v) {
                    if pass + 1 < GDRST_LOOPS {
                        self.state = State::GdrstWrite { pass: pass + 1 };
                        Ok(Output::Write(WritePlan { reg: r::GEN6_GDRST, value: self.hw_mask }))
                    } else {
                        self.state = State::Settle;
                        Ok(Output::DelayUs(GDRST_SETTLE_US))
                    }
                } else if polls >= self.max_polls {
                    self.gdrst_failed(v, polls)
                } else {
                    self.state = State::GdrstPoll { pass, polls };
                    Ok(Output::Poll(p))
                }
            }
            (State::GdrstPoll { polls, .. }, Input::PollTimedOut(v)) => self.gdrst_failed(v, polls),
            (State::Settle, Input::Delayed) => self.begin_cancel(),
            (State::Cancel { i }, Input::Wrote) => self.cancel_step(i),
            (State::Init, _) => Err(Self::out_of_order("Begin")),
            (State::PrepRead { .. }, _) => Err(Self::out_of_order("Read")),
            (State::PrepWrite { .. }, _) | (State::GdrstWrite { .. }, _) | (State::Cancel { .. }, _) => Err(Self::out_of_order("Wrote")),
            (State::PrepPoll { .. }, _) | (State::GdrstPoll { .. }, _) => Err(Self::out_of_order("Polled or PollTimedOut")),
            (State::Settle, _) => Err(Self::out_of_order("Delayed")),
            (State::Finished, _) => Err(Self::out_of_order("nothing: the reset is over")),
        }
    }

    /// The GDRST ack never came: Linux still takes the settle delay (`udelay(50)` follows the loop whether or not it failed,
    /// intel_reset.c:313) and then cancels.
    fn gdrst_failed(&mut self, got: u32, polls: u32) -> Result<Output, GtError> {
        let p = self.gdrst_poll();
        self.pending = Some(GtError::ResetTimeout { stage: ResetStage::GdrstAck, reg: p.reg, mask: p.mask, want: p.want, got, polls });
        self.state = State::Settle;
        Ok(Output::DelayUs(GDRST_SETTLE_US))
    }
}
