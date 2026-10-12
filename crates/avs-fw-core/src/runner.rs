// SPDX-License-Identifier: GPL-2.0-only
//! A straight-line program of [`Step`]s with Linux's failure policy: the [`Runner`] walks the main program, judges the
//! caller's answer to each step, enforces the poll read bound, and on a failure runs the cleanup Linux runs (nothing, core
//! disable, or CLDMA stop then core disable) and the finalizer, then reports the FIRST failure.
use crate::effect::*;
use crate::error::FwError;
use crate::programs;

/// Steps one program can hold. Every builder in [`programs`] is tested to fit.
pub const PROGRAM_CAP: usize = 40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Program {
    steps: [Step; PROGRAM_CAP],
    len: usize,
    overflow: bool,
}

impl Program {
    pub const fn new() -> Self {
        Self { steps: [Step::NOP; PROGRAM_CAP], len: 0, overflow: false }
    }
    pub fn push(&mut self, s: Step) -> &mut Self {
        if self.len < PROGRAM_CAP {
            self.steps[self.len] = s;
            self.len += 1;
        } else {
            self.overflow = true;
        }
        self
    }
    pub fn extend(&mut self, other: &Program) -> &mut Self {
        for s in other.as_slice() {
            self.push(*s);
        }
        if other.overflow {
            self.overflow = true;
        }
        self
    }
    pub fn as_slice(&self) -> &[Step] {
        &self.steps[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn overflowed(&self) -> bool {
        self.overflow
    }
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}

/// The verdict on one answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    Ok,
    /// A poll question was answered "not yet": ask again.
    Again,
    Fail(Cause),
}

/// Judges the caller's [`Input`] against the [`Step`] that was asked, counting poll reads (the planner's own bound).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Judge {
    pub awaiting: Option<Step>,
    reads: u32,
}

impl Judge {
    pub(crate) const fn new() -> Self {
        Self { awaiting: None, reads: 0 }
    }
    pub(crate) fn ask(&mut self, s: Step) -> Step {
        self.awaiting = Some(s);
        self.reads = 0;
        s
    }
    pub(crate) fn judge(&mut self, input: Input) -> Option<(Step, Verdict)> {
        let step = self.awaiting?;
        let v = match (step.op, input) {
            (Op::Update { .. } | Op::Write { .. } | Op::DelayUs(_) | Op::CopyToDma { .. } | Op::IpcSend(_), Input::Done) => Verdict::Ok,
            (Op::Read { .. }, Input::Value(_)) => Verdict::Ok,
            (Op::Poll(p), Input::Value(v)) => {
                if p.satisfied(v) {
                    Verdict::Ok
                } else {
                    self.reads += 1;
                    if self.reads >= p.max_polls {
                        Verdict::Fail(Cause::PollBound { reads: self.reads, last: v })
                    } else {
                        Verdict::Again
                    }
                }
            }
            (Op::Poll(_), Input::PollTimedOut(last)) => Verdict::Fail(Cause::PollTimeout { last }),
            (Op::Wait { .. }, Input::EventFired) => Verdict::Ok,
            (Op::Wait { .. }, Input::EventTimedOut) => Verdict::Fail(Cause::EventTimeout),
            (Op::IpcAwait { .. }, Input::IpcStatus(0)) => Verdict::Ok,
            (Op::IpcAwait { .. }, Input::IpcStatus(status)) => Verdict::Fail(Cause::IpcRefused { status }),
            (Op::IpcAwait { .. }, Input::EventTimedOut) => Verdict::Fail(Cause::EventTimeout),
            _ => Verdict::Fail(Cause::Protocol),
        };
        if v != Verdict::Again {
            self.awaiting = None;
        }
        Some((step, v))
    }
}

/// `OnFail::Abort`: Linux returns the error with nothing undone.
static NO_CLEANUP: Program = Program::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Main,
    Cleanup(OnFail),
    Final,
    Done,
}

#[derive(Clone, Debug)]
pub struct Runner {
    main: Program,
    fin: Program,
    cleanup_disable: Program,
    cleanup_stop_disable: Program,
    phase: Phase,
    pc: usize,
    judge: Judge,
    failure: Option<Failure>,
    begun: bool,
}

impl Runner {
    /// `main` then `fin`. `fin` runs after the main program finished AND after a failure's cleanup, with every failure in it
    /// tolerated (Linux's `reenable_gating:` label, loader.c:643-646).
    pub fn new(main: Program, fin: Program) -> Result<Self, FwError> {
        let cleanup_disable = programs::core_disable_cleanup();
        let cleanup_stop_disable = programs::stop_then_disable_cleanup();
        if main.overflowed() || fin.overflowed() || cleanup_disable.overflowed() || cleanup_stop_disable.overflowed() {
            return Err(FwError::ProgramOverflow);
        }
        Ok(Self { main, fin, cleanup_disable, cleanup_stop_disable, phase: Phase::Main, pc: 0, judge: Judge::new(), failure: None, begun: false })
    }

    /// A runner that starts already failed with `failure` and runs only the cleanup `kind` names, then `fin`.
    pub fn aborting(failure: Failure, kind: OnFail, fin: Program) -> Result<Self, FwError> {
        let mut r = Self::new(Program::new(), fin)?;
        r.failure = Some(failure);
        r.phase = Phase::Cleanup(kind);
        Ok(r)
    }

    pub fn failure(&self) -> Option<Failure> {
        self.failure
    }

    fn program(&self) -> &Program {
        match self.phase {
            Phase::Main => &self.main,
            Phase::Cleanup(OnFail::AbortStopDisable) => &self.cleanup_stop_disable,
            Phase::Cleanup(OnFail::AbortDisable) => &self.cleanup_disable,
            Phase::Cleanup(OnFail::Abort | OnFail::Tolerate) => &NO_CLEANUP,
            Phase::Final | Phase::Done => &self.fin,
        }
    }

    fn advance(&mut self) -> Output {
        loop {
            if self.pc < self.program().len() {
                let s = self.program().as_slice()[self.pc];
                self.pc += 1;
                return Output::Do(self.judge.ask(s));
            }
            self.pc = 0;
            self.phase = match self.phase {
                Phase::Main | Phase::Cleanup(_) => Phase::Final,
                Phase::Final | Phase::Done => Phase::Done,
            };
            if self.phase == Phase::Done {
                return match self.failure {
                    Some(f) => Output::Failed(f),
                    None => Output::Finished,
                };
            }
        }
    }

    fn fail_now(&mut self, step: &'static str, cause: Cause) -> Output {
        if self.failure.is_none() {
            self.failure = Some(Failure { step, cause });
        }
        self.pc = 0;
        self.phase = Phase::Done;
        Output::Failed(self.failure.unwrap_or(Failure { step, cause }))
    }

    pub fn next(&mut self, input: Input) -> Output {
        if !self.begun {
            if input != Input::Begin {
                return self.fail_now("begin", Cause::Protocol);
            }
            self.begun = true;
            return self.advance();
        }
        if self.phase == Phase::Done {
            return match self.failure {
                Some(f) => Output::Failed(f),
                None => Output::Finished,
            };
        }
        let Some((step, verdict)) = self.judge.judge(input) else {
            return self.fail_now("protocol", Cause::Protocol);
        };
        match verdict {
            Verdict::Ok => self.advance(),
            Verdict::Again => Output::Do(step),
            Verdict::Fail(cause) => {
                if cause == Cause::Protocol {
                    return self.fail_now(step.label, cause);
                }
                match (self.phase, step.on_fail) {
                    (Phase::Main, OnFail::Tolerate) | (Phase::Cleanup(_), _) | (Phase::Final, _) => self.advance(),
                    (Phase::Main, kind) => {
                        if self.failure.is_none() {
                            self.failure = Some(Failure { step: step.label, cause });
                        }
                        self.pc = 0;
                        self.phase = Phase::Cleanup(kind);
                        self.advance()
                    }
                    (Phase::Done, _) => self.advance(),
                }
            }
        }
    }
}
