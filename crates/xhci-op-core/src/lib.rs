// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI operational-register and command sequencing logic.
//!
//! Ported from Linux `drivers/usb/host/xhci.c`, with register definitions from
//! `drivers/usb/host/xhci.h`, `drivers/usb/host/xhci-caps.h`, and
//! `drivers/usb/host/xhci-ext-caps.h`, plus command-abort and event-dequeue logic from
//! `drivers/usb/host/xhci-ring.c` and ERST programming from `drivers/usb/host/xhci-mem.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp. Copyright also belongs to the
//! Linux xHCI authors.
//!
//! This crate only constructs and decodes register words and advances explicit state machines. It
//! performs no MMIO, owns no DMA memory, and intentionally does not duplicate TRB, PORTSC, or
//! context mechanics from the sibling xHCI core crates.

#![no_std]
#![forbid(unsafe_code)]

pub mod command_ring;
pub mod interrupter;
pub mod registers;
pub mod sequence;
