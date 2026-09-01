// SPDX-License-Identifier: GPL-2.0-only
//! Linux I2C core message semantics, represented as pure values and validation functions.
//!
//! Ported mechanically from Linux:
//! * `drivers/i2c/i2c-core-base.c` — address bounds and strict reserved-address policy
//! * `include/uapi/linux/i2c.h` — `struct i2c_msg`, message flags, and SMBus block bound
//! * `include/linux/i2c.h` — 7-bit and 10-bit wire-address encoding
//!
//! Original copyright holders: Simon G. Vogl, Kyösti Mälkki, Frodo Looijaard,
//! Rodolfo Giometti, Michael Lawnick, Wolfram Sang, and the Linux I2C authors.
//!
//! This `no_std` crate performs no MMIO and no I/O. Callers pass message metadata in and receive
//! encoded bytes or a named refusal out.

#![no_std]
#![forbid(unsafe_code)]

pub mod address;
pub mod flags;
pub mod length;
