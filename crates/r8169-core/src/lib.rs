// SPDX-License-Identifier: GPL-2.0-only
//
// Ported from the Linux kernel:
//   drivers/net/ethernet/realtek/r8169_main.c
//   drivers/net/ethernet/realtek/r8169.h
// Copyright (c) a lot of people, see the Linux source. GPL-2.0-only, preserved.
//
//! r8169-core — the RealTek RTL8169/8168/8101 register map and descriptor format, ported.
//!
//! A PORT, NOT A REWRITE, and the difference is the whole point of this repository: these numbers
//! come from a driver that is known to work on real silicon, so every constant here carries the
//! file and line it came from. A value that cannot be traced back is a value somebody guessed, and
//! on a NIC a guessed offset does not fail loudly — it reads a different register and the driver
//! misbehaves in a way that looks like a hardware fault.
//!
//! WHY THIS DRIVER FIRST, among the fleet: the CoralOS deploy path was measured at ~117 MB/s to the
//! eMMC and ~4 MB/s over the network, so the NIC is the bottleneck in every deployment.
//!
//! This crate is the FOUNDATION slice — the register map and the descriptor layout. Reset/init, the
//! RX/TX rings and error recovery follow, in that order, each with its own vectors.

#![cfg_attr(not(test), no_std)]

pub mod desc;
pub mod init;
pub mod irq;
pub mod regs;
pub mod ring;
pub mod chip;
pub mod mdio;
pub mod rx;
