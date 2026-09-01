// SPDX-License-Identifier: GPL-2.0-only
//! Linux regmap register-cache policy and byte formatting, without hardware access.
//!
//! Ported mechanically from Linux:
//!   * `drivers/base/regmap/regmap.c` — formatting, access predicates, and read/write cache policy
//!   * `drivers/base/regmap/regcache.c` — volatile exclusion, dirty tracking, and sync ordering
//!
//! Copyright 2011 Wolfson Microelectronics plc.
//! Original regmap author: Mark Brown. Original regcache author: Dimitris Papastamos.
//!
//! This crate is `no_std`, contains no MMIO or I/O, and allocates nothing. Callers provide words,
//! byte buffers, predicate answers, and cached-value presence; the crate returns policy decisions.

#![no_std]
#![forbid(unsafe_code)]

pub mod cache;
pub mod format;
pub mod policy;
