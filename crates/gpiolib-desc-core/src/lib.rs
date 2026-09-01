// SPDX-License-Identifier: GPL-2.0-only
//! Linux GPIO descriptor flags and value/configuration decisions, without MMIO.
//!
//! Mechanically ported from Linux:
//!   * `drivers/gpio/gpiolib.c` — descriptor validation, flag precedence, direction, bias, and
//!     logical/raw value handling
//!   * `drivers/gpio/gpiolib.h` — internal descriptor flag bit numbers
//!   * `include/linux/gpio/machine.h` — lookup (`GPIO_*`) flags
//!   * `include/linux/gpio/consumer.h` — descriptor request (`GPIOD_*`) flags
//!   * `include/linux/gpio/driver.h` — direction encoding
//!   * `include/linux/pinctrl/pinconf-generic.h` — packed pin configuration encoding
//!
//! Copyright (C) 2013 Intel Corporation; Copyright (C) 2011 ST-Ericsson SA; and the Linux GPIO
//! subsystem authors.
//!
//! This crate performs no hardware access. It maps caller-supplied values to pure decisions that a
//! controller layer can execute.

#![no_std]
#![forbid(unsafe_code)]

pub mod config;
pub mod flags;
pub mod validation;
pub mod value;
