// SPDX-License-Identifier: GPL-2.0-only
//! URB transfer-flag literals and Linux's per-transfer-type flag policy.
//!
//! Ported from Linux `include/linux/usb.h:1382-1407` and
//! `drivers/usb/core/urb.c:511-533`.
//!
//! Copyright (C) the Linux USB core and Linux USB API authors.

use crate::{Direction, TransferType};

pub const SHORT_NOT_OK: u32 = 0x0001; // include/linux/usb.h:1386
pub const ISO_ASAP: u32 = 0x0002; // include/linux/usb.h:1387
pub const NO_TRANSFER_DMA_MAP: u32 = 0x0004; // include/linux/usb.h:1389
pub const ZERO_PACKET: u32 = 0x0040; // include/linux/usb.h:1390
pub const NO_INTERRUPT: u32 = 0x0080; // include/linux/usb.h:1391
pub const FREE_BUFFER: u32 = 0x0100; // include/linux/usb.h:1393
pub const DIR_IN: u32 = 0x0200; // include/linux/usb.h:1396
pub const DIR_OUT: u32 = 0; // include/linux/usb.h:1397
pub const DIR_MASK: u32 = DIR_IN; // include/linux/usb.h:1398

pub const DMA_MAP_SINGLE: u32 = 0x0001_0000; // include/linux/usb.h:1400
pub const DMA_MAP_PAGE: u32 = 0x0002_0000; // include/linux/usb.h:1401
pub const DMA_MAP_SG: u32 = 0x0004_0000; // include/linux/usb.h:1402
pub const MAP_LOCAL: u32 = 0x0008_0000; // include/linux/usb.h:1403
pub const SETUP_MAP_SINGLE: u32 = 0x0010_0000; // include/linux/usb.h:1404
pub const SETUP_MAP_LOCAL: u32 = 0x0020_0000; // include/linux/usb.h:1405
pub const DMA_SG_COMBINED: u32 = 0x0040_0000; // include/linux/usb.h:1406

/// Linux-owned mapping and direction bits cleared before each submission
/// (`drivers/usb/core/urb.c:424-429`).
pub const INTERNAL_SUBMIT_MASK: u32 = DIR_MASK
    | DMA_MAP_SINGLE
    | DMA_MAP_PAGE
    | DMA_MAP_SG
    | MAP_LOCAL
    | SETUP_MAP_SINGLE
    | SETUP_MAP_LOCAL
    | DMA_SG_COMBINED;

/// Result of clearing Linux-owned bits and caching the endpoint direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedFlags {
    pub flags: u32,
    pub removed_internal: u32,
}

/// Clear stale internal flags and cache the current transfer direction
/// (`drivers/usb/core/urb.c:424-430`).
pub const fn prepare_for_submit(flags: u32, direction: Direction) -> PreparedFlags {
    let removed_internal = flags & INTERNAL_SUBMIT_MASK;
    let flags = (flags & !INTERNAL_SUBMIT_MASK)
        | match direction {
            Direction::Out => DIR_OUT,
            Direction::In => DIR_IN,
        };
    PreparedFlags {
        flags,
        removed_internal,
    }
}

/// Result of Linux's simple/standard flag policy (`drivers/usb/core/urb.c:511-533`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagPolicy {
    /// Flags Linux keeps.
    pub accepted: u32,
    /// Flags Linux warns about and excludes from `accepted`.
    pub refused: u32,
}

/// Apply the per-type and per-direction semantics used by `usb_submit_urb`.
///
/// `SHORT_NOT_OK` is read-only and non-isochronous; `ZERO_PACKET` is bulk/interrupt OUT;
/// `ISO_ASAP` is isochronous only. `NO_TRANSFER_DMA_MAP` is common to every type
/// (`drivers/usb/core/urb.c:511-526`).
pub const fn apply_policy(
    transfer_type: TransferType,
    direction: Direction,
    flags: u32,
) -> FlagPolicy {
    let mut allowed = NO_TRANSFER_DMA_MAP | NO_INTERRUPT | DIR_MASK | FREE_BUFFER;
    match transfer_type {
        TransferType::Isochronous => allowed |= ISO_ASAP,
        TransferType::Bulk | TransferType::Interrupt => {
            if matches!(direction, Direction::Out) {
                allowed |= ZERO_PACKET;
            }
            if matches!(direction, Direction::In) {
                allowed |= SHORT_NOT_OK;
            }
        }
        TransferType::Control => {
            if matches!(direction, Direction::In) {
                allowed |= SHORT_NOT_OK;
            }
        }
    }

    FlagPolicy {
        accepted: flags & allowed,
        refused: flags & !allowed,
    }
}
