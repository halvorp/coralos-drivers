// SPDX-License-Identifier: GPL-2.0-only
//! A MAX98373 on the codec's I2C bus: a 16-bit register file with Linux's reset values (`max98373-i2c.c` `max98373_reg`), a soft reset, the
//! revision id, and a hardware reset line that wipes it. Everything written is RECORDED with a global sequence number so a test can say what
//! was set before what; knobs make the part misbehave (a NAK, a register bit that will not take).
#![allow(dead_code)]
use std::collections::HashMap;

pub const R_SW_RESET: u16 = 0x2000;
pub const R_RX_EN: u16 = 0x202B;
pub const R_AMP_EN: u16 = 0x2043;
pub const R_GLOBAL_SHDN: u16 = 0x20FF;
pub const R_REV_ID: u16 = 0x21FF;

/// `max98373_reg` (max98373-i2c.c:29-123): the reset values of the registers that are not zero.
const NONZERO_DEFAULTS: &[(u16, u8)] = &[
    (0x2014, 0x10),
    (0x2015, 0x27),
    (0x2016, 0x01),
    (0x2017, 0xC0),
    (0x201E, 0x55),
    (0x2020, 0xFE),
    (0x2021, 0xFF),
    (0x2024, 0xC0),
    (0x2026, 0x04),
    (0x2027, 0x08),
    (0x2028, 0x88),
    (0x2030, 0xFF),
    (0x2031, 0xFF),
    (0x2032, 0x30),
    (0x2036, 0x05),
    (0x203E, 0x08),
    (0x203F, 0x02),
    (0x2041, 0x03),
    (0x2046, 0x04),
    (0x20D1, 0x01),
    (0x20D2, 0x02),
    (0x20D3, 0x03),
    (0x21FF, 0x42),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AmpTx {
    /// The global sequence number of the access (see `Sim::tx_seq`).
    pub seq: u64,
    pub write: bool,
    pub reg: u16,
    /// For a write the value written; for a read the value returned.
    pub value: u8,
}

pub struct Amp {
    pub addr: u16,
    pub regs: HashMap<u16, u8>,
    /// The reset line is asserted (low): the part answers nothing and holds no state.
    pub in_reset: bool,
    pub txs: Vec<AmpTx>,
    /// Answer NAK to every transaction after this many have succeeded (None: never).
    pub nak_after: Option<usize>,
    /// Registers whose bits in `mask` will not take a written value (a dead field): `(reg, mask)`.
    pub stuck: Vec<(u16, u8)>,
    pub hw_resets: u32,
    /// Sequence numbers of the hardware resets.
    pub reset_at: Vec<u64>,
    pub soft_resets: u32,
    /// Sequence numbers of the transitions of the output stage becoming enabled / disabled.
    pub enabled_at: Vec<u64>,
    pub disabled_at: Vec<u64>,
    was_enabled: bool,
}

impl Amp {
    pub fn new(addr: u16) -> Self {
        let mut a = Self {
            addr,
            regs: HashMap::new(),
            in_reset: false,
            txs: vec![],
            nak_after: None,
            stuck: vec![],
            hw_resets: 0,
            reset_at: vec![],
            soft_resets: 0,
            enabled_at: vec![],
            disabled_at: vec![],
            was_enabled: false,
        };
        a.load_defaults();
        a
    }

    fn load_defaults(&mut self) {
        self.regs.clear();
        for &(r, v) in NONZERO_DEFAULTS {
            self.regs.insert(r, v);
        }
    }

    pub fn reg(&self, r: u16) -> u8 {
        self.regs.get(&r).copied().unwrap_or(0)
    }

    /// The output stage can make sound: globally enabled, receiving PCM, speaker enabled.
    pub fn enabled(&self) -> bool {
        self.reg(R_GLOBAL_SHDN) & 1 != 0 && self.reg(R_RX_EN) & 1 != 0 && self.reg(R_AMP_EN) & 1 != 0
    }

    /// The reset line changed (`asserted` = low).
    pub fn reset_line(&mut self, asserted: bool, seq: u64) {
        if asserted && !self.in_reset {
            self.hw_resets += 1;
            self.reset_at.push(seq);
            self.load_defaults();
            self.track(seq);
        }
        self.in_reset = asserted;
    }

    fn track(&mut self, seq: u64) {
        let now = self.enabled();
        if now && !self.was_enabled {
            self.enabled_at.push(seq);
        }
        if !now && self.was_enabled {
            self.disabled_at.push(seq);
        }
        self.was_enabled = now;
    }

    fn alive(&self) -> bool {
        if self.in_reset {
            return false;
        }
        if let Some(n) = self.nak_after {
            if self.txs.len() >= n {
                return false;
            }
        }
        true
    }

    /// `[reg_hi, reg_lo, value]`.
    pub fn write(&mut self, data: &[u8], seq: u64) -> bool {
        if !self.alive() || data.len() != 3 {
            return false;
        }
        let reg = u16::from_be_bytes([data[0], data[1]]);
        let mut v = data[2];
        for &(r, m) in &self.stuck {
            if r == reg {
                v = (v & !m) | (self.reg(reg) & m);
            }
        }
        self.txs.push(AmpTx { seq, write: true, reg, value: data[2] });
        if reg == R_SW_RESET && v & 1 != 0 {
            self.load_defaults();
            self.soft_resets += 1;
            self.track(seq);
            return true;
        }
        if reg == R_REV_ID {
            return true;
        }
        self.regs.insert(reg, v);
        self.track(seq);
        true
    }

    /// The register address, then one byte back.
    pub fn write_read(&mut self, wr: &[u8], out: &mut [u8], seq: u64) -> bool {
        if !self.alive() || wr.len() != 2 || out.len() != 1 {
            return false;
        }
        let reg = u16::from_be_bytes([wr[0], wr[1]]);
        out[0] = self.reg(reg);
        self.txs.push(AmpTx { seq, write: false, reg, value: out[0] });
        true
    }

    /// The writes, in order, as `(reg, value)`.
    pub fn writes(&self) -> Vec<(u16, u8)> {
        self.txs.iter().filter(|t| t.write).map(|t| (t.reg, t.value)).collect()
    }
}
