// SPDX-License-Identifier: GPL-2.0-only
//! Endpoint address, attribute, and `wMaxPacketSize` decoding.
//!
//! Ported from Linux `drivers/usb/core/config.c` and `include/uapi/linux/usb/ch9.h`.
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.

pub const ENDPOINT_NUMBER_MASK: u8 = 0x0f; // include/uapi/linux/usb/ch9.h:437
pub const ENDPOINT_DIRECTION_MASK: u8 = 0x80; // include/uapi/linux/usb/ch9.h:438
pub const TRANSFER_TYPE_MASK: u8 = 0x03; // include/uapi/linux/usb/ch9.h:440
pub const SYNC_TYPE_MASK: u8 = 0x0c; // include/uapi/linux/usb/ch9.h:458
pub const USAGE_TYPE_MASK: u8 = 0x30; // include/uapi/linux/usb/ch9.h:464
pub const MAX_PACKET_MASK: u16 = 0x07ff; // include/uapi/linux/usb/ch9.h:447
pub const MAX_PACKET_MULT_SHIFT: u8 = 11; // include/uapi/linux/usb/ch9.h:448
pub const MAX_PACKET_MULT_MASK: u16 = 3 << MAX_PACKET_MULT_SHIFT; // include/uapi/linux/usb/ch9.h:449

/// Linux's four endpoint transfer-type names and literals.
pub const TRANSFER_TYPES: &[(&str, u8)] = &[
    ("CONTROL", 0), // include/uapi/linux/usb/ch9.h:441
    ("ISOC", 1),    // include/uapi/linux/usb/ch9.h:442
    ("BULK", 2),    // include/uapi/linux/usb/ch9.h:443
    ("INT", 3),     // include/uapi/linux/usb/ch9.h:444
];

/// Linux's four synchronization-type names and literals.
pub const SYNC_TYPES: &[(&str, u8)] = &[
    ("NONE", 0x00),     // include/uapi/linux/usb/ch9.h:459
    ("ASYNC", 0x04),    // include/uapi/linux/usb/ch9.h:460
    ("ADAPTIVE", 0x08), // include/uapi/linux/usb/ch9.h:461
    ("SYNC", 0x0c),     // include/uapi/linux/usb/ch9.h:462
];

/// The three usage encodings Linux names. `0x30` is reserved.
pub const USAGE_TYPES: &[(&str, u8)] = &[
    ("DATA", 0x00),        // include/uapi/linux/usb/ch9.h:465
    ("FEEDBACK", 0x10),    // include/uapi/linux/usb/ch9.h:466
    ("IMPLICIT_FB", 0x20), // include/uapi/linux/usb/ch9.h:467
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferType {
    Control,
    Isochronous,
    Bulk,
    Interrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncType {
    None,
    Asynchronous,
    Adaptive,
    Synchronous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsageType {
    Data,
    Feedback,
    ImplicitFeedback,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointAddress {
    pub number: u8,
    pub direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointAttributes {
    pub transfer_type: TransferType,
    pub sync_type: SyncType,
    pub usage_type: UsageType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MaxPacketSize {
    /// Packet bytes, bits 10:0 (`ch9.h:650-657`).
    pub bytes: u16,
    /// Transactional opportunities, encoded bits 12:11 plus one (`ch9.h:661-671`).
    pub transactions: u8,
}

/// Decode endpoint number and direction (`ch9.h:474-479`, `ch9.h:495-514`). Reserved address bits
/// are ignored just as Linux clears them in `config.c:333-340`.
pub const fn decode_address(address: u8) -> EndpointAddress {
    EndpointAddress {
        number: address & ENDPOINT_NUMBER_MASK,
        direction: if address & ENDPOINT_DIRECTION_MASK != 0 {
            Direction::In
        } else {
            Direction::Out
        },
    }
}

/// Decode transfer, synchronization, and usage fields from `bmAttributes` (`ch9.h:440-467`).
pub const fn decode_attributes(attributes: u8) -> EndpointAttributes {
    EndpointAttributes {
        transfer_type: match attributes & TRANSFER_TYPE_MASK {
            0 => TransferType::Control,
            1 => TransferType::Isochronous,
            2 => TransferType::Bulk,
            _ => TransferType::Interrupt,
        },
        sync_type: match attributes & SYNC_TYPE_MASK {
            0x00 => SyncType::None,
            0x04 => SyncType::Asynchronous,
            0x08 => SyncType::Adaptive,
            _ => SyncType::Synchronous,
        },
        usage_type: match attributes & USAGE_TYPE_MASK {
            0x00 => UsageType::Data,
            0x10 => UsageType::Feedback,
            0x20 => UsageType::ImplicitFeedback,
            _ => UsageType::Reserved,
        },
    }
}

/// Decode packet bytes and the high-bandwidth multiplier. Linux's helper adds one to the encoded
/// multiplier (`ch9.h:650-671`); raw `0x1800` therefore means four transactions.
pub const fn decode_max_packet_size(raw: u16) -> MaxPacketSize {
    MaxPacketSize {
        bytes: raw & MAX_PACKET_MASK,
        transactions: (((raw & MAX_PACKET_MULT_MASK) >> MAX_PACKET_MULT_SHIFT) + 1) as u8,
    }
}
