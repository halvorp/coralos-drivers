// SPDX-License-Identifier: GPL-2.0-only
//! The [`Rig`]: the machine plus the three allow-lists, and the only way the rest of the device touches hardware. A register write passes
//! [`Allow::check`], an I2C transfer passes [`I2cAllow::check`], a pad write passes [`PadAllow`]; each refusal is a named error, is
//! counted, and reaches the hardware not at all. Reads of granted windows are unrestricted.
use crate::allow::{Allow, I2cAllow, I2cRefusal, PadAllow};
use crate::machine::{I2cFault, Machine, PadFault, PadReg, RegFault, Want, Wake};
use avs_fw_core::effect::{Reg, Space, Width};

/// How many accesses the fences refused (a nonzero count is a bug in a plan or in a caller, and the status says so).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Refusals {
    pub reg: u32,
    pub i2c: u32,
    pub pad: u32,
}

pub struct Rig<M: Machine> {
    pub m: M,
    pub allow: Allow,
    pub i2c: I2cAllow,
    pub pads: PadAllow,
    pub refused: Refusals,
    /// The first refusal's name, kept for the status line.
    pub first_refusal: Option<&'static str>,
}

impl<M: Machine> Rig<M> {
    pub fn new(m: M, allow: Allow, i2c: I2cAllow, pads: PadAllow) -> Self {
        Self { m, allow, i2c, pads, refused: Refusals::default(), first_refusal: None }
    }

    fn note(&mut self, what: &'static str) {
        if self.first_refusal.is_none() {
            self.first_refusal = Some(what);
        }
    }

    pub fn read(&mut self, reg: Reg) -> Result<u32, RegFault> {
        self.m.reg_read(reg.space, reg.off, reg.width)
    }

    pub fn write(&mut self, reg: Reg, value: u32) -> Result<(), RegFault> {
        if let Err(e) = self.allow.check(reg) {
            self.refused.reg += 1;
            self.note(e.name());
            return Err(e);
        }
        self.m.reg_write(reg.space, reg.off, reg.width, value)
    }

    /// `reg = (reg & !mask) | value`, at the register's own width.
    pub fn update(&mut self, reg: Reg, mask: u32, value: u32) -> Result<(), RegFault> {
        // refuse BEFORE the read so a refused update leaves no trace on a read-sensitive register
        if let Err(e) = self.allow.check(reg) {
            self.refused.reg += 1;
            self.note(e.name());
            return Err(e);
        }
        let cur = self.read(reg)?;
        self.write(reg, (cur & !mask) | (value & mask))
    }

    pub fn read32(&mut self, space: Space, off: u32) -> Result<u32, RegFault> {
        self.read(Reg { space, off, width: Width::D32 })
    }

    pub fn now(&mut self) -> u64 {
        self.m.now_ms()
    }

    /// Block for at least `ms` milliseconds, waking for nothing. A BLOCKING timed wait, never a spin. It is the named exception for a bounded
    /// hardware wait that has no wakeup source (a codec settling delay, a register that settles in microseconds, an I2C status with no line
    /// granted): the same shape as i915-display-link's `Effect::DelayUs` / `PollPlan`. Where an interrupt exists the code blocks on it instead
    /// (the cAVS pump, the I2C controller's line); the bound is the plan's own and is enforced twice (its clock budget and its read count).
    pub fn sleep_ms(&mut self, ms: u64) {
        if ms == 0 {
            return;
        }
        let end = self.m.now_ms().saturating_add(ms);
        loop {
            let _ = self.m.wait(Some(end), Want::NOTHING);
            if self.m.now_ms() >= end {
                return;
            }
        }
    }

    /// Microsecond delays are served at the clock's millisecond granularity, rounded UP: never shorter than asked.
    pub fn sleep_us(&mut self, us: u32) {
        self.sleep_ms((us as u64).div_ceil(1000));
    }

    pub fn i2c_write(&mut self, addr: u16, data: &[u8]) -> Result<(), I2cFault> {
        self.gate_i2c(addr, data, 0)?;
        self.m.i2c_write(addr, data)
    }

    pub fn i2c_write_read(&mut self, addr: u16, wr: &[u8], rd: &mut [u8]) -> Result<(), I2cFault> {
        self.gate_i2c(addr, wr, rd.len())?;
        self.m.i2c_write_read(addr, wr, rd)
    }

    fn gate_i2c(&mut self, addr: u16, wr: &[u8], read_len: usize) -> Result<(), I2cFault> {
        match self.i2c.check_msg(addr, wr, read_len) {
            Ok(()) => Ok(()),
            Err(r) => {
                self.refused.i2c += 1;
                self.note(match r {
                    I2cRefusal::SpeakerAmplifier { .. } => "I2C-SPEAKER-AMPLIFIER-REFUSED",
                    I2cRefusal::NotTheCodec { .. } => "I2C-NOT-THE-CODEC",
                    I2cRefusal::AmplifierRegister { .. } => "I2C-AMPLIFIER-REGISTER-REFUSED",
                    I2cRefusal::AmplifierShape { .. } => "I2C-AMPLIFIER-SHAPE-REFUSED",
                });
                Err(I2cFault::AddressRefused { addr })
            }
        }
    }

    pub fn pad_read(&mut self, reg: PadReg) -> Result<u32, PadFault> {
        self.m.pad_read(reg)
    }

    pub fn pad_write(&mut self, reg: PadReg, value: u32) -> Result<(), PadFault> {
        if !self.pads.permits_write(reg) {
            self.refused.pad += 1;
            self.note("PAD-WRITE-NOT-ALLOWED");
            return Err(PadFault::NotAllowed { community: reg.community, off: reg.off });
        }
        self.m.pad_write(reg, value)
    }

    /// Block until `deadline_ms` or the first wanted wake.
    pub fn wait(&mut self, deadline_ms: Option<u64>, want: Want) -> Wake {
        self.m.wait(deadline_ms, want)
    }
}
