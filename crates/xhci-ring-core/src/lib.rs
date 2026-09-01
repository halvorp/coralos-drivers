// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI transfer and command ring mechanics.
//!
//! Ported from Linux `drivers/usb/host/xhci-ring.c` and the TRB definitions used by that file in
//! `drivers/usb/host/xhci.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp. Copyright also belongs to the
//! Linux xHCI authors.
//!
//! This crate only transforms TRB words and ring positions. It performs no MMIO, no doorbell
//! writes, and owns no DMA memory.

#![no_std]
#![forbid(unsafe_code)]

pub mod completion;
pub mod ring;
pub mod trb;
