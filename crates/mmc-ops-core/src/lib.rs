// SPDX-License-Identifier: GPL-2.0-only
//! Pure MMC/eMMC card-operation command construction and busy-wait policy.
//!
//! Ported mechanically from Linux `drivers/mmc/core/mmc_ops.c` and
//! `drivers/mmc/core/mmc_ops.h`, with protocol literals from
//! `include/linux/mmc/core.h` and `include/linux/mmc/mmc.h`. The EXT_CSD and
//! R1 decoders remain owned by `mmc-core-cmd`; this crate performs no MMIO or
//! I/O.
//!
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.

#![no_std]
#![forbid(unsafe_code)]

pub use mmc_core_cmd as command;

pub mod busy;
pub mod ops;
pub mod switch;
