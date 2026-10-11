// SPDX-License-Identifier: GPL-2.0-only
//! The i8042 keyboard controller and the AT keyboard's command protocol, without hardware access.
//!
//! Ported mechanically from Linux:
//! - `drivers/input/serio/i8042.c`, `drivers/input/serio/i8042.h`, `include/linux/i8042.h` — the
//!   controller: bounded waits, flush, command encoding, self-test, CTR init, port enable ([`controller`]);
//! - `drivers/input/serio/libps2.c` — sending a command to a PS/2 device and collecting its reply,
//!   including the ACK/NAK/ERR rules and the RESET_BAT / GETID timeout adjustments ([`ps2`]);
//! - `drivers/input/keyboard/atkbd.c` — the keyboard's probe, set selection, typematic, LEDs and
//!   activation commands ([`atkbd`]).
//!
//! [`bringup`] runs them in the order Linux does and names every step, so a host can print one line
//! per step and a refusal that says which step refused.
//!
//! Scancode DECODING is not here: `atkbd-scancode-core` ports `atkbd_receive_byte`.
//!
//! Copyright (c) 1999-2004 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov; the Linux input authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod atkbd;
pub mod bringup;
pub mod controller;
pub mod ps2;
pub mod regs;

/// The narrow surface the protocol needs from its host: two registers and a delay.
///
/// `reg` is [`regs::DATA_REG`] or [`regs::STATUS_REG`] / [`regs::COMMAND_REG`] — OFFSETS the host adds
/// to the controller's I/O base, which it takes from ACPI `PNP0303 _CRS` (Linux
/// `i8042-acpipnpio.h:39-41` reads them the same way). No absolute port lives in this crate.
pub trait Port {
    fn inb(&mut self, reg: u16) -> u8;
    fn outb(&mut self, reg: u16, value: u8);
    /// Busy-wait `us` microseconds (Linux `udelay`).
    fn udelay(&mut self, us: u32);
}
