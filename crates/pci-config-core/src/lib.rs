// SPDX-License-Identifier: GPL-2.0-only
//! Pure PCI configuration-space layout and decoding.
//!
//! Ported from Linux `drivers/pci/pci.c`, `drivers/pci/pci.h`,
//! `drivers/pci/probe.c`, and `include/uapi/linux/pci_regs.h`.
//!
//! Copyright 1993--1997 Drew Eckhardt, Frederic Potter, and David Mosberger-Tang.
//! Copyright 1994 Drew Eckhardt. Copyright 1997--2000 Martin Mares.
//! Copyright the Linux PCI authors.
//!
//! This crate performs no MMIO or I/O. Callers provide a configuration-space
//! byte slice and receive decoded values or named refusals.

#![no_std]
#![forbid(unsafe_code)]

pub mod bar;
pub mod capability;
pub mod regs;
