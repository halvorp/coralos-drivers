// SPDX-License-Identifier: GPL-2.0-only
//! Pure PCI BAR resource request, ordering, bucketing, and assignment arithmetic.
//!
//! Ported from Linux `drivers/pci/setup-res.c`, `drivers/pci/setup-bus.c`,
//! `drivers/pci/bus.c`, `kernel/resource.c`, and `include/linux/ioport.h`.
//! BAR kind and sizing-probe decoding remain in the sibling `pci-config-core` crate.
//!
//! Copyright Dave Rusling, David Mosberger, David Miller, Andrea Arcangeli,
//! Ivan Kokshaysky, Linus Torvalds, and the Linux PCI authors.
//!
//! This crate performs no MMIO or I/O. Callers provide requests and windows and
//! receive assignments or named refusals.

#![no_std]
#![forbid(unsafe_code)]

pub mod assign;
pub mod request;
