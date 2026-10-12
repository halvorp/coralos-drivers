// SPDX-License-Identifier: GPL-2.0-only
//! The PCU memory-latency read the watermark code starts from, as a planner: two `GEN9_PCODE_READ_MEM_LATENCY` mailbox exchanges
//! (data0 = 0 then 1), each a single bounded attempt on `display-clock-core`'s [`PcodeSession`].
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_watermark.c `skl_read_wm_latency` (two `intel_pcode_read` calls) and
//! drivers/gpu/drm/i915/intel_pcode.c `snb_pcode_read` / `__snb_pcode_rw` (`500 us` fast + `20 ms` slow ready wait, no resend: a
//! READ is one attempt, unlike `skl_pcode_request`). Copyright (c) Intel Corporation.
//!
//! The mailbox words are decoded and adjusted by `i915_kbl_display_core::skl_wm::{latencies_from_pcode, adjust_latencies}`.
//!
//! DEVIATION: if either exchange fails, [`MemLatencyRead`] reports the failure. Linux `drm_err()`s and carries on with the latency
//! table zeroed, which `adjust_wm_latency` turns into a single 2 us level - watermarks built on a latency nobody measured.
use i915_common_core::Reg;
use i915_display_clock_core::{AttemptOutcome, ClockError, PcodeRequest, PcodeSession, SessionStep};
use i915_display_link_core::plan::PollPlan;
use i915_gt_regs_core::i915_reg::{GEN6_PCODE_MAILBOX, GEN6_PCODE_READY};
use i915_kbl_display_core::regs_write::GEN9_PCODE_READ_MEM_LATENCY;

/// `__snb_pcode_rw(.., 500, 20, ..)` as one bound: 500 us fast + 20 ms slow (intel_pcode.c:103-104).
pub const READ_READY_TIMEOUT_US: u32 = 500 + 20_000;
/// Spacing of `GEN6_PCODE_MAILBOX` reads while waiting for READY to clear.
pub const READ_READY_INTERVAL_US: u32 = 10;

/// What the executor does for one exchange: the three register writes, then the bounded READY wait, then reads MAILBOX and DATA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadAttempt {
    /// `(register, value)` in order: `GEN6_PCODE_DATA = data0`, `GEN6_PCODE_DATA1 = 0`, `GEN6_PCODE_MAILBOX = READY | mbox`.
    pub writes: [(Reg, u32); 3],
    pub ready_poll: PollPlan,
}

/// Progress after an exchange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadStep {
    /// Exchange 0 done; perform [`MemLatencyRead::attempt`] again for exchange 1.
    Next,
    /// Both words, `[levels 0-3, levels 4-7]`, ready for `latencies_from_pcode`.
    Done([u32; 2]),
    Failed(ClockError),
}

/// The two-exchange read.
#[derive(Clone, Copy, Debug)]
pub struct MemLatencyRead {
    phase: u8,
    session: PcodeSession,
    words: [u32; 2],
}

fn request(data0: u32) -> PcodeRequest {
    // reply_mask 0: any data acknowledges (the reply IS the payload); a non-zero mailbox STATUS still fails it. One attempt.
    PcodeRequest::new(GEN9_PCODE_READ_MEM_LATENCY, data0, 0, 0, 0).with_max_attempts(1)
}

impl MemLatencyRead {
    pub fn new() -> MemLatencyRead {
        MemLatencyRead { phase: 0, session: PcodeSession::new(request(0)), words: [0; 2] }
    }
    /// Which exchange is next: 0, 1, or 2 when finished.
    pub const fn phase(&self) -> u8 {
        self.phase
    }
    /// The current exchange, or `None` once both are done.
    pub fn attempt(&self) -> Option<ReadAttempt> {
        if self.phase > 1 {
            return None;
        }
        let req = request(self.phase as u32);
        Some(ReadAttempt {
            writes: req.attempt_writes(),
            ready_poll: PollPlan::with_timeout(
                Reg::new(GEN6_PCODE_MAILBOX),
                GEN6_PCODE_READY,
                0,
                READ_READY_INTERVAL_US,
                READ_READY_TIMEOUT_US,
                "PCODE mailbox ready (memory latency read)",
            ),
        })
    }
    /// Report what the exchange came to and the microseconds since it began.
    pub fn feed(&mut self, outcome: AttemptOutcome, elapsed_us: u32) -> ReadStep {
        if self.phase > 1 {
            return ReadStep::Failed(ClockError::PcodeNoAck { attempts: 0, elapsed_us });
        }
        match self.session.after_attempt(outcome, elapsed_us) {
            SessionStep::Acknowledged { .. } => {
                if let AttemptOutcome::Replied { data, .. } = outcome {
                    self.words[self.phase as usize] = data;
                }
                self.phase += 1;
                if self.phase == 1 {
                    self.session = PcodeSession::new(request(1));
                    ReadStep::Next
                } else {
                    ReadStep::Done(self.words)
                }
            }
            SessionStep::Fail(e) => {
                self.phase = 2;
                ReadStep::Failed(e)
            }
            // A single-attempt request never resends; if the session ever said so, the bound would be broken - refuse.
            SessionStep::Resend => {
                self.phase = 2;
                ReadStep::Failed(ClockError::PcodeNoAck { attempts: self.session.attempts(), elapsed_us })
            }
        }
    }
}

impl Default for MemLatencyRead {
    fn default() -> Self {
        Self::new()
    }
}
