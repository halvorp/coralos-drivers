// SPDX-License-Identifier: GPL-2.0-only
//! Pure Cherryview/Braswell pin-control register and state logic.
//!
//! Ported from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//!
//! Copyright (C) 2014-2020 Intel Corporation. Original author Mika Westerberg;
//! based on the original Cherryview GPIO driver by Ning Li and Alan Cox.
//!
//! This crate performs no MMIO: callers provide register words and apply the returned words.

#![no_std]
#![forbid(unsafe_code)]

pub mod address;
pub mod communities;
pub mod interrupt;
pub mod padctrl;
pub mod pins;
pub mod regs;
