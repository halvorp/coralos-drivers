// SPDX-License-Identifier: GPL-2.0-only
//! A fake DA7219: a 256-byte register file with Linux's reset values, executing the plans' steps over the crate's own wire encoding.
//! Every bus transaction is recorded with its target address; the fake refuses nothing, so a test asserts what was (not) touched.
#![allow(dead_code)]
pub mod defaults;

use da7219_core::bus::*;
use da7219_core::regs::*;
use da7219_core::runner::*;

#[derive(Clone, Debug)]
pub struct Tx {
    pub addr: u16,
    pub write: bool,
    pub reg: u8,
}

pub struct Fake {
    pub regs: [u8; 256],
    /// Every I2C message the plans put on the wire: (target address, is-write, first byte).
    pub log: Vec<Tx>,
    /// Register writes in the order they landed: (step label, register, new value).
    pub writes: Vec<(&'static str, u8, u8)>,
    /// Steps in the order they were asked for.
    pub steps: Vec<&'static str>,
    /// Answer `Nak` to the step with this label.
    pub nak_at: Option<&'static str>,
    /// Bits of a register that ignore writes: (register, mask).
    pub stuck: Vec<(u8, u8)>,
    pub elapsed_ms: u32,
    /// How many poll reads each Poll step took.
    pub poll_reads: u32,
}

impl Fake {
    pub fn reset() -> Self {
        let mut regs = [0u8; 256];
        for &(r, v) in defaults::DEFAULTS {
            regs[r as usize] = v;
        }
        Self { regs, log: vec![], writes: vec![], steps: vec![], nak_at: None, stuck: vec![], elapsed_ms: 0, poll_reads: 0 }
    }

    fn put(&mut self, label: &'static str, reg: u8, value: u8) {
        let mut v = value;
        for &(r, m) in &self.stuck {
            if r == reg {
                v = (v & !m) | (self.regs[reg as usize] & m);
            }
        }
        self.regs[reg as usize] = v;
        self.writes.push((label, reg, v));
    }

    fn record(&mut self, t: &wire::Transaction) {
        for (i, m) in t.messages().iter().enumerate() {
            let write = m.flags & i2c_core_msg_core::flags::I2C_M_RD == 0;
            self.log.push(Tx { addr: m.addr, write, reg: if i == 0 { t.out[0] } else { 0 } });
        }
    }

    /// Execute one step; `None` is a Nak.
    pub fn exec(&mut self, s: Step) -> Input {
        self.steps.push(s.label);
        if self.nak_at == Some(s.label) {
            return Input::Nak;
        }
        match s.op {
            Op::Write { reg, value } => {
                self.record(&wire::write(reg, value).unwrap());
                self.put(s.label, reg, value);
                Input::Done
            }
            Op::Update { reg, mask, value } => {
                self.record(&wire::read(reg, 1).unwrap());
                let new = (self.regs[reg as usize] & !mask) | (value & mask);
                self.record(&wire::write(reg, new).unwrap());
                self.put(s.label, reg, new);
                Input::Done
            }
            Op::Read { reg } | Op::Verify { reg, .. } => {
                self.record(&wire::read(reg, 1).unwrap());
                Input::Value(self.regs[reg as usize])
            }
            Op::Poll { reg, .. } => {
                self.record(&wire::read(reg, 1).unwrap());
                self.poll_reads += 1;
                Input::Value(self.regs[reg as usize])
            }
            Op::DelayMs(ms) => {
                self.elapsed_ms += ms as u32;
                Input::Done
            }
            Op::ReadPair { reg } => {
                self.record(&wire::read(reg, 2).unwrap());
                Input::Pair([self.regs[reg as usize], self.regs[reg as usize + 1]])
            }
            Op::WritePair { reg, first, second } => {
                self.record(&wire::write_pair(reg, first, second).unwrap());
                if reg == DA7219_ACCDET_IRQ_EVENT_A {
                    // the event registers clear on a write of 1
                    self.regs[reg as usize] &= !first;
                    self.regs[reg as usize + 1] &= !second;
                    self.writes.push((s.label, reg, first));
                } else {
                    self.put(s.label, reg, first);
                    self.put(s.label, reg + 1, second);
                }
                Input::Done
            }
        }
    }

    /// Run a whole plan; returns the failure, if any. Poll waits are charged at their interval per extra read.
    pub fn run(&mut self, plan: Plan) -> Result<Option<Failure>, Failure> {
        let mut r = Runner::new(plan).unwrap();
        let mut input = Input::Begin;
        loop {
            match r.next(input) {
                Output::Do(s) => {
                    let before = self.poll_reads;
                    input = self.exec(s);
                    if let Op::Poll { interval_ms, .. } = s.op {
                        let reads = self.poll_reads - before;
                        self.elapsed_ms += interval_ms as u32 * reads.saturating_sub(1);
                    }
                }
                Output::Finished => return Ok(r.tolerated()),
                Output::Failed(f) => return Err(f),
            }
        }
    }

    pub fn reg(&self, r: u8) -> u8 {
        self.regs[r as usize]
    }
}

/// Every message in the log targeted the DA7219 and nothing else.
pub fn assert_only_codec(f: &Fake) {
    assert!(!f.log.is_empty());
    for t in &f.log {
        assert_eq!(t.addr, 0x1A, "a message left the codec: {t:?}");
        assert_ne!(t.addr, 0x31);
        assert_ne!(t.addr, 0x32);
    }
}
