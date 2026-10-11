// SPDX-License-Identifier: GPL-2.0-only
//! The transport's register-access seam and the read-modify-write / poll helpers built on it, ported
//! from Linux `drivers/net/wireless/intel/iwlwifi/iwl-io.c:48-63, :156-189` and `iwl-io.h:26-31`.
//!
//! Copyright (C) 2003-2014, 2018-2022, 2024-2025 Intel Corporation; Copyright (C) 2015-2016 Intel
//! Deutschland GmbH.
//!
//! The service owns the BAR mapping, the NIC-access grab and the clock. `prph_*` are the indirect
//! periphery accesses (`iwl_read_prph` / `iwl_write_prph`, HBUS_TARG_PRPH_*); on the 9000 family the
//! UMAC periphery offset is 0 (`umac_prph_offset` is set only in cfg/bz.c:81, :93), so the
//! `iwl_*_umac_prph` accessors are the plain ones.

/// The four register accesses and the one wait every sequence here needs.
pub trait TransIo {
    /// `iwl_read32` (BAR0 CSR / FH).
    fn csr_read(&mut self, reg: u32) -> u32;
    /// `iwl_write32`.
    fn csr_write(&mut self, reg: u32, val: u32);
    /// `iwl_read_prph`.
    fn prph_read(&mut self, addr: u32) -> u32;
    /// `iwl_write_prph`.
    fn prph_write(&mut self, addr: u32, val: u32);
    /// `udelay` / `usleep_range(us, …)` / `msleep`: at least `us` microseconds.
    fn sleep_us(&mut self, us: u32);
}

/// The poll step of every `iwl_poll_*` loop. // iwl-io.c:48
pub const IWL_POLL_INTERVAL: u32 = 10; // iwl-io.c:48

/// `iwl_set_bit`: read-modify-write OR.
pub fn set_bit<I: TransIo + ?Sized>(io: &mut I, reg: u32, mask: u32) {
    let v = io.csr_read(reg);
    io.csr_write(reg, v | mask);
}

/// `iwl_clear_bit`: read-modify-write AND-NOT.
pub fn clear_bit<I: TransIo + ?Sized>(io: &mut I, reg: u32, mask: u32) {
    let v = io.csr_read(reg);
    io.csr_write(reg, v & !mask);
}

/// `iwl_set_bits_prph`. // iwl-io.c:171-178
pub fn set_bits_prph<I: TransIo + ?Sized>(io: &mut I, addr: u32, mask: u32) {
    let v = io.prph_read(addr);
    io.prph_write(addr, v | mask);
}

/// `iwl_poll_bits(trans, addr, bits, timeout)` = `iwl_poll_bits_mask(…, bits, bits, …)`: true once
/// every bit of `bits` reads set, checking every IWL_POLL_INTERVAL µs while `t < timeout_us`.
/// // iwl-io.h:28-31; iwl-io.c:50-63
pub fn poll_bits<I: TransIo + ?Sized>(io: &mut I, reg: u32, bits: u32, timeout_us: u32) -> bool {
    let mut t = 0u32;
    loop {
        if io.csr_read(reg) & bits == bits {
            return true;
        }
        io.sleep_us(IWL_POLL_INTERVAL);
        t += IWL_POLL_INTERVAL;
        if t >= timeout_us {
            return false;
        }
    }
}

/// `iwl_poll_prph_bit(trans, addr, bits, mask, timeout)`. // iwl-io.c:156-169
pub fn poll_prph_bit<I: TransIo + ?Sized>(io: &mut I, addr: u32, bits: u32, mask: u32, timeout_us: u32) -> bool {
    let mut t = 0u32;
    loop {
        if io.prph_read(addr) & mask == bits & mask {
            return true;
        }
        io.sleep_us(IWL_POLL_INTERVAL);
        t += IWL_POLL_INTERVAL;
        if t >= timeout_us {
            return false;
        }
    }
}
