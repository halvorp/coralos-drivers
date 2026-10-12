//! Read-only, grant-bounded access to a device window (MIT OR Apache-2.0, like the rest of this crate).
//!
//! The loader endows a task with device windows before exec and numbers them in "slots" (`MMIO_MAP` returns a window's
//! virtual address, `MMIO_LEN` its exact length; both are in the ABI list at the top of lib.rs). [`ReadWindow`] is what a
//! driver holds for one slot. It exposes READS ONLY: *exposing an operation is not granting it*, and a read-only
//! bring-up (a state readout, an identity check) must not be one typo away from a write. Every access is an OFFSET into
//! the granted length, checked here before any dereference: an offset at or past the end, or not aligned to its
//! width, is a named refusal and nothing is read.
//!
//! The virtual address the kernel returns is validated before it is believed: `0` is the kernel's "no window", and an
//! address in the upper (kernel) half of the 48-bit address space cannot be a user mapping, so it is an error word,
//! not a base.

use crate::nr;

/// The first address that cannot be a user-space mapping (the canonical upper half starts at 2^47 for 4-level paging).
pub const USER_VA_END: u64 = 1 << 47;

/// Why a window could not be opened or a read was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmioError {
    /// `MMIO_LEN` says the slot holds nothing.
    NoWindow { slot: u32 },
    /// The window is shorter than the registers this driver needs.
    TooShort { slot: u32, len: u64, need: u64 },
    /// `MMIO_MAP` returned 0 or an error word.
    MapFailed { slot: u32, word: u64 },
    /// The offset plus the access width runs past the end of the window.
    OutOfRange { off: u64, width: u64, len: u64 },
    /// The offset is not a multiple of the access width.
    Misaligned { off: u64, width: u64 },
}

impl MmioError {
    pub const fn name(&self) -> &'static str {
        match self {
            MmioError::NoWindow { .. } => "MMIO-NO-WINDOW",
            MmioError::TooShort { .. } => "MMIO-TOO-SHORT",
            MmioError::MapFailed { .. } => "MMIO-MAP-FAILED",
            MmioError::OutOfRange { .. } => "MMIO-OUT-OF-RANGE",
            MmioError::Misaligned { .. } => "MMIO-MISALIGNED",
        }
    }
}

/// The transport to `MMIO_MAP` / `MMIO_LEN`: the kernel in a driver, a model of it in a host test.
pub trait MmioRaw {
    /// The window's virtual address (`MMIO_MAP`), or 0.
    fn map(&mut self, slot: u32) -> u64;
    /// The window's exact length (`MMIO_LEN`), 0 when the slot is empty.
    fn len(&mut self, slot: u32) -> u64;
}

/// The two system calls, for real.
#[cfg(target_arch = "x86_64")]
pub struct Kernel;
#[cfg(target_arch = "x86_64")]
impl MmioRaw for Kernel {
    fn map(&mut self, slot: u32) -> u64 {
        unsafe { crate::sc(nr::MMIO_MAP, slot as u64, 0, 0, 0) }
    }
    fn len(&mut self, slot: u32) -> u64 {
        unsafe { crate::sc(nr::MMIO_LEN, slot as u64, 0, 0, 0) }
    }
}

/// A mapped device window, readable and nothing else.
#[derive(Debug)]
pub struct ReadWindow {
    base: u64,
    len: u64,
}

impl ReadWindow {
    /// Map the window in `slot`, requiring at least `need` bytes of it.
    pub fn open<R: MmioRaw>(raw: &mut R, slot: u32, need: u64) -> Result<ReadWindow, MmioError> {
        let len = raw.len(slot);
        if len == 0 {
            return Err(MmioError::NoWindow { slot });
        }
        if len < need {
            return Err(MmioError::TooShort { slot, len, need });
        }
        let base = raw.map(slot);
        if base == 0 || base >= USER_VA_END {
            return Err(MmioError::MapFailed { slot, word: base });
        }
        Ok(ReadWindow { base, len })
    }

    /// The granted length in bytes.
    pub const fn len(&self) -> u64 {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn check(&self, off: u64, width: u64) -> Result<u64, MmioError> {
        if off % width != 0 {
            return Err(MmioError::Misaligned { off, width });
        }
        match off.checked_add(width) {
            Some(end) if end <= self.len => Ok(self.base + off),
            _ => Err(MmioError::OutOfRange { off, width, len: self.len }),
        }
    }

    pub fn read8(&self, off: u64) -> Result<u8, MmioError> {
        let va = self.check(off, 1)?;
        // SAFETY: `check` proved [off, off + 1) lies inside the window the kernel granted and mapped at `base`.
        Ok(unsafe { core::ptr::read_volatile(va as *const u8) })
    }
    pub fn read16(&self, off: u64) -> Result<u16, MmioError> {
        let va = self.check(off, 2)?;
        // SAFETY: as `read8`; `check` also proved the offset 2-byte aligned (the base is page aligned).
        Ok(unsafe { core::ptr::read_volatile(va as *const u16) })
    }
    pub fn read32(&self, off: u64) -> Result<u32, MmioError> {
        let va = self.check(off, 4)?;
        // SAFETY: as `read8`; 4-byte aligned.
        Ok(unsafe { core::ptr::read_volatile(va as *const u32) })
    }
}
