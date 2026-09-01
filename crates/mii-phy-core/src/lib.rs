// SPDX-License-Identifier: GPL-2.0-only
//! Pure MII/PHY register and negotiation logic.
//!
//! Ported mechanically from Linux:
//! * `drivers/net/mii.c` — settings, link, restart, GMII and media decisions
//! * `include/linux/mii.h` — negotiation priority and ethtool conversions
//! * `include/uapi/linux/mii.h` — Clause 22 registers and register fields
//! * `include/uapi/linux/ethtool.h` — legacy link-mode bits and settings literals
//!
//! Copyright 2001, 2002 Jeff Garzik; copyright 1998-2002 Donald Becker;
//! copyright (C) 1996, 1999, 2001 David S. Miller and the Linux networking authors.
//!
//! This crate performs no MMIO and no I/O. Callers supply register words and receive decoded
//! values or an explicit write plan.

#![no_std]
#![forbid(unsafe_code)]

pub mod ethtool;
pub mod fields;
pub mod negotiation;
pub mod registers;
pub mod settings;
