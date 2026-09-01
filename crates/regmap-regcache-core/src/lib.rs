// SPDX-License-Identifier: GPL-2.0-only
//! Linux regmap cache selection, indexing, defaults, dirty state, and sync/drop planning.
//!
//! Ported mechanically from Linux:
//!   * `drivers/base/regmap/regcache.c` — cache selection, defaults, dirty state, and sync order
//!   * `drivers/base/regmap/regcache-flat.c` — FLAT indexing and drop arithmetic
//!   * `drivers/base/regmap/regcache-rbtree.c` — RBTREE block and range arithmetic
//!   * `drivers/base/regmap/internal.h` — stride-order index arithmetic
//!   * `include/linux/regmap.h` — cache type values and names
//!
//! Copyright 2011, 2012 Wolfson Microelectronics plc.
//! Original authors: Dimitris Papastamos and Mark Brown.
//!
//! This crate is `no_std`, performs no MMIO or I/O, uses no unsafe code, and allocates nothing.
//! Callers provide cache/default observations; this crate returns values and ordered write plans.
//! Formatting and access predicates remain in the already-landed `regmap-cache-core` crate.

#![no_std]
#![forbid(unsafe_code)]

pub mod defaults;
pub mod index;
pub mod selection;
pub mod sync;
