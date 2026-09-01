// SPDX-License-Identifier: GPL-2.0-only
//! Pure xHCI root-hub port logic.
//!
//! Ported from Linux `drivers/usb/host/xhci-hub.c`, with PORTSC definitions from
//! `drivers/usb/host/xhci-port.h`, by Sarah Sharp and the Linux xHCI authors.
//! Original copyright: Copyright (C) 2008 Intel Corp.
//!
//! This crate performs no MMIO and no I/O. Callers supply register snapshots and receive values or
//! explicit state-machine actions to apply.

#![no_std]
#![forbid(unsafe_code)]

pub mod descriptor;
pub mod portsc;
pub mod reset;
pub mod transition;
