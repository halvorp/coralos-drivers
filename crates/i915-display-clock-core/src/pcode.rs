// SPDX-License-Identifier: GPL-2.0-only
//! The PCU mailbox handshake (`GEN6_PCODE_MAILBOX`) as a planner with BOUNDED retries.
//!
//! Ported from Linux drivers/gpu/drm/i915/intel_pcode.c (`__snb_pcode_rw`, `skl_pcode_try_request`, `skl_pcode_request`,
//! `snb_pcode_write_timeout`, `gen7_check_mailbox_status`) at v7.0-rc6. Copyright (c) Intel Corporation.
//!
//! The mailbox is outside the forcewake domains (intel_pcode.c:65-69). Linux holds `sb_lock` across the whole exchange; the
//! executor is single-threaded per device and owes the same exclusion.
//!
//! One ATTEMPT (`__snb_pcode_rw`, intel_pcode.c:58-95), which the executor performs:
//!   1. read MAILBOX; if READY is set the PCU is busy: [`AttemptOutcome::Busy`] (`-EAGAIN`, intel_pcode.c:71-72);
//!   2. write DATA, DATA1, then MAILBOX = `READY | mbox` ([`attempt_writes`]);
//!   3. poll MAILBOX until READY clears ([`ready_poll`]); on timeout [`AttemptOutcome::MailboxTimeout`];
//!   4. read MAILBOX (status) and DATA (reply): [`AttemptOutcome::Replied`].
//!
//! A REQUEST repeats attempts until the reply matches (`skl_pcode_request`). Linux's loop is `_wait_for(COND, base_ms*1000, 10, 10)`
//! followed by one more `wait_for_atomic(COND, 50)`: unbounded in iterations, bounded in time. [`PcodeSession`] keeps the time
//! bound (`base + 50 ms`) AND adds an attempt bound ([`PCODE_MAX_ATTEMPTS`]) so a host whose clock never advances still stops.
use crate::error::{pcode_status, Bound, ClockError, Handshake};
use crate::plan::{OnTimeout, PollPlan, DEFAULT_MAX_POLLS};
use crate::regs::*;
use i915_common_core::Reg;

/// `__snb_pcode_rw` fast timeout for a request/read (intel_pcode.c:103: `500, 20`; `skl_pcode_try_request` passes `500, 0`, :137).
pub const PCODE_REQUEST_FAST_US: u32 = 500; // intel_pcode.c:137
/// `snb_pcode_write_timeout` fast timeout (intel_pcode.c:121: `250, timeout_ms`).
pub const PCODE_WRITE_FAST_US: u32 = 250; // intel_pcode.c:121
/// The second, preemption-disabled poll of `skl_pcode_request` (intel_pcode.c:200: `wait_for_atomic(COND, 50)`).
pub const PCODE_RETRY_EXTRA_MS: u32 = 50; // intel_pcode.c:200
/// Resend bound. `_wait_for(.., 10 us, 10 us)` can resend at most once per 10 us: base 3 ms + 50 ms = at most ~5300 resends;
/// this is an order above that and exists for a host without a clock.
pub const PCODE_MAX_ATTEMPTS: u32 = 100_000;

/// A request that is resent until the reply matches (`intel_pcode_request`, intel_pcode.c:294-300).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcodeRequest {
    pub mbox: u32,
    pub request: u32,
    pub reply_mask: u32,
    pub reply: u32,
    /// `timeout_base_ms`; Linux `drm_WARN_ON_ONCE`s above 3 (intel_pcode.c:198).
    pub timeout_base_ms: u32,
    pub max_attempts: u32,
}

/// A fire-and-check write (`intel_pcode_write` = `snb_pcode_write_timeout(.., 1)`, intel_pcode.h:33-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PcodeWrite {
    pub mbox: u32,
    pub val: u32,
    pub timeout_ms: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PcodeOp {
    Request(PcodeRequest),
    Write(PcodeWrite),
}

impl PcodeRequest {
    pub const fn new(mbox: u32, request: u32, reply_mask: u32, reply: u32, timeout_base_ms: u32) -> PcodeRequest {
        PcodeRequest { mbox, request, reply_mask, reply, timeout_base_ms, max_attempts: PCODE_MAX_ATTEMPTS }
    }
    /// `SKL_CDCLK_PREPARE_FOR_CHANGE` until `SKL_CDCLK_READY_FOR_CHANGE` (intel_cdclk.c:1178-1181, timeout 3 ms).
    pub const fn cdclk_prepare() -> PcodeRequest {
        PcodeRequest::new(SKL_PCODE_CDCLK_CONTROL, SKL_CDCLK_PREPARE_FOR_CHANGE, SKL_CDCLK_READY_FOR_CHANGE, SKL_CDCLK_READY_FOR_CHANGE, 3)
    }
    pub const fn with_max_attempts(mut self, n: u32) -> PcodeRequest {
        self.max_attempts = n;
        self
    }
    /// Total time the exchange may take: `base` then the 50 ms atomic retry (intel_pcode.c:178-200).
    pub const fn deadline_us(&self) -> u32 {
        (self.timeout_base_ms + PCODE_RETRY_EXTRA_MS) * 1000
    }
    /// The three writes of one attempt (intel_pcode.c:74-77): `(register, value)` in order.
    pub const fn attempt_writes(&self) -> [(Reg, u32); 3] {
        [
            (Reg::new(GEN6_PCODE_DATA), self.request),
            (Reg::new(GEN6_PCODE_DATA1), 0),
            (Reg::new(GEN6_PCODE_MAILBOX), GEN6_PCODE_READY | self.mbox),
        ]
    }
    /// The wait for READY to clear (intel_pcode.c:79-85), `slow_timeout_ms = 0` for a request (intel_pcode.c:137).
    pub const fn ready_poll(&self) -> PollPlan {
        ready_poll(PCODE_REQUEST_FAST_US)
    }
    /// Does a reply satisfy the request? (`skl_pcode_try_request`, intel_pcode.c:139: status 0 AND masked reply equal.)
    pub const fn acknowledged(&self, mailbox: u32, data: u32) -> bool {
        pcode_status(mailbox).is_none() && data & self.reply_mask == self.reply
    }
}

impl PcodeWrite {
    /// `intel_pcode_write(mbox, val)`: 1 ms (intel_pcode.h:33-34).
    pub const fn new(mbox: u32, val: u32) -> PcodeWrite {
        PcodeWrite { mbox, val, timeout_ms: 1 }
    }
    pub const fn attempt_writes(&self) -> [(Reg, u32); 3] {
        [
            (Reg::new(GEN6_PCODE_DATA), self.val),
            (Reg::new(GEN6_PCODE_DATA1), 0),
            (Reg::new(GEN6_PCODE_MAILBOX), GEN6_PCODE_READY | self.mbox),
        ]
    }
    pub const fn ready_poll(&self) -> PollPlan {
        let mut p = ready_poll(PCODE_WRITE_FAST_US);
        p.timeout_us = PCODE_WRITE_FAST_US + self.timeout_ms * 1000;
        p
    }
    /// The status word after READY cleared: `Ok` only for status 0 (`gen7_check_mailbox_status`).
    pub fn judge(&self, mailbox: u32) -> Result<(), ClockError> {
        match pcode_status(mailbox) {
            None => Ok(()),
            Some(errno_class) => Err(ClockError::PcodeRejected { status: mailbox & GEN6_PCODE_ERROR_MASK, errno_class }),
        }
    }
}

const fn ready_poll(timeout_us: u32) -> PollPlan {
    PollPlan {
        what: Handshake::PcodeMailboxReady,
        reg: Reg::new(GEN6_PCODE_MAILBOX),
        mask: GEN6_PCODE_READY,
        want: 0,
        timeout_us,
        max_polls: DEFAULT_MAX_POLLS,
        on_timeout: OnTimeout::Fail,
    }
}

/// What one attempt came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttemptOutcome {
    /// READY was already set before the request was sent.
    Busy { mailbox: u32 },
    /// READY never cleared.
    MailboxTimeout { got: u32 },
    /// READY cleared; the mailbox status word and the DATA reply.
    Replied { mailbox: u32, data: u32 },
}

/// The session's decision after an attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionStep {
    Acknowledged { attempts: u32 },
    Resend,
    Fail(ClockError),
}

/// The bounded retry state of one [`PcodeRequest`].
#[derive(Clone, Copy, Debug)]
pub struct PcodeSession {
    req: PcodeRequest,
    attempts: u32,
    last: Option<AttemptOutcome>,
}

impl PcodeSession {
    pub const fn new(req: PcodeRequest) -> PcodeSession {
        PcodeSession { req, attempts: 0, last: None }
    }
    pub const fn attempts(&self) -> u32 {
        self.attempts
    }
    /// Report an attempt and the time since the exchange began. Success is checked BEFORE the bounds ("Guarantee COND check
    /// prior to timeout"). Linux returns `status ? status : ret` when it gives up (intel_pcode.c:205): a non-zero status
    /// wins over the generic timeout, as here.
    pub fn after_attempt(&mut self, outcome: AttemptOutcome, elapsed_us: u32) -> SessionStep {
        self.attempts = self.attempts.saturating_add(1);
        if let AttemptOutcome::Replied { mailbox, data } = outcome {
            if self.req.acknowledged(mailbox, data) {
                return SessionStep::Acknowledged { attempts: self.attempts };
            }
        }
        self.last = Some(outcome);
        if self.attempts < self.req.max_attempts && elapsed_us < self.req.deadline_us() {
            return SessionStep::Resend;
        }
        SessionStep::Fail(self.failure(elapsed_us))
    }
    fn failure(&self, elapsed_us: u32) -> ClockError {
        match self.last {
            Some(AttemptOutcome::Replied { mailbox, .. }) => match pcode_status(mailbox) {
                Some(errno_class) => ClockError::PcodeRejected { status: mailbox & GEN6_PCODE_ERROR_MASK, errno_class },
                None => ClockError::PcodeNoAck { attempts: self.attempts, elapsed_us },
            },
            Some(AttemptOutcome::Busy { mailbox }) => ClockError::PcodeBusy { mailbox },
            Some(AttemptOutcome::MailboxTimeout { got }) => ClockError::HandshakeTimeout {
                what: Handshake::PcodeMailboxReady,
                bound: if self.attempts >= self.req.max_attempts { Bound::Polls } else { Bound::Time },
                reg: Reg::new(GEN6_PCODE_MAILBOX),
                mask: GEN6_PCODE_READY,
                want: 0,
                got: got & GEN6_PCODE_READY,
                polls: self.attempts,
                elapsed_us,
            },
            None => ClockError::PcodeNoAck { attempts: self.attempts, elapsed_us },
        }
    }
}

