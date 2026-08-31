// SPDX-License-Identifier: GPL-2.0-only
//! Intel INT0002 virtual GPIO core: GPE0a register values and wake-source decoding.
//!
//! Ported mechanically from Linux
//! `drivers/platform/x86/intel/int0002_vgpio.c`.
//!
//! Original copyright holders:
//! - Copyright (C) 2017 Hans de Goede <hdegoede@redhat.com>
//! - Copyright (c) 2014 Intel Corporation
//!
//! This crate performs no port I/O or MMIO. Callers supply sampled register values and perform the
//! returned writes themselves.

#![no_std]
#![forbid(unsafe_code)]

pub mod registers;
pub mod wake;
