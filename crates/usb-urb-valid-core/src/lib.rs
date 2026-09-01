// SPDX-License-Identifier: GPL-2.0-only
//! Pure validation and interval conversion for USB request blocks (URBs).
//!
//! Ported mechanically from Linux:
//!   * `drivers/usb/core/urb.c` — submission sanity rules, flags, packet and interval bounds
//!   * `include/linux/usb.h` — URB flag literals and descriptor-interval conversion
//!   * `include/uapi/linux/usb/ch9.h` — transfer-type and speed names
//!
//! Copyright (C) the Linux USB core, Linux USB API, and Chapter 9 header authors.
//!
//! The crate examines caller-supplied descriptors only. It performs no MMIO, allocation, or I/O.

#![no_std]
#![forbid(unsafe_code)]

pub mod flags;
pub mod interval;
pub mod validation;

/// USB endpoint transfer type (`include/uapi/linux/usb/ch9.h:441-444`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

/// Linux's endpoint transfer-type names and literals (`include/uapi/linux/usb/ch9.h:441-444`).
pub const TRANSFER_TYPES: &[(&str, u8, TransferType)] = &[
    ("CONTROL", 0, TransferType::Control), // include/uapi/linux/usb/ch9.h:441
    ("ISOC", 1, TransferType::Isochronous), // include/uapi/linux/usb/ch9.h:442
    ("BULK", 2, TransferType::Bulk),       // include/uapi/linux/usb/ch9.h:443
    ("INT", 3, TransferType::Interrupt),   // include/uapi/linux/usb/ch9.h:444
];

/// Transfer direction cached by Linux at submission (`drivers/usb/core/urb.c:424-430`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
}

/// Wired speeds accepted by the interval switch (`drivers/usb/core/urb.c:551-580`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
    SuperPlus,
}

/// Linux's wired speed names (`include/uapi/linux/usb/ch9.h:1203-1207`).
pub const WIRED_SPEEDS: &[(&str, Speed)] = &[
    ("LOW", Speed::Low),              // include/uapi/linux/usb/ch9.h:1203
    ("FULL", Speed::Full),            // include/uapi/linux/usb/ch9.h:1203
    ("HIGH", Speed::High),            // include/uapi/linux/usb/ch9.h:1204
    ("SUPER", Speed::Super),          // include/uapi/linux/usb/ch9.h:1206
    ("SUPER_PLUS", Speed::SuperPlus), // include/uapi/linux/usb/ch9.h:1207
];
