// SPDX-License-Identifier: GPL-2.0-only
//! Intel Crystal Cove PMIC sub-device register map and IRQ-domain/level decode.
//!
//! Mechanically ported from Linux `drivers/mfd/intel_soc_pmic_crc.c`.
//!
//! Copyright (C) 2012-2014, 2022 Intel Corporation. All rights reserved.
//! Original authors: Yang, Bin <bin.yang@intel.com> and Zhu, Lejun <lejun.zhu@linux.intel.com>.
//!
//! This crate performs no MMIO or I/O. Callers provide register values and receive decoded IRQs,
//! named refusals, or frozen sub-device metadata.

#![no_std]
#![forbid(unsafe_code)]

pub mod devices;
pub mod irq;
pub mod registers;
