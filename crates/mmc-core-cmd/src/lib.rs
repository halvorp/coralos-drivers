// SPDX-License-Identifier: GPL-2.0-only
//! Pure MMC/eMMC command-response decoding.
//!
//! Ported mechanically from Linux `drivers/mmc/core/mmc.c`, `mmc_ops.c`, and
//! `mmc_ops.h`, with protocol literals used there from `include/linux/mmc/mmc.h`
//! and response flags from `include/linux/mmc/core.h`.
//!
//! Copyright (C) 2003-2004 Russell King; Copyright (C) 2005-2007 Pierre Ossman;
//! MMCv4 support Copyright (C) 2006 Philip Langdale.

#![no_std]
#![forbid(unsafe_code)]

pub mod cid;
pub mod csd;
pub mod ext_csd;
pub mod response;
pub mod status;
