// SPDX-License-Identifier: GPL-2.0-only
//! A fake cAVS HDA controller: BAR0 and PCI configuration as byte arrays with the reset handshakes of GCTL and the stream descriptors,
//! driven by the planners' `Program`s through avs-fw-core's own `Runner`. Every access is logged.
#![allow(dead_code)]
use avs_fw_core::effect::*;
use avs_fw_core::runner::{Program, Runner};

pub const BAR0_LEN: usize = 0x4000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    pub label: &'static str,
    pub space: Space,
    pub off: u32,
    pub width: Width,
    pub write: Option<u32>,
}

pub struct Fake {
    pub bar0: Vec<u8>,
    pub pci: Vec<u8>,
    pub log: Vec<Access>,
    pub delays_us: u32,
    /// GCTL: reads that still show the OLD reset bit after a write changed it (the controller taking its time).
    pub gctl_lag: u32,
    gctl_pending: Option<(u32, u8)>,
    /// A stuck GCTL never changes its reset bit.
    pub gctl_stuck: bool,
    pub sd_lag: u32,
    sd_pending: Vec<(u32, u32, u8)>,
    pub sd_stuck: bool,
    pub nak_at: Option<&'static str>,
}

impl Fake {
    pub fn new() -> Self {
        Self { bar0: vec![0; BAR0_LEN], pci: vec![0; 256], log: vec![], delays_us: 0, gctl_lag: 0, gctl_pending: None, gctl_stuck: false, sd_lag: 0, sd_pending: vec![], sd_stuck: false, nak_at: None }
    }

    fn mem(&mut self, s: Space) -> &mut Vec<u8> {
        match s {
            Space::Hda => &mut self.bar0,
            Space::PciConfig => &mut self.pci,
            Space::Dsp => panic!("hda-ext-core never touches BAR4"),
        }
    }

    pub fn get(&self, s: Space, off: u32, w: Width) -> u32 {
        let m = match s {
            Space::Hda => &self.bar0,
            Space::PciConfig => &self.pci,
            Space::Dsp => panic!(),
        };
        let n = match w {
            Width::B8 => 1,
            Width::W16 => 2,
            Width::D32 => 4,
        };
        let mut v = 0u32;
        for i in 0..n {
            v |= (m[off as usize + i] as u32) << (8 * i);
        }
        v
    }

    pub fn put(&mut self, s: Space, off: u32, w: Width, v: u32) {
        let n = match w {
            Width::B8 => 1,
            Width::W16 => 2,
            Width::D32 => 4,
        };
        let m = self.mem(s);
        for i in 0..n {
            m[off as usize + i] = (v >> (8 * i)) as u8;
        }
    }

    pub fn hda(&self, off: u32, w: Width) -> u32 {
        self.get(Space::Hda, off, w)
    }

    fn is_gctl(r: &Reg) -> bool {
        r.space == Space::Hda && r.off == 0x08
    }

    fn is_sd_ctl_byte(r: &Reg) -> Option<u32> {
        // stream descriptor n: SD_CTL at 0x80 + 0x20 n (byte access)
        if r.space == Space::Hda && r.width == Width::B8 && r.off >= 0x80 && (r.off - 0x80) % 0x20 == 0 {
            Some((r.off - 0x80) / 0x20)
        } else {
            None
        }
    }

    fn read(&mut self, label: &'static str, r: Reg) -> u32 {
        // handshakes: a pending change becomes visible after `lag` reads
        if Self::is_gctl(&r) {
            if let Some((left, newbit)) = self.gctl_pending {
                if left == 0 {
                    let cur = self.hda(0x08, Width::B8) as u8;
                    self.put(Space::Hda, 0x08, Width::B8, ((cur & !1) | newbit) as u32);
                    self.gctl_pending = None;
                } else {
                    self.gctl_pending = Some((left - 1, newbit));
                }
            }
        }
        if let Some(idx) = Self::is_sd_ctl_byte(&r) {
            if let Some(p) = self.sd_pending.iter().position(|p| p.0 == idx) {
                let (_, left, newbit) = self.sd_pending[p];
                if left == 0 {
                    let cur = self.hda(r.off, Width::B8) as u8;
                    self.put(Space::Hda, r.off, Width::B8, ((cur & !1) | newbit) as u32);
                    self.sd_pending.remove(p);
                } else {
                    self.sd_pending[p].1 = left - 1;
                }
            }
        }
        let v = self.get(r.space, r.off, r.width);
        self.log.push(Access { label, space: r.space, off: r.off, width: r.width, write: None });
        v
    }

    fn write(&mut self, label: &'static str, r: Reg, v: u32) {
        self.log.push(Access { label, space: r.space, off: r.off, width: r.width, write: Some(v) });
        if Self::is_gctl(&r) && !self.gctl_stuck {
            let old = self.hda(0x08, Width::B8) as u8 & 1;
            let new = (v & 1) as u8;
            if old != new {
                self.gctl_pending = Some((self.gctl_lag, new));
                // the reset bit does not change yet; every other bit of the write lands
                self.put(r.space, r.off, r.width, (v & !1) | old as u32);
                return;
            }
        }
        if Self::is_gctl(&r) && self.gctl_stuck {
            let old = self.hda(0x08, Width::B8) & 1;
            self.put(r.space, r.off, r.width, (v & !1) | old);
            return;
        }
        if let Some(idx) = Self::is_sd_ctl_byte(&r) {
            let old = self.hda(r.off, Width::B8) as u8 & 1;
            let new = (v & 1) as u8;
            if self.sd_stuck {
                self.put(r.space, r.off, r.width, (v & !1) | old as u32);
                return;
            }
            if old != new {
                self.sd_pending.retain(|p| p.0 != idx);
                self.sd_pending.push((idx, self.sd_lag, new));
                self.put(r.space, r.off, r.width, (v & !1) | old as u32);
                return;
            }
        }
        self.put(r.space, r.off, r.width, v);
    }

    /// Perform one planner step. `None` for a Nak.
    pub fn exec(&mut self, s: Step) -> Input {
        if self.nak_at == Some(s.label) {
            return Input::IpcStatus(1); // any answer the runner will not accept as Done: a protocol error is the loud failure here
        }
        match s.op {
            Op::Update { reg, mask, value } => {
                let cur = self.get(reg.space, reg.off, reg.width);
                let new = (cur & !mask) | (value & mask);
                // log the read-modify-write as one write of the merged value
                self.write(s.label, reg, new);
                Input::Done
            }
            Op::Write { reg, value } => {
                self.write(s.label, reg, value);
                Input::Done
            }
            Op::Read { reg } => Input::Value(self.read(s.label, reg)),
            Op::Poll(p) => Input::Value(self.read(s.label, p.reg)),
            Op::DelayUs(us) => {
                self.delays_us += us;
                Input::Done
            }
            other => panic!("unexpected op {other:?}"),
        }
    }

    pub fn run(&mut self, p: Program) -> Result<(), Failure> {
        let mut r = Runner::new(p, Program::new()).unwrap();
        let mut input = Input::Begin;
        loop {
            match r.next(input) {
                Output::Do(s) => input = self.exec(s),
                Output::Finished => return Ok(()),
                Output::Failed(f) => return Err(f),
            }
        }
    }

    pub fn run_all(&mut self, ps: Vec<Program>) -> Result<(), Failure> {
        for p in ps {
            self.run(p)?;
        }
        Ok(())
    }

    pub fn writes(&self) -> Vec<&Access> {
        self.log.iter().filter(|a| a.write.is_some()).collect()
    }

    pub fn labels(&self) -> Vec<&'static str> {
        self.log.iter().map(|a| a.label).collect()
    }
}
