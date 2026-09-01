// SPDX-License-Identifier: GPL-2.0-only
//! USB control-transfer setup construction, pipe encoding, and validation.
//!
//! Ported mechanically from Linux:
//!   * `drivers/usb/core/message.c` — setup construction and send/receive completion rules
//!   * `drivers/usb/core/urb.c` — setup length and control-direction validation
//!   * `include/uapi/linux/usb/ch9.h` — setup layout and Chapter 9 encodings
//!   * `include/linux/usb.h` — Linux pipe encoding
//!
//! Copyright (C) the Linux USB core authors and Linux USB API authors.
//!
//! This crate performs no I/O and no MMIO. Callers supply values and receive encoded bytes or
//! named refusals. Descriptor and hub-port parsing deliberately remain in their existing crates.

#![no_std]
#![forbid(unsafe_code)]

pub mod pipe;
pub mod request;
pub mod setup;
pub mod validation;
