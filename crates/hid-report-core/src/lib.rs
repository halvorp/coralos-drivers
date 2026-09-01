// SPDX-License-Identifier: GPL-2.0-only
//! Allocation-free HID report descriptor parsing.
//!
//! Ported mechanically from Linux `drivers/hid/hid-core.c` and
//! `include/linux/hid.h`: item decoding, global/local/main semantics, report-bit
//! accumulation, and collection ancestry.
//!
//! Original Linux copyright holders: Andreas Gal; Vojtech Pavlik; Michael
//! Haboustak for Concept2, Inc.; Jiri Kosina; and the Linux HID authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod item;
pub mod parser;
