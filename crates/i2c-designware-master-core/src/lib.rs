// SPDX-License-Identifier: GPL-2.0-only
//! Pure DesignWare I2C master transfer machinery.
//!
//! Ported from Linux `drivers/i2c/busses/i2c-designware-master.c`, layered on
//! `i2c-designware-core` for the register map and abort decoding. Message flag literals additionally
//! come from `include/uapi/linux/i2c.h` and frequency literals from `include/linux/i2c.h`.
//!
//! Copyright (C) 2006 Texas Instruments.
//! Copyright (C) 2007 MontaVista Software Inc.
//! Copyright (C) 2009 Provigent Ltd.
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod fifo;
pub mod setup;
pub mod timing;
