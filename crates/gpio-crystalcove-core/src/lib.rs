// SPDX-License-Identifier: GPL-2.0-only
//! Intel Crystal Cove PMIC GPIO register maps and encodings.
//!
//! Mechanically ported from Linux `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.
//!
//! This crate performs no MMIO or I/O. Callers provide register values and consume register
//! addresses, masks, encoded values, or named refusals.

#![no_std]
#![forbid(unsafe_code)]

pub mod direction;
pub mod irq;
pub mod regs;
