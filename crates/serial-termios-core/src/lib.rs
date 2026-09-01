// SPDX-License-Identifier: GPL-2.0-only
//! Pure serial termios frame and baud policy.
//!
//! Mechanically ported from Linux:
//! - `drivers/tty/serial/serial_core.c` — serial termios use and baud bounds;
//! - `drivers/tty/tty_baudrate.c` — the standard baud corpus and 2% encoding tolerance;
//! - `drivers/tty/tty_ioctl.c` — character and frame-size decoding;
//! - `include/linux/util_macros.h` — the directional `find_closest` rule;
//! - `include/uapi/asm-generic/termbits.h` and `termbits-common.h` — cflag encodings.
//!
//! Based on `drivers/char/serial.c`, by Linus Torvalds and Theodore Ts'o.
//! Copyright 1999 ARM Limited. Copyright (C) 2000-2001 Deep Blue Solutions Ltd.
//! Copyright (C) 1991-1994 Linus Torvalds.
//!
//! This crate is `no_std`, performs no MMIO or I/O, and contains no divisor arithmetic.

#![no_std]
#![forbid(unsafe_code)]

pub mod baud;
pub mod frame;
