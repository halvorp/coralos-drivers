// SPDX-License-Identifier: GPL-2.0-only
//! A DA7219 on an I2C bus: a 256-byte register file with Linux's reset values, soft reset, the accessory detect's event registers
//! (write 1 to clear) and its interrupt line (LOW while any unmasked event is pending), a jack that can be plugged and unplugged, and
//! knobs that make it misbehave (a NAK, a register bit that will not set).
#![allow(dead_code)]
use da7219_core::aad_fields::*;
use da7219_core::regs::*;

pub const ADDR: u16 = 0x1a;

pub struct Codec {
    pub regs: [u8; 256],
    /// Every transaction that reached this address, in order: (is_write, first byte, len).
    pub txs: Vec<(bool, u8, usize)>,
    /// Answer NAK to every transaction after this many have succeeded (None: never).
    pub nak_after: Option<usize>,
    /// Registers whose written value is ignored (a dead stage).
    pub stuck: Vec<(u8, u8)>,
    pub inserted: bool,
    pub four_pole: bool,
    pub soft_resets: u32,
    /// After every clear of the event registers, raise the insertion event again (a line that will not go quiet).
    pub reassert: bool,
}

impl Codec {
    pub fn new() -> Self {
        let mut c = Self { regs: [0; 256], txs: vec![], nak_after: None, stuck: vec![], inserted: false, four_pole: false, soft_resets: 0, reassert: false };
        c.reset_regs();
        c
    }

    fn reset_regs(&mut self) {
        self.regs = [0; 256];
        for &(r, v) in super::defaults::DEFAULTS {
            self.regs[r as usize] = v;
        }
        // identity and the state the machine driver leaves alone
        self.regs[DA7219_CHIP_ID1 as usize] = 0x23;
        self.regs[DA7219_CHIP_ID2 as usize] = 0x93;
        self.regs[DA7219_CHIP_REVISION as usize] = 0x02;
        // the accessory detect's masks reset to ALL MASKED
        self.regs[DA7219_ACCDET_IRQ_MASK_A as usize] = 0xff;
        self.regs[DA7219_ACCDET_IRQ_MASK_B as usize] = 0xff;
    }

    /// Is the interrupt line asserted (LOW)? Any pending event that is not masked.
    pub fn irq_asserted(&self) -> bool {
        let a = self.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] & !self.regs[DA7219_ACCDET_IRQ_MASK_A as usize];
        let b = self.regs[DA7219_ACCDET_IRQ_EVENT_B as usize] & !self.regs[DA7219_ACCDET_IRQ_MASK_B as usize];
        a | b != 0
    }

    pub fn plug(&mut self, four_pole: bool) {
        self.inserted = true;
        self.four_pole = four_pole;
        let ea = DA7219_E_JACK_INSERTED_MASK | DA7219_E_JACK_DETECT_COMPLETE_MASK;
        self.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] |= ea;
        let mut st = DA7219_JACK_INSERTION_STS_MASK;
        if four_pole {
            st |= DA7219_JACK_TYPE_STS_MASK;
        }
        self.regs[DA7219_ACCDET_STATUS_A as usize] = st;
    }

    pub fn press(&mut self, button: u8) {
        self.regs[DA7219_ACCDET_IRQ_EVENT_B as usize] |= 1 << button;
    }

    pub fn release(&mut self, button: u8) {
        self.regs[DA7219_ACCDET_IRQ_EVENT_B as usize] |= 0x80 >> button;
    }

    pub fn unplug(&mut self) {
        self.inserted = false;
        self.regs[DA7219_ACCDET_IRQ_EVENT_A as usize] |= DA7219_E_JACK_REMOVED_MASK;
        self.regs[DA7219_ACCDET_STATUS_A as usize] = 0;
    }

    fn put(&mut self, reg: u8, v: u8) {
        let mut v = v;
        for &(r, m) in &self.stuck {
            if r == reg {
                v = (v & !m) | (self.regs[reg as usize] & m);
            }
        }
        if reg == DA7219_CIF_CTRL && v & DA7219_CIF_REG_SOFT_RESET_MASK_B != 0 {
            let id_keep = self.regs[DA7219_SYSTEM_ACTIVE as usize];
            self.reset_regs();
            self.regs[DA7219_SYSTEM_ACTIVE as usize] = id_keep;
            self.soft_resets += 1;
            return;
        }
        if reg == DA7219_ACCDET_IRQ_EVENT_A || reg == DA7219_ACCDET_IRQ_EVENT_B {
            // write 1 to clear
            self.regs[reg as usize] &= !v;
            if self.reassert && reg == DA7219_ACCDET_IRQ_EVENT_A {
                self.regs[reg as usize] |= DA7219_E_JACK_INSERTED_MASK;
            }
            return;
        }
        let prev = self.regs[reg as usize];
        self.regs[reg as usize] = v;
        // switching the accessory detect ON with a plug in the jack makes it notice the plug (Linux relies on this at probe)
        if reg == DA7219_ACCDET_CONFIG_1 && v & da7219_core::aad_fields::DA7219_ACCDET_EN_MASK != 0 && prev & da7219_core::aad_fields::DA7219_ACCDET_EN_MASK == 0 && self.inserted {
            let fp = self.four_pole;
            self.plug(fp);
        }
    }

    fn ok(&mut self) -> bool {
        if let Some(n) = self.nak_after {
            if self.txs.len() >= n {
                return false;
            }
        }
        true
    }

    /// A write transaction `[reg, v0, v1...]`.
    pub fn write(&mut self, data: &[u8]) -> bool {
        if !self.ok() || data.is_empty() {
            return false;
        }
        self.txs.push((true, data[0], data.len()));
        for (i, b) in data[1..].iter().enumerate() {
            self.put(data[0].wrapping_add(i as u8), *b);
        }
        true
    }

    /// A write of the register address then a read of `out.len()` registers (auto-increment).
    pub fn write_read(&mut self, wr: &[u8], out: &mut [u8]) -> bool {
        if !self.ok() || wr.len() != 1 {
            return false;
        }
        self.txs.push((false, wr[0], out.len()));
        for (i, o) in out.iter_mut().enumerate() {
            let r = wr[0].wrapping_add(i as u8);
            // the SRM PLL reports lock (and the MCLK it locked to) once the clocks it needs are there; the model always has them
            *o = if r == DA7219_PLL_SRM_STS { 0x90 } else { self.regs[r as usize] };
        }
        true
    }
}

const DA7219_CIF_REG_SOFT_RESET_MASK_B: u8 = 0x80;
