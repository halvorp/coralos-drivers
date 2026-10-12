// SPDX-License-Identifier: GPL-2.0-only
//! The two windows the device reads, behind one tiny trait so a host test can stand a register snapshot in for BAR0.
//!
//! The loader's convention for a PCI function's endowment (the same one every CoralOS PCI `.device` gets): the
//! function's BAR0 window in MMIO slot 0, and its PCI configuration page, READ-only, in slot 1. Neither is writable
//! from here: [`ReadWindow`] has no write.
use coralos_rt::mmio::{MmioError, ReadWindow};

/// MMIO slot of BAR0 (the GTTMMADR window: registers, then the GGTT).
pub const BAR0_SLOT: u32 = 0;
/// MMIO slot of the PCI configuration page (read-only).
pub const CFG_SLOT: u32 = 1;
/// The PCI configuration header the identity check reads: 64 bytes (PCI local bus spec header type 0).
pub const CFG_HEADER_BYTES: u64 = 0x40;
/// The part of BAR0 the Kaby Lake display readout reads: every register it touches is below 0x80000
/// (TRANSCONF(EDP) at 0x7f008 is the highest).
pub const READOUT_WINDOW_BYTES: u64 = 0x8_0000;

/// A readable register window.
pub trait RegRead {
    /// Granted length in bytes.
    fn len(&self) -> u64;
    fn read32(&mut self, off: u32) -> Result<u32, MmioError>;
}

impl RegRead for ReadWindow {
    fn len(&self) -> u64 {
        ReadWindow::len(self)
    }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        ReadWindow::read32(self, off as u64)
    }
}
