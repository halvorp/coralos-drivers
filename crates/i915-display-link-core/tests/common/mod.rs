// SPDX-License-Identifier: GPL-2.0-only
//! Test harness: a scripted AUX sink and an executor for `AuxOp` / `DpcdStep`. Everything it does is bounded by an op budget so a
//! planner that loses a bound FAILS an assertion instead of looping or eating memory (WAVE2-BRIEF, forcewake incident).
#![allow(dead_code, unused_imports)]
pub mod chdr;
use i915_display_link_core::aux::*;
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::plan::{Effect, PollStep};
use i915_display_link_core::regs::*;
use i915_vbt_core::child::AuxCh;
use std::collections::HashMap;

/// Hard cap on executor operations in any one scripted run.
pub const OP_BUDGET: usize = 20_000;

/// What the hardware does when the CPU writes `DP_AUX_CH_CTL`: given the bytes loaded into the data registers (as sent) and
/// the control word, the status word + receive bytes the channel ends with.
pub struct AuxHw {
    pub ch: AuxCh,
    pub regs: HashMap<u32, u32>,
    /// Every register write, in order.
    pub writes: Vec<(u32, u32)>,
    pub sleeps_ms: Vec<u32>,
    pub sleeps_us: Vec<(u32, u32)>,
    /// Decide the outcome of the n-th send (0-based): returns (status flags, rx bytes).
    pub responder: Box<dyn FnMut(usize, &[u8]) -> (u32, Vec<u8>)>,
    pub sends: usize,
    pub sent: Vec<Vec<u8>>,
    /// Number of times CTL is read "busy" before it reads idle, per transfer.
    pub busy_reads: usize,
    pub busy_left: usize,
    pub ops: usize,
}

impl AuxHw {
    pub fn new(ch: AuxCh, responder: impl FnMut(usize, &[u8]) -> (u32, Vec<u8>) + 'static) -> AuxHw {
        AuxHw { ch, regs: HashMap::new(), writes: vec![], sleeps_ms: vec![], sleeps_us: vec![], responder: Box::new(responder), sends: 0, sent: vec![], busy_reads: 0, busy_left: 0, ops: 0 }
    }

    fn ctl(&self) -> u32 {
        dp_aux_ch_ctl(self.ch).offset()
    }

    /// The bytes currently in the five send data registers, `n` of them.
    fn loaded(&self, n: usize) -> Vec<u8> {
        let mut out = vec![];
        for i in 0..n {
            let r = self.regs.get(&dp_aux_ch_data(self.ch, (i / 4) as u32).offset()).copied().unwrap_or(0);
            out.push((r >> ((3 - (i % 4)) * 8)) as u8);
        }
        out
    }

    pub fn write(&mut self, reg: u32, v: u32) {
        self.writes.push((reg, v));
        if reg == self.ctl() {
            if v & DP_AUX_CH_CTL_SEND_BUSY != 0 {
                let n = ((v & DP_AUX_CH_CTL_MESSAGE_SIZE_MASK) >> 20) as usize;
                let sent = self.loaded(n);
                let (status, rx) = (self.responder)(self.sends, &sent);
                self.sends += 1;
                self.sent.push(sent);
                // The channel completes at once: BUSY clear, the status flags and the size field set, rx bytes in the data regs.
                let size = (rx.len() as u32) << 20;
                self.regs.insert(reg, (status & !DP_AUX_CH_CTL_MESSAGE_SIZE_MASK) | (size & DP_AUX_CH_CTL_MESSAGE_SIZE_MASK));
                for (i, chunk) in rx.chunks(4).enumerate() {
                    let mut w = 0u32;
                    for (j, b) in chunk.iter().enumerate() {
                        w |= (*b as u32) << ((3 - j) * 8);
                    }
                    self.regs.insert(dp_aux_ch_data(self.ch, i as u32).offset(), w);
                }
            } else {
                // A "clear status" write: W1C of DONE / TIMEOUT / RECEIVE_ERROR.
                let cur = self.regs.get(&reg).copied().unwrap_or(0);
                let clear = v & (DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR | DP_AUX_CH_CTL_RECEIVE_ERROR);
                self.regs.insert(reg, cur & !clear);
            }
        } else {
            self.regs.insert(reg, v);
        }
    }

    pub fn read(&mut self, reg: u32) -> u32 {
        if reg == self.ctl() && self.busy_left > 0 {
            self.busy_left -= 1;
            return DP_AUX_CH_CTL_SEND_BUSY;
        }
        self.regs.get(&reg).copied().unwrap_or(0)
    }

    /// Perform one `AuxOp`, return the feed for the machine.
    pub fn perform(&mut self, op: AuxOp) -> Feed {
        self.ops += 1;
        assert!(self.ops < OP_BUDGET, "AUX machine exceeded the op budget: a bound is missing");
        match op {
            AuxOp::Read(r) => Feed::Value(self.read(r.offset())),
            AuxOp::Write(r, v) => {
                self.write(r.offset(), v);
                Feed::Ack
            }
            AuxOp::WaitNotBusy(plan) => {
                let reg = plan.reg.offset();
                let mut last = 0;
                let _ = plan.run(|| { last = self.read(reg); last }, |_| {});
                // run() consumed the closure's borrow; re-read the value it ended with.
                Feed::Value(last)
            }
            AuxOp::SleepMs(ms) => {
                self.sleeps_ms.push(ms);
                Feed::Ack
            }
            AuxOp::SleepUs { min, max } => {
                self.sleeps_us.push((min, max));
                Feed::Ack
            }
        }
    }
}

/// Drive a raw `AuxXfer`.
pub fn run_xfer(hw: &mut AuxHw, x: &mut AuxXfer) -> Result<AuxRx, AuxFail> {
    let mut feed = Feed::Start;
    loop {
        match x.step(feed) {
            AuxStep::Op(op) => feed = hw.perform(op),
            AuxStep::Done(r) => return r,
        }
    }
}

/// Drive a `DpcdXfer`.
pub fn run_dpcd(hw: &mut AuxHw, x: &mut DpcdXfer) -> Result<DpcdOk, DpcdFail> {
    let mut feed = Feed::Start;
    loop {
        match x.step(feed) {
            DpcdStep::Op(op) => feed = hw.perform(op),
            DpcdStep::Done(r) => return r,
        }
    }
}

/// A reply of native-ACK with `payload` for a read, as the wire carries it: reply byte (code in the high nibble) then data.
pub fn ack_read(payload: &[u8]) -> (u32, Vec<u8>) {
    let mut rx = vec![0x00];
    rx.extend_from_slice(payload);
    (DP_AUX_CH_CTL_DONE, rx)
}

pub fn ack_write() -> (u32, Vec<u8>) {
    (DP_AUX_CH_CTL_DONE, vec![0x00])
}

pub fn effects_writes(plan: &[Effect]) -> Vec<(u32, u32)> {
    plan.iter().filter_map(|e| if let Effect::Write { reg, value } = e { Some((reg.offset(), *value)) } else { None }).collect()
}

pub fn judge_all_unsatisfied(plan: &i915_display_link_core::plan::PollPlan) -> u32 {
    let mut n = 0;
    loop {
        n += 1;
        match plan.judge(n, !plan.want & plan.mask) {
            PollStep::Again => {}
            _ => return n,
        }
        assert!(n < 10_000_000, "poll never ends");
    }
}
