// SPDX-License-Identifier: GPL-2.0-only
//! A tiny hardware model and a plan INTERPRETER for the tests. The interpreter is test-only: the crate itself never executes a
//! plan (`devices/i915` does). It drives every poll through `PollBudget` and every PCU exchange through `PcodeSession`, exactly
//! as an executor must, and panics if a bound is not enforced (so a removed bound fails the test instead of hanging it).
#![allow(dead_code)]
use i915_display_clock_core::pcode::*;
use i915_display_clock_core::plan::*;
use i915_display_clock_core::regs::*;
use i915_display_clock_core::ClockError;
use std::collections::HashMap;

/// Hard stop for any loop in the interpreter: reaching it means a bound was not enforced.
pub const HARD_LIMIT: u32 = 2_000_000;

#[derive(Default)]
pub struct Sim {
    pub regs: HashMap<u32, u32>,
    pub reads: HashMap<u32, u32>,
    /// Every register write, in order.
    pub writes: Vec<(u32, u32)>,
    /// Power-well idx bits that never acknowledge a request.
    pub stuck_pw_idx: u32,
    /// Power-well idx bits whose STATE stays set after the request is withdrawn (a DMC / KVMR request holding the well).
    pub forced_on_idx: u32,
    pub pll_never_locks: bool,
    /// PCU model.
    pub pcu_requests: u32,
    pub pcu_ack_after: u32,
    pub pcu_status: u32,
    pub pcu_ready_stuck: bool,
    pub pcu_reply: u32,
    /// Microseconds charged per register sample / per PCU attempt.
    pub us_per_sample: u32,
    pub us_per_attempt: u32,
    pub tolerated: Vec<ClockError>,
    pub clock_us: u32,
}

impl Sim {
    pub fn new() -> Sim {
        let mut s = Sim { us_per_sample: 1, us_per_attempt: 10, pcu_reply: SKL_CDCLK_READY_FOR_CHANGE, ..Default::default() };
        // fuse distribution done for PG0..PG2 and the download flag
        s.regs.insert(SKL_FUSE_STATUS, SKL_FUSE_DOWNLOAD_STATUS | (1 << 27) | (1 << 26) | (1 << 25));
        s
    }
    pub fn get(&self, off: u32) -> u32 {
        *self.regs.get(&off).unwrap_or(&0)
    }
    pub fn set(&mut self, off: u32, v: u32) {
        self.regs.insert(off, v);
    }
    pub fn read(&mut self, off: u32) -> u32 {
        *self.reads.entry(off).or_insert(0) += 1;
        self.clock_us = self.clock_us.saturating_add(self.us_per_sample);
        self.get(off)
    }
    pub fn write(&mut self, off: u32, v: u32) {
        self.writes.push((off, v));
        self.regs.insert(off, v);
        match off {
            HSW_PWR_WELL_CTL2 => {
                let mut state = 0;
                for idx in 0..16 {
                    let on = v & (0x2 << (idx * 2)) != 0 && self.stuck_pw_idx & (1 << idx) == 0;
                    if on || self.forced_on_idx & (1 << idx) != 0 {
                        state |= 0x1 << (idx * 2);
                    }
                }
                let keep = v & 0xaaaa_aaaa;
                self.regs.insert(off, keep | state);
            }
            LCPLL1_CTL => {
                let lock = if v & LCPLL_PLL_ENABLE != 0 && !self.pll_never_locks { LCPLL_PLL_LOCK } else { 0 };
                self.regs.insert(off, (v & !LCPLL_PLL_LOCK) | lock);
            }
            LCPLL2_CTL | WRPLL_CTL1 | WRPLL_CTL2 => {
                let id: u32 = match off {
                    LCPLL2_CTL => 1,
                    WRPLL_CTL1 => 2,
                    _ => 3,
                };
                let mut st = self.get(DPLL_STATUS) & !(1 << (id * 8));
                if v & LCPLL_PLL_ENABLE != 0 && !self.pll_never_locks {
                    st |= 1 << (id * 8);
                }
                self.regs.insert(DPLL_STATUS, st);
            }
            GEN6_PCODE_MAILBOX if v & GEN6_PCODE_READY != 0 => {
                self.pcu_requests += 1;
                if self.pcu_ready_stuck {
                    return;
                }
                let acked = self.pcu_requests > self.pcu_ack_after;
                let data = if acked { self.pcu_reply } else { 0 };
                self.regs.insert(GEN6_PCODE_DATA, data);
                self.regs.insert(GEN6_PCODE_MAILBOX, self.pcu_status & GEN6_PCODE_ERROR_MASK);
            }
            _ => {}
        }
    }
    pub fn writes_to(&self, off: u32) -> Vec<u32> {
        self.writes.iter().filter(|w| w.0 == off).map(|w| w.1).collect()
    }
}

#[derive(Debug, Default)]
pub struct Report {
    pub steps: usize,
    pub polls: u32,
}

fn run_poll(sim: &mut Sim, p: PollPlan) -> Result<u32, ClockError> {
    let mut b = PollBudget::new(p);
    let start = sim.clock_us;
    for _ in 0..HARD_LIMIT {
        let v = sim.read(p.reg.offset());
        match b.observe(v, sim.clock_us - start) {
            PollVerdict::Met { polls } => return Ok(polls),
            PollVerdict::Pending => {}
            PollVerdict::Tolerated(e) => {
                sim.tolerated.push(e);
                return Ok(b.polls());
            }
            PollVerdict::Failed(e) => return Err(e),
        }
    }
    panic!("poll {:?} never terminated: a bound is not enforced", p.what);
}

fn pcu_attempt(sim: &mut Sim, writes: [(i915_common_core::Reg, u32); 3], poll: PollPlan) -> AttemptOutcome {
    let mb = sim.read(GEN6_PCODE_MAILBOX);
    if mb & GEN6_PCODE_READY != 0 {
        return AttemptOutcome::Busy { mailbox: mb };
    }
    for (r, v) in writes {
        sim.write(r.offset(), v);
    }
    match run_poll(sim, poll) {
        Err(_) => AttemptOutcome::MailboxTimeout { got: sim.get(GEN6_PCODE_MAILBOX) },
        Ok(_) => {
            let mailbox = sim.read(GEN6_PCODE_MAILBOX);
            let data = sim.read(GEN6_PCODE_DATA);
            AttemptOutcome::Replied { mailbox, data }
        }
    }
}

pub fn run_pcode(sim: &mut Sim, op: PcodeOp) -> Result<u32, ClockError> {
    match op {
        PcodeOp::Request(r) => {
            let mut s = PcodeSession::new(r);
            let start = sim.clock_us;
            for _ in 0..HARD_LIMIT {
                let out = pcu_attempt(sim, r.attempt_writes(), r.ready_poll());
                sim.clock_us = sim.clock_us.saturating_add(sim.us_per_attempt);
                match s.after_attempt(out, sim.clock_us - start) {
                    SessionStep::Acknowledged { attempts } => return Ok(attempts),
                    SessionStep::Resend => {}
                    SessionStep::Fail(e) => return Err(e),
                }
            }
            panic!("pcode request never terminated: the attempt bound is not enforced");
        }
        PcodeOp::Write(w) => {
            let out = pcu_attempt(sim, w.attempt_writes(), w.ready_poll());
            match out {
                AttemptOutcome::Replied { mailbox, .. } => w.judge(mailbox).map(|_| 1),
                AttemptOutcome::Busy { mailbox } => Err(ClockError::PcodeBusy { mailbox }),
                AttemptOutcome::MailboxTimeout { .. } => Err(ClockError::PcodeBusy { mailbox: sim.get(GEN6_PCODE_MAILBOX) }),
            }
        }
    }
}

pub fn run_plan(sim: &mut Sim, plan: &WritePlan) -> Result<Report, ClockError> {
    let mut rep = Report::default();
    for s in plan.iter() {
        match *s {
            Step::Rmw { reg, clear, set } => {
                let old = sim.read(reg.offset());
                sim.write(reg.offset(), (old & !clear) | set);
            }
            Step::Write { reg, value } => sim.write(reg.offset(), value),
            Step::PostingRead { reg } => {
                sim.read(reg.offset());
            }
            Step::Poll(p) => rep.polls += run_poll(sim, p)?,
            Step::Pcode(op) => rep.polls += run_pcode(sim, op)?,
        }
        rep.steps += 1;
    }
    Ok(rep)
}

/// Forward; on failure, rollback. Returns the forward result and (if run) the rollback result.
pub fn run_tx(sim: &mut Sim, t: &Transaction) -> (Result<Report, ClockError>, Option<Result<Report, ClockError>>) {
    match run_plan(sim, &t.forward) {
        Ok(r) => (Ok(r), None),
        Err(e) => {
            // the hardware model is healed for the rollback only where the test says so
            let rb = run_plan(sim, &t.rollback);
            (Err(e), Some(rb))
        }
    }
}
