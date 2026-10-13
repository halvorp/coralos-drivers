// SPDX-License-Identifier: GPL-2.0-only
//! The coroutine that walks a [`Plan`]: it asks for one [`Step`] at a time, judges the `.device`'s answer, enforces every poll bound
//! ITSELF (the caller's clock is not trusted to terminate), and on the first aborting failure stops with a NAMED [`Failure`].
use crate::bus::*;

/// What the `.device` reports for the last [`Output::Do`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Begin,
    /// A write, update, delay or pair-write was performed.
    Done,
    /// The value of the register a `Read`, `Verify` or one question of a `Poll` asked for.
    Value(u8),
    /// The two registers a `ReadPair` asked for (`reg`, `reg + 1`).
    Pair([u8; 2]),
    /// The bus did not acknowledge (or failed) the operation.
    Nak,
}

/// Why a step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The planner's own read bound was reached before the condition held.
    PollBound { reads: u32, last: u8 },
    /// A `Verify` read a value other than the one required.
    VerifyMismatch { reg: u8, mask: u8, want: u8, got: u8 },
    /// The bus failed the operation.
    BusNak,
    /// The `.device` answered something the last question did not ask for.
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub step: &'static str,
    pub cause: Cause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Do(Step),
    Finished,
    Failed(Failure),
}

#[derive(Clone, Copy, Debug)]
pub struct Runner {
    plan: Plan,
    next: usize,
    awaiting: Option<Step>,
    reads: u32,
    tolerated: Option<Failure>,
    last_read: Option<u8>,
    last_pair: Option<[u8; 2]>,
    failed: Option<Failure>,
}

impl Runner {
    /// A plan that overflowed its builder is refused here, before any step is issued.
    pub fn new(plan: Plan) -> Result<Self, crate::PlanError> {
        let plan = plan.checked()?;
        Ok(Self { plan, next: 0, awaiting: None, reads: 0, tolerated: None, last_read: None, last_pair: None, failed: None })
    }

    /// A runner over exactly one step (always fits [`PLAN_CAP`]).
    pub fn single(step: Step) -> Self {
        let mut p = Plan::new();
        p.push(step);
        Self { plan: p, next: 0, awaiting: None, reads: 0, tolerated: None, last_read: None, last_pair: None, failed: None }
    }

    /// The first failure that a step tolerated (`OnFail::Tolerate`), if any.
    pub const fn tolerated(&self) -> Option<Failure> {
        self.tolerated
    }

    /// The value of the last `Read` (or the last read of a `Poll`/`Verify`).
    pub const fn last_read(&self) -> Option<u8> {
        self.last_read
    }

    /// The two registers of the last `ReadPair`.
    pub const fn last_pair(&self) -> Option<[u8; 2]> {
        self.last_pair
    }

    fn fail(&mut self, step: Step, cause: Cause) -> Output {
        let f = Failure { step: step.label, cause };
        match step.on_fail {
            OnFail::Abort => {
                self.failed = Some(f);
                Output::Failed(f)
            }
            OnFail::Tolerate => {
                if self.tolerated.is_none() {
                    self.tolerated = Some(f);
                }
                self.advance()
            }
        }
    }

    fn advance(&mut self) -> Output {
        self.awaiting = None;
        self.reads = 0;
        match self.plan.as_slice().get(self.next) {
            Some(&s) => {
                self.next += 1;
                self.awaiting = Some(s);
                Output::Do(s)
            }
            None => Output::Finished,
        }
    }

    pub fn next(&mut self, input: Input) -> Output {
        if let Some(f) = self.failed {
            return Output::Failed(f);
        }
        let Some(step) = self.awaiting else {
            return match input {
                Input::Begin => self.advance(),
                _ => Output::Failed(Failure { step: "begin", cause: Cause::Protocol }),
            };
        };
        if input == Input::Nak {
            return self.fail(step, Cause::BusNak);
        }
        match (step.op, input) {
            (Op::Write { .. } | Op::Update { .. } | Op::DelayMs(_) | Op::WritePair { .. }, Input::Done) => self.advance(),
            (Op::Read { .. }, Input::Value(v)) => {
                self.last_read = Some(v);
                self.advance()
            }
            (Op::ReadPair { .. }, Input::Pair(v)) => {
                self.last_pair = Some(v);
                self.advance()
            }
            (Op::Verify { reg, mask, want }, Input::Value(v)) => {
                self.last_read = Some(v);
                if v & mask == want {
                    self.advance()
                } else {
                    self.fail(step, Cause::VerifyMismatch { reg, mask, want, got: v & mask })
                }
            }
            (Op::Poll { mask, want, max_polls, .. }, Input::Value(v)) => {
                self.last_read = Some(v);
                if v & mask == want {
                    return self.advance();
                }
                self.reads += 1;
                if self.reads >= max_polls as u32 {
                    let reads = self.reads;
                    self.fail(step, Cause::PollBound { reads, last: v })
                } else {
                    Output::Do(step)
                }
            }
            _ => {
                let f = Failure { step: step.label, cause: Cause::Protocol };
                self.failed = Some(f);
                Output::Failed(f)
            }
        }
    }
}
