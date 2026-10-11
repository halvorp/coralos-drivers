//! Published with the architect's ruling (Astra, 2026-10-11): capability-scoped PORT I/O. MIT OR Apache-2.0.
//!
//! WHAT A DRIVER HOLDS is a [`PortRange`] — an opaque handle to one port-range capability the LOADER granted before
//! exec, learned by asking the kernel (`PORT_IO` op [`OP_QUERY`]) which ranges this task holds. A driver never names an
//! absolute port to the kernel through this crate: every access is an OFFSET into a range it holds, checked here against
//! the range's length, direction and width, and checked again by the kernel against the capability itself. Exposing the
//! operation is not granting it — a task with no capability gets [`PortError::NoRange`], and a task with one still cannot
//! reach the port next to it, cross its end, widen the access, or write through a read-only grant.
//!
//! THE KERNEL ABI (system call 52, `nr::PORT_IO`): `PORT_IO(op = RDI, port_or_slot = RSI, value = RDX)`.
//!   op 0 IN8 · 1 OUT8 · 2 IN16 · 3 OUT16 · 4 IN32 · 5 OUT32 · 6 QUERY
//!   IN  returns the value read (zero-extended); OUT returns 0; any refusal returns `u64::MAX`
//!       (no capability covers [port, port + width) · the capability lacks the direction · it lacks the width · bad op).
//!   QUERY(slot) returns `base | len << 16 | rights << 32` for the capability in port slot `slot` (0..[`SLOTS`]),
//!       or `u64::MAX` when the slot is empty or out of range.
//!   rights: bit 0 IN · bit 1 OUT · bit 2 8-bit · bit 3 16-bit · bit 4 32-bit.

use crate::nr;

pub const OP_IN8: u64 = 0;
pub const OP_OUT8: u64 = 1;
pub const OP_IN16: u64 = 2;
pub const OP_OUT16: u64 = 3;
pub const OP_IN32: u64 = 4;
pub const OP_OUT32: u64 = 5;
pub const OP_QUERY: u64 = 6;
/// The kernel's refusal word, for every op.
pub const REFUSED: u64 = u64::MAX;
/// How many port-range capability slots a task has.
pub const SLOTS: u8 = 4;

/// Rights bits of a port-range capability.
pub mod rights {
    pub const IN: u8 = 1 << 0;
    pub const OUT: u8 = 1 << 1;
    pub const W8: u8 = 1 << 2;
    pub const W16: u8 = 1 << 3;
    pub const W32: u8 = 1 << 4;
}

/// Why an access was refused, by name. [`PortError::Refused`] is the KERNEL's refusal; the others are caught here first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortError {
    /// No range held in any slot covers the port asked for.
    NoRange,
    /// The offset plus the width crosses the end of the range (or overflows).
    OutOfRange,
    /// The capability does not carry that direction.
    Direction,
    /// The capability does not carry that width.
    Width,
    /// The kernel refused a call this crate let through.
    Refused,
}

impl PortError {
    pub const fn name(self) -> &'static str {
        match self {
            PortError::NoRange => "PORT-NO-RANGE",
            PortError::OutOfRange => "PORT-OUT-OF-RANGE",
            PortError::Direction => "PORT-DIRECTION",
            PortError::Width => "PORT-WIDTH",
            PortError::Refused => "PORT-KERNEL-REFUSED",
        }
    }
}

/// The transport to `PORT_IO`: the kernel in a driver, a model of it in a host test.
pub trait PortRaw {
    fn call(&mut self, op: u64, port_or_slot: u64, value: u64) -> u64;
}

/// `PORT_IO` over the real system call.
#[cfg(target_arch = "x86_64")]
pub struct Kernel;
#[cfg(target_arch = "x86_64")]
impl PortRaw for Kernel {
    fn call(&mut self, op: u64, a: u64, v: u64) -> u64 {
        unsafe { crate::sc(nr::PORT_IO, op, a, v, 0) }
    }
}

/// An opaque handle to ONE port-range capability. It has no public constructor but [`PortRange::open`] and
/// [`PortRange::find`], both of which ask the kernel what this task holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortRange {
    base: u16,
    len: u16,
    rights: u8,
}

const fn width_right(width: u16) -> u8 {
    match width {
        1 => rights::W8,
        2 => rights::W16,
        _ => rights::W32,
    }
}

impl PortRange {
    /// The capability in port slot `slot`, as the kernel reports it.
    pub fn open<R: PortRaw>(raw: &mut R, slot: u8) -> Result<PortRange, PortError> {
        if slot >= SLOTS {
            return Err(PortError::NoRange);
        }
        let w = raw.call(OP_QUERY, slot as u64, 0);
        if w == REFUSED {
            return Err(PortError::NoRange);
        }
        Ok(PortRange { base: w as u16, len: (w >> 16) as u16, rights: (w >> 32) as u8 })
    }

    /// The held range that covers `port` and carries every right in `need` (a [`rights`] mask).
    pub fn find<R: PortRaw>(raw: &mut R, port: u16, need: u8) -> Result<PortRange, PortError> {
        let mut covered = false;
        for slot in 0..SLOTS {
            if let Ok(r) = PortRange::open(raw, slot) {
                if r.len != 0 && port >= r.base && (port as u32) < r.base as u32 + r.len as u32 {
                    if r.rights & need == need {
                        return Ok(r);
                    }
                    covered = true;
                }
            }
        }
        Err(if covered { PortError::Direction } else { PortError::NoRange })
    }

    pub const fn len(&self) -> u16 {
        self.len
    }
    pub const fn rights(&self) -> u8 {
        self.rights
    }

    /// The checks this handle can make without the kernel; returns the absolute port.
    fn check(&self, off: u16, width: u16, dir: u8) -> Result<u16, PortError> {
        if self.rights & dir == 0 {
            return Err(PortError::Direction);
        }
        if self.rights & width_right(width) == 0 {
            return Err(PortError::Width);
        }
        if off as u32 + width as u32 > self.len as u32 {
            return Err(PortError::OutOfRange);
        }
        Ok(self.base + off)
    }

    fn input<R: PortRaw>(&self, raw: &mut R, op: u64, off: u16, width: u16) -> Result<u32, PortError> {
        let port = self.check(off, width, rights::IN)?;
        match raw.call(op, port as u64, 0) {
            REFUSED => Err(PortError::Refused),
            v => Ok(v as u32),
        }
    }

    fn output<R: PortRaw>(&self, raw: &mut R, op: u64, off: u16, width: u16, v: u32) -> Result<(), PortError> {
        let port = self.check(off, width, rights::OUT)?;
        match raw.call(op, port as u64, v as u64) {
            REFUSED => Err(PortError::Refused),
            _ => Ok(()),
        }
    }

    pub fn in8<R: PortRaw>(&self, raw: &mut R, off: u16) -> Result<u8, PortError> {
        self.input(raw, OP_IN8, off, 1).map(|v| v as u8)
    }
    pub fn out8<R: PortRaw>(&self, raw: &mut R, off: u16, v: u8) -> Result<(), PortError> {
        self.output(raw, OP_OUT8, off, 1, v as u32)
    }
    pub fn in16<R: PortRaw>(&self, raw: &mut R, off: u16) -> Result<u16, PortError> {
        self.input(raw, OP_IN16, off, 2).map(|v| v as u16)
    }
    pub fn out16<R: PortRaw>(&self, raw: &mut R, off: u16, v: u16) -> Result<(), PortError> {
        self.output(raw, OP_OUT16, off, 2, v as u32)
    }
    pub fn in32<R: PortRaw>(&self, raw: &mut R, off: u16) -> Result<u32, PortError> {
        self.input(raw, OP_IN32, off, 4)
    }
    pub fn out32<R: PortRaw>(&self, raw: &mut R, off: u16, v: u32) -> Result<(), PortError> {
        self.output(raw, OP_OUT32, off, 4, v)
    }
}

/// The range in a slot, over the real kernel.
#[cfg(target_arch = "x86_64")]
pub fn port_open(slot: u8) -> Result<PortRange, PortError> {
    PortRange::open(&mut Kernel, slot)
}
/// The held range covering `port` with the rights in `need`, over the real kernel.
#[cfg(target_arch = "x86_64")]
pub fn port_find(port: u16, need: u8) -> Result<PortRange, PortError> {
    PortRange::find(&mut Kernel, port, need)
}
/// `range`'s byte at `off`, over the real kernel.
#[cfg(target_arch = "x86_64")]
pub fn port_in8(range: &PortRange, off: u16) -> Result<u8, PortError> {
    range.in8(&mut Kernel, off)
}
/// Write `v` to `range`'s byte at `off`, over the real kernel.
#[cfg(target_arch = "x86_64")]
pub fn port_out8(range: &PortRange, off: u16, v: u8) -> Result<(), PortError> {
    range.out8(&mut Kernel, off, v)
}
#[cfg(target_arch = "x86_64")]
pub fn port_in16(range: &PortRange, off: u16) -> Result<u16, PortError> {
    range.in16(&mut Kernel, off)
}
#[cfg(target_arch = "x86_64")]
pub fn port_out16(range: &PortRange, off: u16, v: u16) -> Result<(), PortError> {
    range.out16(&mut Kernel, off, v)
}
#[cfg(target_arch = "x86_64")]
pub fn port_in32(range: &PortRange, off: u16) -> Result<u32, PortError> {
    range.in32(&mut Kernel, off)
}
#[cfg(target_arch = "x86_64")]
pub fn port_out32(range: &PortRange, off: u16, v: u32) -> Result<(), PortError> {
    range.out32(&mut Kernel, off, v)
}
