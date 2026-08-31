// SPDX-License-Identifier: GPL-2.0-only
//! Pure Intel LPSS / Synopsys DesignWare 8250 UART core for Cherry Trail.
//!
//! Mechanically ported from Linux:
//! - `drivers/tty/serial/8250/8250_dw.c` — USR busy detection and interrupt decoding;
//! - `drivers/tty/serial/8250/8250_lpss.c` — LPSS private clock, boards, PCI IDs and DMA setup;
//! - `drivers/tty/serial/8250/8250_dwlib.c` — DesignWare fractional divisor arithmetic;
//! - `include/uapi/linux/serial_reg.h` — standard 8250 FIFO/LCR/MCR register encodings.
//!
//! Copyright 2011 Picochip, Jamie Iles.
//! Copyright 2013, 2016 Intel Corporation.
//! Copyright 1992, 1994 Theodore Ts'o.
//!
//! This crate performs no MMIO. Callers supply sampled register values and apply returned words.

#![no_std]
#![forbid(unsafe_code)]

pub mod baud;
pub mod busy;
pub mod fifo;
pub mod lpss;
pub mod regs;
