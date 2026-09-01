// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI device and endpoint context layout and command-flag rules.
//!
//! Ported mechanically from Linux `drivers/usb/host/xhci-mem.c`,
//! `drivers/usb/host/xhci.h`, `drivers/usb/host/xhci-caps.h`, and
//! `drivers/usb/host/xhci.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.
//! Some Linux code was borrowed from the Linux EHCI driver.
//!
//! This crate performs no MMIO and owns no DMA memory. It only transforms caller-provided values.

#![no_std]
#![forbid(unsafe_code)]

pub mod endpoint;
pub mod flags;
pub mod layout;
pub mod slot;
