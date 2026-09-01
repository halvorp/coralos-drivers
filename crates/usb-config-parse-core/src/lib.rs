// SPDX-License-Identifier: GPL-2.0-only
//! Allocation-free USB configuration descriptor parsing.
//!
//! Ported mechanically from Linux `drivers/usb/core/config.c` (descriptor walk and validation)
//! and `include/uapi/linux/usb/ch9.h` (descriptor layouts and field encodings).
//!
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.
//!
//! This crate only examines caller-supplied bytes. It performs no MMIO, allocation, or I/O.

#![no_std]
#![forbid(unsafe_code)]

pub mod decode;
pub mod descriptor;
pub mod interval;
pub mod walk;
