// SPDX-License-Identifier: GPL-2.0-only
//! A scripted 9000-family register file for the vectors. Register numbers and bits are Linux
//! LITERALS here (iwl-csr.h / iwl-fh.h / iwl-prph.h), never the crate's constants, so a wrong
//! constant in the crate drives the wrong register of the fake and the test goes red.
//!
//! Device reactions (each switchable):
//! - CSR_HW_IF_CONFIG_REG (0x000) PCI_OWN_SET (0x00400000) sticks only from the `own_after`-th claim;
//! - CSR_GP_CNTRL (0x024) INIT_DONE (0x4) raises MAC_CLOCK_READY (0x1);
//! - CSR_RESET (0x020) STOP_MASTER (0x200) raises MASTER_DISABLED (0x100); SW_RESET (0x80) self-clears;
//! - FH_TSSR_TX_STATUS_REG (0x1EB0) reads `tx_status`; RFH_GEN_STATUS (0xA09808) reads `rfh_status`.
#![allow(dead_code)]

use iwlwifi_pcie_trans_core::io::TransIo;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    W(u32, u32),
    P(u32, u32),
    Sleep(u32),
}

pub struct Sim {
    pub csr: HashMap<u32, u32>,
    pub prph: HashMap<u32, u32>,
    pub ops: Vec<Op>,
    pub claims: u32,
    pub own_after: u32,
    pub clock_follows_init_done: bool,
    pub master_follows_stop: bool,
    pub tx_status: u32,
    pub rfh_status: u32,
}

impl Sim {
    pub fn new() -> Self {
        let mut s = Sim {
            csr: HashMap::new(),
            prph: HashMap::new(),
            ops: vec![],
            claims: 0,
            own_after: 1,
            clock_follows_init_done: true,
            master_follows_stop: true,
            tx_status: 0x00FF_0000, // all 8 channels idle
            rfh_status: 0x8000_0000, // RXF_DMA_IDLE
        };
        s.csr.insert(0x024, 0x0800_0000); // GP_CNTRL: HW_RF_KILL_SW set = radio enabled
        s
    }
    /// Register traffic only (no sleeps).
    pub fn writes(&self) -> Vec<Op> {
        self.ops.iter().copied().filter(|o| !matches!(o, Op::Sleep(_))).collect()
    }
    pub fn sleeps(&self, us: u32) -> usize {
        self.ops.iter().filter(|o| **o == Op::Sleep(us)).count()
    }
    pub fn pos(&self, op: Op) -> usize {
        self.ops.iter().position(|o| *o == op).unwrap_or_else(|| panic!("{op:x?} never happened"))
    }
}

impl TransIo for Sim {
    fn csr_read(&mut self, reg: u32) -> u32 {
        match reg {
            0x1EB0 => self.tx_status,
            _ => *self.csr.get(&reg).unwrap_or(&0),
        }
    }
    fn csr_write(&mut self, reg: u32, val: u32) {
        self.ops.push(Op::W(reg, val));
        let mut v = val;
        if reg == 0x000 && v & 0x0040_0000 != 0 {
            self.claims += 1;
            if self.claims < self.own_after {
                v &= !0x0040_0000;
            }
        }
        if reg == 0x024 && self.clock_follows_init_done && v & 0x4 != 0 {
            v |= 0x1;
        }
        if reg == 0x024 && v & 0x4 == 0 {
            v &= !0x1; // INIT_DONE cleared: the MAC clock goes with it
        }
        if reg == 0x020 {
            if self.master_follows_stop && v & 0x200 != 0 {
                v |= 0x100;
            }
            v &= !0x80; // SW_RESET self-clears
        }
        self.csr.insert(reg, v);
    }
    fn prph_read(&mut self, addr: u32) -> u32 {
        match addr {
            0xA0_9808 => self.rfh_status,
            _ => *self.prph.get(&addr).unwrap_or(&0),
        }
    }
    fn prph_write(&mut self, addr: u32, val: u32) {
        self.ops.push(Op::P(addr, val));
        self.prph.insert(addr, val);
    }
    fn prph_read_no_grab(&mut self, addr: u32) -> u32 {
        self.prph_read(addr)
    }
    fn prph_write_no_grab(&mut self, addr: u32, val: u32) {
        self.prph_write(addr, val)
    }
    fn sleep_us(&mut self, us: u32) {
        self.ops.push(Op::Sleep(us));
    }
}
