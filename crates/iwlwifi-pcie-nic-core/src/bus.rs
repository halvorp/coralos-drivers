// SPDX-License-Identifier: GPL-2.0-only
//! The raw BAR0 seam and the NIC-access handshake over it: [`Nic`] is the one implementation of
//! `iwlwifi-pcie-trans-core`'s [`TransIo`] a device needs. Ported from Linux
//! `pcie/gen1_2/trans.c:1881-1905` (`iwl_trans_pcie_prph_msk`, `_read_prph`, `_write_prph`),
//! `:2330-2411` (`_iwl_trans_pcie_grab_nic_access`), `:2427-2456` (`iwl_trans_pcie_release_nic_access`),
//! `:2459-2503` (`iwl_trans_pcie_read_mem`), `iwl-trans.c:464-481` (`iwl_trans_write_mem`),
//! `iwl-io.c:50-63, :66-95, :124-143` (`iwl_poll_bits_mask`, `iwl_read_direct32`,
//! `iwl_write_direct32`, `iwl_write_prph64_no_grab`, `iwl_read_prph`).
//!
//! Copyright (C) 2007-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! WHAT "GRAB" MEANS. A periphery or SRAM access, and a flow-handler (FH) register access, is only
//! defined while the MAC is awake: the host sets MAC_ACCESS_REQ, waits for MAC_CLOCK_READY with
//! GOING_TO_SLEEP clear, does the access, and clears the request. Linux does this per access
//! (`iwl_read_prph`, `iwl_write_direct32`) or once around a run of accesses
//! (`iwl_trans_grab_nic_access` + `*_no_grab`). [`Nic`] does both: [`Nic::grab`] nests (a counter),
//! and every periphery, SRAM and FH access grabs for itself — except the two shadow-register
//! windows Linux writes with plain `iwl_write32` (the RFH write-index trigger, `rx.c:199-201`).
//!
//! DEVIATION, named: a grab that times out FORCES AN NMI (`CSR_RESET ← FORCE_NMI`, trans.c:2397-2398)
//! and Linux returns false for that one access; a later access tries again. A device that did not wake
//! will not wake for the next register either, and each retry costs 15 ms and another NMI, so [`Nic`]
//! remembers the first fault ([`Nic::fault`]) and refuses every later grab until [`Nic::clear_fault`]
//! (the restart path). Linux gets the same effect from `STATUS_TRANS_DEAD` (trans.c:2339).

use crate::regs::*;
use iwlwifi_pcie_trans_core::io::{TransIo, IWL_POLL_INTERVAL};
use iwlwifi_pcie_trans_core::regs::{is_hw_error_value, CSR_GP_CNTRL, CSR_RESET};

/// `iwl_poll_bits_mask`'s timeout for the grab. // trans.c:2378
pub const GRAB_TIMEOUT_US: u32 = 15_000;
/// The settle after setting MAC_ACCESS_REQ on family >= 8000. // trans.c:2355-2356
pub const GRAB_SETTLE_US: u32 = 2;
/// What `iwl_read_prph` / `iwl_read_direct32` return when the grab failed. // iwl-io.c:76, :142
pub const READ_FAILED: u32 = 0x5a5a_5a5a;

/// BAR0, as bytes of registers: nothing else.
pub trait RawBus {
    /// `iwl_trans_pcie_read32`.
    fn read32(&mut self, off: u32) -> u32;
    /// `iwl_trans_pcie_write32`.
    fn write32(&mut self, off: u32, val: u32);
    /// `iwl_write8`: a single byte (CSR_INT_COALESCING, CSR_INT_PERIODIC_REG). // rx.c:893, :985
    fn write8(&mut self, off: u32, val: u8);
    /// `udelay` / `usleep_range`: at least `us` microseconds.
    fn sleep_us(&mut self, us: u32);
}

/// Why the NIC stopped answering. // trans.c:2380-2398
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NicFault {
    /// "Timeout waiting for hardware access (CSR_GP_CNTRL 0x%08x)". All-ones is a dead link.
    AccessTimeout { gp_cntrl: u32 },
}

/// The register and SRAM accessors the sequences above the transport use. Everything except
/// [`NicIo::write8`] and `csr_*` outside the FH range grabs for itself.
pub trait NicIo: TransIo {
    /// `iwl_trans_grab_nic_access`. Nests; the first grab does the handshake.
    fn grab(&mut self) -> bool;
    /// `iwl_trans_release_nic_access`.
    fn release(&mut self);
    /// `iwl_read_direct32` — grabs for itself.
    fn read_direct32(&mut self, reg: u32) -> u32;
    /// `iwl_write_direct32` — grabs for itself.
    fn write_direct32(&mut self, reg: u32, val: u32);
    /// `iwl_write8`.
    fn write8(&mut self, reg: u32, val: u8);
    /// `iwl_write_prph64_no_grab`: low dword at `addr`, high dword at `addr + 4`; the caller holds the grab.
    fn write_prph64_no_grab(&mut self, addr: u32, val: u64);
    /// `iwl_trans_write_mem`: `words` dwords to device SRAM at `addr`; `None` data writes zeroes.
    /// `Err` when the grab failed (-EBUSY).
    fn write_mem(&mut self, addr: u32, words: &[u32]) -> Result<(), NicFault>;
    /// `iwl_trans_write_mem` with a NULL buffer: `dwords` zero dwords. // iwl-trans.c:472-476
    fn zero_mem(&mut self, addr: u32, dwords: usize) -> Result<(), NicFault>;
    /// `iwl_trans_pcie_read_mem`.
    fn read_mem(&mut self, addr: u32, out: &mut [u32]) -> Result<(), NicFault>;
    /// The first fault seen, if any.
    fn fault(&self) -> Option<NicFault>;
}

#[derive(Debug)]
pub struct Nic<B: RawBus> {
    pub bus: B,
    held: u32,
    fault: Option<NicFault>,
}

/// A FH register is one of the 0x1000+ flow-handler registers. The RFH shadow write-index trigger
/// window (`RFH_Q_FRBDCB_WIDX_TRG(q)`, 0x1C80 + 4q, 31 queues at most) is written WITHOUT a grab —
/// that is what the shadow registers are for (CSR_MAC_SHADOW_REG_CTRL, trans.c:513-517).
/// // iwl-fh.h:337-338; rx.c:199-201
pub const RFH_WIDX_TRG_LO: u32 = 0x1C80; // iwl-fh.h:337
pub const RFH_WIDX_TRG_HI: u32 = 0x1C80 + 4 * 32;

/// Whether `iwl_write_direct32` (a grab) is what Linux uses for `reg`.
pub fn csr_needs_grab(reg: u32) -> bool {
    reg >= FH_MEM_LOWER_BOUND && !(RFH_WIDX_TRG_LO..RFH_WIDX_TRG_HI).contains(&reg)
}

impl<B: RawBus> Nic<B> {
    pub fn new(bus: B) -> Self {
        Nic { bus, held: 0, fault: None }
    }

    /// Forget a fault: the device was reset and may answer again. Drops any grab still counted.
    pub fn clear_fault(&mut self) {
        self.fault = None;
        self.held = 0;
    }

    pub fn grabs_held(&self) -> u32 {
        self.held
    }

    /// `iwl_poll_bits_mask`. // iwl-io.c:50-63
    fn poll_bits_mask(&mut self, reg: u32, bits: u32, mask: u32, timeout_us: u32) -> bool {
        let mut t = 0u32;
        loop {
            if self.bus.read32(reg) & mask == bits & mask {
                return true;
            }
            self.bus.sleep_us(IWL_POLL_INTERVAL);
            t += IWL_POLL_INTERVAL;
            if t >= timeout_us {
                return false;
            }
        }
    }

    fn do_grab(&mut self) -> bool {
        if self.fault.is_some() {
            return false; // STATUS_TRANS_DEAD, trans.c:2339
        }
        if self.held > 0 {
            self.held += 1;
            return true;
        }
        let v = self.bus.read32(CSR_GP_CNTRL);
        self.bus.write32(CSR_GP_CNTRL, v | CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ); // trans.c:2354
        self.bus.sleep_us(GRAB_SETTLE_US); // trans.c:2355-2356
        let mask = CSR_GP_CNTRL_REG_FLAG_MAC_CLOCK_READY | CSR_GP_CNTRL_REG_FLAG_GOING_TO_SLEEP; // trans.c:2335-2336
        if self.poll_bits_mask(CSR_GP_CNTRL, CSR_GP_CNTRL_REG_VAL_MAC_ACCESS_EN, mask, GRAB_TIMEOUT_US) {
            self.held = 1;
            return true;
        }
        let gp_cntrl = self.bus.read32(CSR_GP_CNTRL); // trans.c:2380
        self.bus.write32(CSR_RESET, CSR_RESET_REG_FLAG_FORCE_NMI); // trans.c:2397-2398
        self.fault = Some(NicFault::AccessTimeout { gp_cntrl });
        false
    }

    fn do_release(&mut self) {
        if self.held == 0 {
            return;
        }
        self.held -= 1;
        if self.held == 0 {
            let v = self.bus.read32(CSR_GP_CNTRL);
            self.bus.write32(CSR_GP_CNTRL, v & !CSR_GP_CNTRL_REG_FLAG_MAC_ACCESS_REQ); // trans.c:2445-2447
        }
    }

    fn prph_addr(addr: u32) -> u32 {
        (addr & PRPH_MASK_9000) | PRPH_DWORD_ACCESS // trans.c:1893-1894, :1902-1903
    }

    fn prph_read_no_grab(&mut self, addr: u32) -> u32 {
        self.bus.write32(HBUS_TARG_PRPH_RADDR, Self::prph_addr(addr));
        self.bus.read32(HBUS_TARG_PRPH_RDAT) // trans.c:1893-1895
    }

    fn prph_write_no_grab(&mut self, addr: u32, val: u32) {
        self.bus.write32(HBUS_TARG_PRPH_WADDR, Self::prph_addr(addr));
        self.bus.write32(HBUS_TARG_PRPH_WDAT, val); // trans.c:1902-1904
    }

    /// Whether the value came back as the bus-error pattern (Linux checks it in `read_mem`).
    pub fn is_hw_error(v: u32) -> bool {
        is_hw_error_value(v)
    }
}

impl<B: RawBus> TransIo for Nic<B> {
    fn csr_read(&mut self, reg: u32) -> u32 {
        if csr_needs_grab(reg) {
            return self.read_direct32(reg);
        }
        self.bus.read32(reg)
    }

    fn csr_write(&mut self, reg: u32, val: u32) {
        if csr_needs_grab(reg) {
            return self.write_direct32(reg, val);
        }
        self.bus.write32(reg, val);
    }

    fn prph_read(&mut self, addr: u32) -> u32 {
        if !self.do_grab() {
            return READ_FAILED; // iwl-io.c:141-142
        }
        let v = self.prph_read_no_grab(addr);
        self.do_release();
        v
    }

    fn prph_write(&mut self, addr: u32, val: u32) {
        if self.do_grab() {
            self.prph_write_no_grab(addr, val);
            self.do_release();
        }
    }

    fn sleep_us(&mut self, us: u32) {
        self.bus.sleep_us(us);
    }
}

impl<B: RawBus> NicIo for Nic<B> {
    fn grab(&mut self) -> bool {
        self.do_grab()
    }

    fn release(&mut self) {
        self.do_release()
    }

    fn read_direct32(&mut self, reg: u32) -> u32 {
        if !self.do_grab() {
            return READ_FAILED; // iwl-io.c:75-76
        }
        let v = self.bus.read32(reg);
        self.do_release();
        v
    }

    fn write_direct32(&mut self, reg: u32, val: u32) {
        if self.do_grab() {
            self.bus.write32(reg, val);
            self.do_release();
        }
    }

    fn write8(&mut self, reg: u32, val: u8) {
        self.bus.write8(reg, val);
    }

    fn write_prph64_no_grab(&mut self, addr: u32, val: u64) {
        self.prph_write_no_grab(addr, val as u32); // iwl-io.c:127
        self.prph_write_no_grab(addr + 4, (val >> 32) as u32); // iwl-io.c:128
    }

    fn write_mem(&mut self, addr: u32, words: &[u32]) -> Result<(), NicFault> {
        if !self.do_grab() {
            return Err(self.fault.unwrap_or(NicFault::AccessTimeout { gp_cntrl: 0 }));
        }
        self.bus.write32(HBUS_TARG_MEM_WADDR, addr); // iwl-trans.c:473
        for &w in words {
            self.bus.write32(HBUS_TARG_MEM_WDAT, w); // iwl-trans.c:474-476
        }
        self.do_release();
        Ok(())
    }

    fn zero_mem(&mut self, addr: u32, dwords: usize) -> Result<(), NicFault> {
        if !self.do_grab() {
            return Err(self.fault.unwrap_or(NicFault::AccessTimeout { gp_cntrl: 0 }));
        }
        self.bus.write32(HBUS_TARG_MEM_WADDR, addr);
        for _ in 0..dwords {
            self.bus.write32(HBUS_TARG_MEM_WDAT, 0); // `vals ? vals[offs] : 0`, iwl-trans.c:476
        }
        self.do_release();
        Ok(())
    }

    fn read_mem(&mut self, addr: u32, out: &mut [u32]) -> Result<(), NicFault> {
        if !self.do_grab() {
            return Err(self.fault.unwrap_or(NicFault::AccessTimeout { gp_cntrl: 0 }));
        }
        self.bus.write32(HBUS_TARG_MEM_RADDR, addr); // trans.c:2473-2474
        for slot in out.iter_mut() {
            *slot = self.bus.read32(HBUS_TARG_MEM_RDAT); // trans.c:2477
        }
        self.do_release();
        Ok(())
    }

    fn fault(&self) -> Option<NicFault> {
        self.fault
    }
}
