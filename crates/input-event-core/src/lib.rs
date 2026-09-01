// SPDX-License-Identifier: GPL-2.0-only
//! Linux input event routing and validation, without registration or I/O.
//!
//! Ported mechanically from Linux `drivers/input/input.c` and the code-space literals it uses from
//! `include/uapi/linux/input-event-codes.h` and `include/uapi/linux/input.h`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod absolute;
pub mod codes;
pub mod repeat;
pub mod routing;
