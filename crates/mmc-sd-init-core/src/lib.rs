// SPDX-License-Identifier: GPL-2.0-only
//! Pure SD-card initialisation and capability decoding.
//!
//! Ported mechanically from Linux `drivers/mmc/core/sd.c`, with literals from
//! `include/linux/mmc/card.h`, `include/linux/mmc/host.h`, and
//! `include/linux/mmc/sd.h`. R2 extraction is deliberately reused from
//! `mmc-core-cmd`; this crate performs no MMIO or I/O.
//!
//! Copyright (C) 2003-2004 Russell King; SD support Copyright (C) 2004 Ian
//! Molton; Copyright (C) 2005-2007 Pierre Ossman.

#![no_std]
#![forbid(unsafe_code)]

pub mod bus;
pub mod csd;
pub mod ocr;
pub mod scr;
pub mod ssr;
pub mod switch;
