// SPDX-License-Identifier: GPL-2.0-only
//! The MMIO write guard: a `coralos_hal::Mmio` that forwards READS and refuses every WRITE whose register is not on a
//! `&'static` allow-list. *Exposing an operation is not granting it* (D7/D8): the guard is the in-code half of the
//! grant, the loader's window grant (rt `WriteWindow`) is the hardware-facing half.
//!
//! `Mmio::w32` cannot return an error, so a refused write is NOT forwarded and is LATCHED: [`WriteGuard::first_refusal`]
//! names the first offset refused, [`WriteGuard::refusals`] counts them. A caller checks the latch after each step and
//! stops the device on a refusal (`I915Error::WriteRefused`). 8- and 16-bit writes are always refused: Linux writes these
//! registers 32 bits wide (`fw_set` / `fw_clear`, intel_uncore.c:135-136).
use crate::error::I915Error;
use crate::reg::Reg;
use coralos_hal::Mmio;

/// One refused write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WriteRefusal {
    pub offset: u32,
    pub width: u8,
    pub value: u32,
}

impl WriteRefusal {
    pub const fn error(&self) -> I915Error {
        I915Error::WriteRefused { offset: self.offset, width: self.width }
    }
}

/// `inner` with a write allow-list.
pub struct WriteGuard<M: Mmio> {
    inner: M,
    allow: &'static [Reg],
    first: Option<WriteRefusal>,
    refused: u32,
    forwarded: u32,
}

impl<M: Mmio> WriteGuard<M> {
    pub const fn new(inner: M, allow: &'static [Reg]) -> Self {
        WriteGuard { inner, allow, first: None, refused: 0, forwarded: 0 }
    }
    /// Is a 32-bit write to `offset` on the allow-list?
    pub fn allows(&self, offset: u32) -> bool {
        self.allow.iter().any(|r| r.offset() == offset)
    }
    pub fn first_refusal(&self) -> Option<WriteRefusal> {
        self.first
    }
    /// How many writes were refused (and not forwarded).
    pub fn refusals(&self) -> u32 {
        self.refused
    }
    /// How many writes were forwarded to the device.
    pub fn writes(&self) -> u32 {
        self.forwarded
    }
    /// `Err` with the first refusal if any write was refused.
    pub fn check(&self) -> Result<(), I915Error> {
        match self.first {
            Some(r) => Err(r.error()),
            None => Ok(()),
        }
    }
    /// Ends the guard and returns the device handle. There is deliberately no `&mut M` accessor: a borrow of the raw
    /// handle while the guard lives would let a caller write past the allow-list (sentinel WRAPPER-CLOSES-HOLE, 2026-10-12).
    pub fn into_inner(self) -> M {
        self.inner
    }
    fn refuse(&mut self, offset: u32, width: u8, value: u32) {
        self.refused += 1;
        if self.first.is_none() {
            self.first = Some(WriteRefusal { offset, width, value });
        }
    }
}

impl<M: Mmio> Mmio for WriteGuard<M> {
    fn r8(&mut self, reg: u32) -> u8 {
        self.inner.r8(reg)
    }
    fn r16(&mut self, reg: u32) -> u16 {
        self.inner.r16(reg)
    }
    fn r32(&mut self, reg: u32) -> u32 {
        self.inner.r32(reg)
    }
    fn w8(&mut self, reg: u32, v: u8) {
        self.refuse(reg, 1, v as u32);
    }
    fn w16(&mut self, reg: u32, v: u16) {
        self.refuse(reg, 2, v as u32);
    }
    fn w32(&mut self, reg: u32, v: u32) {
        if self.allows(reg) {
            self.forwarded += 1;
            self.inner.w32(reg, v);
        } else {
            self.refuse(reg, 4, v);
        }
    }
    fn relax(&mut self) {
        self.inner.relax()
    }
    fn arm_deadline(&mut self, ms: u64) {
        self.inner.arm_deadline(ms)
    }
    fn deadline_expired(&mut self) -> bool {
        self.inner.deadline_expired()
    }
    fn park_ms(&mut self, ms: u64) {
        self.inner.park_ms(ms)
    }
}
