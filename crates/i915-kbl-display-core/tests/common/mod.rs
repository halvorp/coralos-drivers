// SPDX-License-Identifier: GPL-2.0-only
//! A STRICT register file: reading an address that was not given a value PANICS naming it. A readout that touches a
//! wrong offset therefore cannot pass by reading a convenient zero; and a register in a gated power domain can be
//! `forbid`den so that reading it fails the test.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;

pub struct Regs {
    vals: BTreeMap<u32, u32>,
    forbidden: Vec<(u32, u32)>,
    pub reads: RefCell<Vec<u32>>,
}

impl Regs {
    /// Every register `read_display_state` reads on Kaby Lake with all three pipes powered, all zero - plus the power
    /// wells set to the shape a firmware boot leaves: PW1 and MISC_IO requested by the BIOS, PW2 and DDI_A_E by the
    /// driver register, so every pipe and port is readable. Tests overlay the registers they care about.
    pub fn firmware_idle() -> Regs {
        let mut r = Regs { vals: BTreeMap::new(), forbidden: vec![], reads: RefCell::new(vec![]) };
        // power: Linux hsw_power_well_regs, intel_display_power_well.c:1978-1983 (CTL1 = BIOS, CTL2 = driver)
        r.set(0x45400, 0x3000_0003); // CTL1: PW1 (idx 14) req+state = 0x3 << 28, MISC_IO (idx 0) = 0x3
        r.set(0x45404, 0xc000_000c); // CTL2: PW2 (idx 15) = 0x3 << 30, DDI_A_E (idx 1) = 0x3 << 2
        r.set(0x42000, 0x8e00_0000); // SKL_FUSE_STATUS: downloaded + PG0..PG2 distributed
        r.set(0x45504, 0); // DC_STATE_EN
        for a in [0x46010, 0x6c058, 0x6c05c, 0x6c060, 0x46000] {
            r.set(a, 0); // LCPLL1_CTL, DPLL_CTRL1, DPLL_CTRL2, DPLL_STATUS, CDCLK_CTL
        }
        for a in [0x46014, 0x46040, 0x46060, 0x6c040, 0x6c044, 0x6c048, 0x6c04c, 0x6c050, 0x6c054] {
            r.set(a, 0); // LCPLL2_CTL, WRPLL_CTL1/2, DPLL1..3 CFGCR1/2
        }
        for a in [0x64000, 0x64100, 0x64200, 0x64300, 0x64400] {
            r.set(a, 0); // DDI_BUF_CTL A..E
        }
        for t in [0x60000u32, 0x61000, 0x62000, 0x6f000] {
            // the transcoder registers: TRANS_DDI_FUNC_CTL + (only when a transcoder is enabled) the rest, set by the test
            r.set(t + 0x400, 0);
        }
        for pipe_base in [0x70000u32, 0x71000, 0x72000] {
            for plane in 0..3u32 {
                for off in [0x180u32, 0x188, 0x18c, 0x190, 0x19c, 0x1a4] {
                    r.set(pipe_base + plane * 0x100 + off, 0);
                }
            }
        }
        for (scaler_base, n) in [(0x68000u32, 2u32), (0x68800, 2), (0x69000, 1)] {
            for id in 0..n {
                for off in [0x170u32, 0x174, 0x180] {
                    r.set(scaler_base + id * 0x100 + off, 0);
                }
            }
        }
        r
    }

    pub fn set(&mut self, addr: u32, v: u32) -> &mut Self {
        self.vals.insert(addr, v);
        self
    }
    pub fn remove(&mut self, addr: u32) -> &mut Self {
        self.vals.remove(&addr);
        self
    }
    /// Reading any address in `[lo, hi)` fails the test.
    pub fn forbid(&mut self, lo: u32, hi: u32) -> &mut Self {
        self.forbidden.push((lo, hi));
        self
    }
    pub fn read(&self, addr: u32) -> u32 {
        self.reads.borrow_mut().push(addr);
        for (lo, hi) in &self.forbidden {
            if addr >= *lo && addr < *hi {
                panic!("read of forbidden (unpowered) register {addr:#x}");
            }
        }
        match self.vals.get(&addr) {
            Some(v) => *v,
            None => panic!("read of unmapped register {addr:#x}"),
        }
    }
    pub fn was_read(&self, addr: u32) -> bool {
        self.reads.borrow().contains(&addr)
    }
}
