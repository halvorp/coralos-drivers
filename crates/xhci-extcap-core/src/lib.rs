// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI extended-capability list and Supported Protocol parsing.
//!
//! Ported mechanically from Linux `drivers/usb/host/xhci-ext-caps.h`: capability-list fields and
//! IDs, USB Legacy Support, the Supported Protocol layout, and the list walk. This crate performs
//! no MMIO; callers provide already-read register words.
//!
//! Copyright (C) 2008 Intel Corp.
//! Original author: Sarah Sharp. Some Linux code was borrowed from the Linux EHCI driver.

#![no_std]
#![forbid(unsafe_code)]

pub mod caps;
pub mod protocol;
pub mod walk;
