// SPDX-License-Identifier: GPL-2.0-only
//! Intel SoC DTS thermal sensor register corpus and pure thermal-zone arithmetic.
//!
//! Ported mechanically from Linux:
//! * `drivers/thermal/intel/intel_soc_dts_iosf.c` — register offsets, bit masks, temperature
//!   decoding, trip programming, and thermal-zone setup.
//! * `drivers/thermal/intel/intel_soc_dts_iosf.h` — sensor/trip counts and interrupt-type names.
//!
//! Copyright (c) 2015, Intel Corporation.
//!
//! This crate performs no MMIO and no I/O. Callers supply register values and receive decoded
//! values or complete proposed register updates.

#![no_std]
#![forbid(unsafe_code)]

pub mod registers;
pub mod temperature;
pub mod trip;
pub mod zone;
