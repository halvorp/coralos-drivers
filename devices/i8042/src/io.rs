// SPDX-License-Identifier: GPL-2.0-only
//! The controller's two registers over the two port-range capabilities the loader granted, and a delay.
use coralos_rt::portio::{PortError, PortRaw, PortRange};
use i8042_core::regs::{COMMAND_REG, DATA_REG, STATUS_REG};
use i8042_core::Port;

/// The legacy controller's absolute ports (i8042-io.h:36-38): the device asks for the ranges covering these and nothing
/// else; the cores see only register offsets.
pub const DATA_PORT: u16 = 0x60;
pub const STATUS_COMMAND_PORT: u16 = 0x64;

/// A microsecond delay: a calibrated TSC spin in the device, a counter in a test.
pub trait Delay {
    fn us(&mut self, us: u32);
}

/// i8042-core's `Port` over two byte ranges. A refusal from the kernel reads as 0xFF (a floating bus) and is COUNTED, so
/// the controller bring-up sees "no controller" rather than a panic, and the device can name it.
pub struct KbPort<R: PortRaw, D: Delay> {
    pub raw: R,
    data: PortRange,
    cmd: PortRange,
    pub delay: D,
    /// The last refusal and how many there were.
    pub refusals: u32,
    pub last: Option<PortError>,
}

impl<R: PortRaw, D: Delay> KbPort<R, D> {
    /// Find the two ranges (read+write, byte) among the capabilities this task holds.
    pub fn open(mut raw: R, delay: D) -> Result<Self, PortError> {
        use coralos_rt::portio::rights::{IN, OUT, W8};
        let data = PortRange::find(&mut raw, DATA_PORT, IN | OUT | W8)?;
        let cmd = PortRange::find(&mut raw, STATUS_COMMAND_PORT, IN | OUT | W8)?;
        Ok(KbPort { raw, data, cmd, delay, refusals: 0, last: None })
    }
    fn note(&mut self, e: PortError) {
        self.refusals += 1;
        self.last = Some(e);
    }
}

impl<R: PortRaw, D: Delay> Port for KbPort<R, D> {
    fn inb(&mut self, reg: u16) -> u8 {
        let r = if reg == DATA_REG { self.data.in8(&mut self.raw, 0) } else { debug_assert_eq!(reg, STATUS_REG); self.cmd.in8(&mut self.raw, 0) };
        r.unwrap_or_else(|e| {
            self.note(e);
            0xFF
        })
    }
    fn outb(&mut self, reg: u16, value: u8) {
        let r = if reg == DATA_REG { self.data.out8(&mut self.raw, 0, value) } else { debug_assert_eq!(reg, COMMAND_REG); self.cmd.out8(&mut self.raw, 0, value) };
        if let Err(e) = r {
            self.note(e);
        }
    }
    fn udelay(&mut self, us: u32) {
        self.delay.us(us);
    }
}
