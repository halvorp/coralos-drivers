// SPDX-License-Identifier: GPL-2.0-only
//! Pure Intel LPSS SPI (PXA2xx-compatible) control-word and transfer decisions.
//!
//! Ported mechanically from Linux `drivers/spi/spi-pxa2xx.c`,
//! `drivers/spi/spi-pxa2xx.h`, and the register definitions those files use from
//! `include/linux/pxa2xx_ssp.h`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation
//!
//! This crate performs no MMIO or I/O. Callers provide register words and receive
//! encoded words or named state decisions.

#![no_std]
#![forbid(unsafe_code)]

pub mod clock;
pub mod control;
pub mod fifo;
pub mod regs;
pub mod state;
