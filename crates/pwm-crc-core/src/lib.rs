// SPDX-License-Identifier: GPL-2.0-only
//! Intel Crystal Cove PWM register encoding and enable sequencing.
//!
//! Ported mechanically from Linux `drivers/pwm/pwm-crc.c`.
//!
//! Copyright (C) 2015 Intel Corporation. All rights reserved.
//! Original author: Shobhit Kumar <shobhit.kumar@intel.com>.
//!
//! This crate performs no MMIO. Callers provide the old and requested states and execute the
//! returned register-write plan.

#![no_std]
#![forbid(unsafe_code)]

pub mod encode;
pub mod registers;
pub mod sequence;
