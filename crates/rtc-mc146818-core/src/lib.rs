// SPDX-License-Identifier: GPL-2.0-only
//! Pure MC146818 CMOS RTC register and protocol logic.
//!
//! Ported mechanically from Linux:
//! * `drivers/rtc/rtc-mc146818-lib.c` — stable reads and time encode/decode
//! * `include/linux/mc146818rtc.h` — register indices, bits, and alarm semantics
//! * `include/linux/bcd.h` — BCD conversion
//!
//! The register header was written by Torsten Duwe and derived from the Motorola data sheet;
//! copyright Torsten Duwe, Motorola, and the Linux RTC authors.
//!
//! This crate performs no MMIO or I/O. Callers supply register samples and execute the returned
//! decisions.

#![no_std]
#![forbid(unsafe_code)]

pub mod alarm;
pub mod bcd;
pub mod registers;
pub mod time;
pub mod uip;
