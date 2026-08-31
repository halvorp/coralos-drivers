// SPDX-License-Identifier: GPL-2.0-only
//! Pure control logic for the Linux PC-style CMOS RTC driver.
//!
//! Ported mechanically from Linux `drivers/rtc/rtc-cmos.c`, originally copyright Paul Gortmaker
//! and David Brownell, with later work by the Linux RTC authors. MC146818 register definitions and
//! BCD conversion are reused from `rtc-mc146818-core`; they are not duplicated here.
//!
//! This crate performs no MMIO or I/O. Callers provide register values and execute returned
//! decisions.

#![no_std]
#![forbid(unsafe_code)]

pub mod alarm;
pub mod control;
pub mod interrupt;
pub mod wake;
