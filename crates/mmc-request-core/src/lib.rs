// SPDX-License-Identifier: GPL-2.0-only
//! Pure MMC request validation, retry decisions, and erase argument encoding.
//!
//! Ported mechanically from Linux `drivers/mmc/core/core.c`,
//! `drivers/mmc/core/core.h`, and `drivers/mmc/core/sd_ops.c`, with protocol
//! literals from `include/linux/mmc/mmc.h` and `include/linux/mmc/sd.h`.
//!
//! Copyright (C) 2003-2004 Russell King; SD support Copyright (C) 2004 Ian
//! Molton; Copyright (C) 2005-2008 Pierre Ossman; MMCv4 support Copyright (C)
//! 2006 Philip Langdale.

#![no_std]
#![forbid(unsafe_code)]

// This dependency is intentionally retained: request policy is layered above
// mmc-core-cmd, which remains the single owner of response/CSD/CID decoding.
pub use mmc_core_cmd as command;

pub mod erase;
pub mod retry;
pub mod transfer;
