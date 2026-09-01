// SPDX-License-Identifier: GPL-2.0-only
//! Pure SMBus protocol encoding, I2C-message emulation, and packet error checking.
//!
//! Ported mechanically from Linux:
//! * `drivers/i2c/i2c-core-smbus.c` — emulation layout, direction rules, PEC, and refusals
//! * `include/uapi/linux/i2c.h` — transaction encodings and the SMBus block bound
//! * `include/linux/i2c.h` — the client PEC flag
//!
//! Original copyright holders: Frodo Looijaard, Mark Studebaker, Jean Delvare, Simon G. Vogl,
//! Kyösti Mälkki, and the Linux I2C authors.
//!
//! This `no_std` crate performs no MMIO and no I/O. It deliberately does not duplicate the
//! already-ported I2C message flag/address corpus: message properties are represented semantically
//! and the caller translates them to its message type.

#![no_std]
#![forbid(unsafe_code)]

pub mod emulation;
pub mod pec;
pub mod protocol;
