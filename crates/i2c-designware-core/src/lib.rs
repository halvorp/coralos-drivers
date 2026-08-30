// SPDX-License-Identifier: GPL-2.0-only
//! Synopsys DesignWare I2C — the register corpus, ported for the Intel LPSS variant on Cherry Trail.
//!
//! Ported from Linux, and every constant below carries the FILE as well as the line:
//!   * `drivers/i2c/busses/i2c-designware-core.h` — offsets, bits, masks, abort bit positions
//!   * `drivers/i2c/busses/i2c-designware-common.c` — `abort_sources[]`, the cause messages
//!
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.
//!
//! THE FILE MATTERS AS MUCH AS THE LINE. Linux has moved this driver between
//! `i2c-designware-core.c` and `i2c-designware-common.c`; our reference is the newer layout and has
//! no `core.c` at all. A citation carrying only a line number would silently point into the wrong
//! file after any re-sync.
//!
//! Everything here was extracted MECHANICALLY from the C source. A mistyped offset does not fail to
//! compile — it drives a different register on real silicon and reports nothing.

#![cfg_attr(not(test), no_std)]

pub mod abort;
pub mod regs;
