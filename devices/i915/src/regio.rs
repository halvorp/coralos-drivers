// SPDX-License-Identifier: GPL-2.0-only
//! The two windows the device holds, behind tiny traits so a host test can stand a register snapshot in for BAR0.
//!
//! The loader's convention for a PCI function's endowment (the same one every CoralOS PCI `.device` gets): the
//! function's BAR0 window in MMIO slot 0, and its PCI configuration page, READ-only, in slot 1. The configuration page is a
//! [`ReadWindow`] and has no write. BAR0 is a [`WriteWindow`] whose write allow-list is [`BAR0_WRITE_RANGES`]: the three forcewake
//! SET registers and nothing else in the default build (`exec::allow`: the `p2-memory` / `p3-exec` features extend it).
use coralos_hal::Mmio;
use coralos_rt::mmio::{MmioError, ReadWindow, WriteWindow};

/// MMIO slot of BAR0 (the GTTMMADR window: registers, then the GGTT).
pub const BAR0_SLOT: u32 = 0;
/// MMIO slot of the PCI configuration page (read-only).
pub const CFG_SLOT: u32 = 1;
/// The PCI configuration header the identity check reads: 64 bytes (PCI local bus spec header type 0).
pub const CFG_HEADER_BYTES: u64 = 0x40;
/// The configuration bytes the device needs: the 64-byte header, and with `p3-exec` also `GMCH_CTRL` (offset 0x50, the GGTT table size).
pub const CFG_NEED_BYTES: u64 = if cfg!(feature = "p3-exec") { GMCH_CTRL_DWORD as u64 + 4 } else { CFG_HEADER_BYTES };
/// The dword holding `GMCH_CTRL` (a 16-bit register at PCI configuration offset 0x50, `SNB_GMCH_CTRL`, include/drm/intel/i915_drm.h:49; read with `pci_read_config_word`, gt/intel_ggtt.c:1274).
pub const GMCH_CTRL_DWORD: u32 = 0x50;
/// The part of BAR0 the Kaby Lake display readout reads: every register it touches is below 0x80000
/// (TRANSCONF(EDP) at 0x7f008 is the highest).
pub const READOUT_WINDOW_BYTES: u64 = 0x8_0000;
pub use crate::exec::allow::{BAR0_NEED_BYTES, BAR0_WRITE_RANGES};

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
    /// Yield the CPU for a moment (`Mmio::relax`): a poll that is waiting calls this between reads and never spins. The default
    /// does nothing, which is a spin; the device's real clock overrides it with a timed wait.
    fn relax(&mut self) {}
    /// Sleep at least `ms` milliseconds (`Mmio::park_ms`). The default yields with [`Clock::relax`] until the clock has moved on,
    /// capped so a clock that never advances still returns.
    fn park_ms(&mut self, ms: u64) {
        let end = self.now_ms().saturating_add(ms);
        let mut n = 0u32;
        while self.now_ms() < end && n < PARK_SPIN_CAP {
            self.relax();
            n += 1;
        }
    }
}

/// The most `relax` calls the default [`Clock::park_ms`] makes.
pub const PARK_SPIN_CAP: u32 = 100_000;

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
    fn relax(&mut self) {
        self.clock.relax()
    }
    fn park_ms(&mut self, ms: u64) {
        self.clock.park_ms(ms)
    }
}

impl<C: Clock> Bar for BarMmio<C> {
    fn take_fault(&mut self) -> Option<MmioError> {
        self.fault.take()
    }
}
