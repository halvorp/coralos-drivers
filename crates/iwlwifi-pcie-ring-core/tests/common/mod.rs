// SPDX-License-Identifier: GPL-2.0-only
//! A recording register file: every access in order, scripted read values, and a scripted `RawBus`.
#![allow(dead_code)]
use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo, RawBus};
use iwlwifi_pcie_trans_core::io::TransIo;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    CsrW(u32, u32),
    PrphW(u32, u32),
    Prph64W(u32, u64),
    Direct32W(u32, u32),
    W8(u32, u8),
    Grab,
    Release,
    MemW(u32, Vec<u32>),
    MemZero(u32, usize),
    Sleep(u32),
}

/// A `NicIo` that records, with per-register read values.
#[derive(Default)]
pub struct Rec {
    pub ops: Vec<Op>,
    pub csr: HashMap<u32, u32>,
    pub prph: HashMap<u32, u32>,
    pub grabs: i32,
    pub fail_grab: bool,
}

impl TransIo for Rec {
    fn csr_read(&mut self, reg: u32) -> u32 {
        *self.csr.get(&reg).unwrap_or(&0)
    }
    fn csr_write(&mut self, reg: u32, val: u32) {
        self.csr.insert(reg, val);
        self.ops.push(Op::CsrW(reg, val));
    }
    fn prph_read(&mut self, addr: u32) -> u32 {
        *self.prph.get(&addr).unwrap_or(&0)
    }
    fn prph_write(&mut self, addr: u32, val: u32) {
        self.prph.insert(addr, val);
        self.ops.push(Op::PrphW(addr, val));
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

impl NicIo for Rec {
    fn grab(&mut self) -> bool {
        if self.fail_grab {
            return false;
        }
        self.grabs += 1;
        self.ops.push(Op::Grab);
        true
    }
    fn release(&mut self) {
        self.grabs -= 1;
        self.ops.push(Op::Release);
    }
    fn read_direct32(&mut self, reg: u32) -> u32 {
        *self.csr.get(&reg).unwrap_or(&0)
    }
    fn write_direct32(&mut self, reg: u32, val: u32) {
        self.csr.insert(reg, val);
        self.ops.push(Op::Direct32W(reg, val));
    }
    fn write8(&mut self, reg: u32, val: u8) {
        self.ops.push(Op::W8(reg, val));
    }
    fn write_prph64_no_grab(&mut self, addr: u32, val: u64) {
        self.ops.push(Op::Prph64W(addr, val));
    }
    fn write_mem(&mut self, addr: u32, words: &[u32]) -> Result<(), NicFault> {
        self.ops.push(Op::MemW(addr, words.to_vec()));
        Ok(())
    }
    fn zero_mem(&mut self, addr: u32, dwords: usize) -> Result<(), NicFault> {
        self.ops.push(Op::MemZero(addr, dwords));
        Ok(())
    }
    fn read_mem(&mut self, _addr: u32, out: &mut [u32]) -> Result<(), NicFault> {
        out.fill(0);
        Ok(())
    }
    fn fault(&self) -> Option<NicFault> {
        self.fail_grab.then_some(NicFault::AccessTimeout { gp_cntrl: 0 })
    }
}

/// A scripted BAR0 for `Nic<Bus>`: reads come from a map (with an optional per-read hook), writes are logged.
#[derive(Default)]
pub struct Bus {
    pub regs: HashMap<u32, u32>,
    pub log: Vec<(&'static str, u32, u32)>,
    pub slept_us: u64,
    /// MAC_CLOCK_READY never comes.
    pub wedge: bool,
}

impl RawBus for Bus {
    fn read32(&mut self, off: u32) -> u32 {
        let v = *self.regs.get(&off).unwrap_or(&0);
        if off == 0x24 && !self.wedge && v & 0x8 != 0 {
            return v | 1; // MAC_ACCESS_REQ -> MAC_CLOCK_READY
        }
        match off {
            0x450 => *self.regs.get(&(0x1_0000 | self.regs[&0x448] & 0xfffff)).unwrap_or(&0xabcd_0000),
            0x41c => {
                let a = self.regs[&0x40c];
                self.regs.insert(0x40c, a + 4);
                0x1000_0000 | a
            }
            _ => v,
        }
    }
    fn write32(&mut self, off: u32, val: u32) {
        self.log.push(("w32", off, val));
        self.regs.insert(off, val);
    }
    fn write8(&mut self, off: u32, val: u8) {
        self.log.push(("w8", off, val as u32));
    }
    fn sleep_us(&mut self, us: u32) {
        self.slept_us += us as u64;
    }
}
