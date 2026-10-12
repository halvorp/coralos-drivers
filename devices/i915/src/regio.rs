// SPDX-License-Identifier: GPL-2.0-only
//! The two windows the device holds, behind tiny traits so a host test can stand a register snapshot in for BAR0.
//!
//! The loader's convention for a PCI function's endowment (the same one every CoralOS PCI `.device` gets): the
//! function's BAR0 window in MMIO slot 0, and its PCI configuration page, READ-only, in slot 1. The configuration page is a
//! [`ReadWindow`] and has no write. BAR0 is a [`WriteWindow`] whose write allow-list is [`BAR0_WRITE_RANGES`]: the three forcewake
//! SET registers and nothing else (increment 1, `crates/i915-uncore-core::WRITE_ALLOWLIST`).
use coralos_hal::Mmio;
use coralos_rt::mmio::{MmioError, ReadWindow, WriteRange, WriteWindow};
use i915_uncore_core::WRITE_ALLOWLIST;

/// MMIO slot of BAR0 (the GTTMMADR window: registers, then the GGTT).
pub const BAR0_SLOT: u32 = 0;
/// MMIO slot of the PCI configuration page (read-only).
pub const CFG_SLOT: u32 = 1;
/// The PCI configuration header the identity check reads: 64 bytes (PCI local bus spec header type 0).
pub const CFG_HEADER_BYTES: u64 = 0x40;
/// The part of BAR0 the Kaby Lake display readout reads: every register it touches is below 0x80000
/// (TRANSCONF(EDP) at 0x7f008 is the highest).
pub const READOUT_WINDOW_BYTES: u64 = 0x8_0000;
/// The part of BAR0 the whole device touches: up to and including GEN6_RP_STATE_CAP (0x145998, intel_mchbar_regs.h:225) and the
/// GT forcewake ack (0x130044). Every register used is below this.
pub const BAR0_NEED_BYTES: u64 = 0x14_59a0;

/// BAR0's MMIO write allow-list: exactly the three Gen9 forcewake SET registers (0xa188, 0xa270, 0xa278), built from the uncore
/// crate's own list so the two cannot drift. Each entry is one 32-bit register.
pub static BAR0_WRITE_RANGES: [WriteRange; 3] = [
    WriteRange::reg32(WRITE_ALLOWLIST[0].offset()),
    WriteRange::reg32(WRITE_ALLOWLIST[1].offset()),
    WriteRange::reg32(WRITE_ALLOWLIST[2].offset()),
];

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

impl RegRead for WriteWindow {
    fn len(&self) -> u64 {
        WriteWindow::len(self)
    }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        WriteWindow::read32(self, off as u64)
    }
}

/// BAR0 as the proof of life needs it: readable by [`RegRead`] (the display readout), accessible as a `coralos_hal::Mmio` (the
/// forcewake protocol), and able to say whether any access since the last check was refused by the window.
pub trait Bar: RegRead + Mmio {
    /// The first access the window refused since the last call (out of range, misaligned, or a write outside the allow-list), taken.
    fn take_fault(&mut self) -> Option<MmioError>;
}

/// The window's raw clock: milliseconds from a monotonic counter (the kernel's `TICKS` system call, which `kernel/.../apic.rs`
/// documents as milliseconds). A host test supplies its own.
pub trait Clock {
    fn now_ms(&mut self) -> u64;
}

/// BAR0 as a [`Bar`] over a [`WriteWindow`]. `coralos_hal::Mmio` cannot return an error, so a refused access is LATCHED
/// ([`Bar::take_fault`]) and a refused read returns all-ones - which the uncore treats as a dead BAR, never as data.
pub struct BarMmio<C: Clock> {
    win: WriteWindow,
    clock: C,
    fault: Option<MmioError>,
    deadline_ms: Option<u64>,
}

impl<C: Clock> BarMmio<C> {
    pub fn new(win: WriteWindow, clock: C) -> Self {
        BarMmio { win, clock, fault: None, deadline_ms: None }
    }
    fn latch(&mut self, e: MmioError) {
        if self.fault.is_none() {
            self.fault = Some(e);
        }
    }
}

impl<C: Clock> RegRead for BarMmio<C> {
    fn len(&self) -> u64 {
        self.win.len()
    }
    fn read32(&mut self, off: u32) -> Result<u32, MmioError> {
        self.win.read32(off as u64)
    }
}

impl<C: Clock> Mmio for BarMmio<C> {
    fn r8(&mut self, reg: u32) -> u8 {
        self.win.read8(reg as u64).unwrap_or_else(|e| {
            self.latch(e);
            u8::MAX
        })
    }
    fn r16(&mut self, reg: u32) -> u16 {
        self.win.read16(reg as u64).unwrap_or_else(|e| {
            self.latch(e);
            u16::MAX
        })
    }
    fn r32(&mut self, reg: u32) -> u32 {
        self.win.read32(reg as u64).unwrap_or_else(|e| {
            self.latch(e);
            u32::MAX
        })
    }
    fn w8(&mut self, reg: u32, _v: u8) {
        self.latch(MmioError::WriteNotAllowed { off: reg as u64, width: 1 });
    }
    fn w16(&mut self, reg: u32, _v: u16) {
        self.latch(MmioError::WriteNotAllowed { off: reg as u64, width: 2 });
    }
    fn w32(&mut self, reg: u32, v: u32) {
        if let Err(e) = self.win.write32(reg as u64, v) {
            self.latch(e);
        }
    }
    fn arm_deadline(&mut self, ms: u64) {
        self.deadline_ms = Some(self.clock.now_ms().saturating_add(ms));
    }
    fn deadline_expired(&mut self) -> bool {
        match self.deadline_ms {
            Some(d) => self.clock.now_ms() >= d,
            None => false,
        }
    }
}

impl<C: Clock> Bar for BarMmio<C> {
    fn take_fault(&mut self) -> Option<MmioError> {
        self.fault.take()
    }
}
