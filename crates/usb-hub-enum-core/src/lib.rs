// SPDX-License-Identifier: GPL-2.0-only
//! USB hub port-state and device-enumeration logic.
//!
//! Ported mechanically from Linux:
//!   * `drivers/usb/core/hub.c` — descriptor handling, debounce/reset sequencing, speed selection,
//!     and enumeration retry policy
//!   * `include/uapi/linux/usb/ch11.h` — hub descriptor and port status/change literals
//!   * `include/uapi/linux/usb/ch9.h` — USB speed ordering
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.
//!
//! This crate performs no MMIO, sleeping, allocation, or I/O. Callers supply descriptor bytes,
//! status samples, and operation results; the returned decisions tell the caller what to do next.

#![no_std]
#![forbid(unsafe_code)]

pub mod debounce;
pub mod descriptor;
pub mod policy;
pub mod port;
pub mod reset;
