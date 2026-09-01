// SPDX-License-Identifier: GPL-2.0-only
//! `bmRequestType`'s three independent fields and Linux's standard `bRequest` encodings.
//!
//! Ported from Linux `include/uapi/linux/usb/ch9.h:39-:113`.
//!
//! Copyright (C) the Linux USB API authors.

/// Direction mask in `bmRequestType` (include/uapi/linux/usb/ch9.h:47-:48).
pub const DIRECTION_MASK: u8 = 0x80; // include/uapi/linux/usb/ch9.h:48
/// Type mask in `bmRequestType` (include/uapi/linux/usb/ch9.h:53).
pub const TYPE_MASK: u8 = 0x60; // include/uapi/linux/usb/ch9.h:53
/// Recipient mask in `bmRequestType` (include/uapi/linux/usb/ch9.h:62).
pub const RECIPIENT_MASK: u8 = 0x1f; // include/uapi/linux/usb/ch9.h:62

/// One named value in a packed request-type subfield.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedEncoding {
    pub name: &'static str,
    pub value: u8,
}

/// Control-transfer data direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Direction {
    /// Host to device (`USB_DIR_OUT`).
    Out = 0x00, // include/uapi/linux/usb/ch9.h:47
    /// Device to host (`USB_DIR_IN`).
    In = 0x80, // include/uapi/linux/usb/ch9.h:48
}

/// The second, two-bit `bmRequestType` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RequestType {
    Standard = 0x00, // include/uapi/linux/usb/ch9.h:54
    Class = 0x20, // include/uapi/linux/usb/ch9.h:55
    Vendor = 0x40, // include/uapi/linux/usb/ch9.h:56
    Reserved = 0x60, // include/uapi/linux/usb/ch9.h:57
}

/// The third, five-bit `bmRequestType` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Recipient {
    Device = 0x00, // include/uapi/linux/usb/ch9.h:63
    Interface = 0x01, // include/uapi/linux/usb/ch9.h:64
    Endpoint = 0x02, // include/uapi/linux/usb/ch9.h:65
    Other = 0x03, // include/uapi/linux/usb/ch9.h:66
    Port = 0x04, // include/uapi/linux/usb/ch9.h:68
    Rpipe = 0x05, // include/uapi/linux/usb/ch9.h:69
}

/// Both direction names Linux defines.
pub const DIRECTIONS: &[NamedEncoding] = &[
    NamedEncoding { name: "OUT", value: 0x00 }, // include/uapi/linux/usb/ch9.h:47
    NamedEncoding { name: "IN", value: 0x80 }, // include/uapi/linux/usb/ch9.h:48
];

/// All four type names Linux defines.
pub const REQUEST_TYPES: &[NamedEncoding] = &[
    NamedEncoding { name: "STANDARD", value: 0x00 }, // include/uapi/linux/usb/ch9.h:54
    NamedEncoding { name: "CLASS", value: 0x20 }, // include/uapi/linux/usb/ch9.h:55
    NamedEncoding { name: "VENDOR", value: 0x40 }, // include/uapi/linux/usb/ch9.h:56
    NamedEncoding { name: "RESERVED", value: 0x60 }, // include/uapi/linux/usb/ch9.h:57
];

/// All six recipient names Linux defines (including Wireless USB's two additions).
pub const RECIPIENTS: &[NamedEncoding] = &[
    NamedEncoding { name: "DEVICE", value: 0x00 }, // include/uapi/linux/usb/ch9.h:63
    NamedEncoding { name: "INTERFACE", value: 0x01 }, // include/uapi/linux/usb/ch9.h:64
    NamedEncoding { name: "ENDPOINT", value: 0x02 }, // include/uapi/linux/usb/ch9.h:65
    NamedEncoding { name: "OTHER", value: 0x03 }, // include/uapi/linux/usb/ch9.h:66
    NamedEncoding { name: "PORT", value: 0x04 }, // include/uapi/linux/usb/ch9.h:68
    NamedEncoding { name: "RPIPE", value: 0x05 }, // include/uapi/linux/usb/ch9.h:69
];

/// A named standard `bRequest` encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StandardRequest {
    pub name: &'static str,
    pub value: u8,
}

/// Every distinct standard-request name Linux defines, in source order.
pub const STANDARD_REQUESTS: &[StandardRequest] = &[
    StandardRequest { name: "GET_STATUS", value: 0x00 }, // include/uapi/linux/usb/ch9.h:78
    StandardRequest { name: "CLEAR_FEATURE", value: 0x01 }, // include/uapi/linux/usb/ch9.h:79
    StandardRequest { name: "SET_FEATURE", value: 0x03 }, // include/uapi/linux/usb/ch9.h:80
    StandardRequest { name: "SET_ADDRESS", value: 0x05 }, // include/uapi/linux/usb/ch9.h:81
    StandardRequest { name: "GET_DESCRIPTOR", value: 0x06 }, // include/uapi/linux/usb/ch9.h:82
    StandardRequest { name: "SET_DESCRIPTOR", value: 0x07 }, // include/uapi/linux/usb/ch9.h:83
    StandardRequest { name: "GET_CONFIGURATION", value: 0x08 }, // include/uapi/linux/usb/ch9.h:84
    StandardRequest { name: "SET_CONFIGURATION", value: 0x09 }, // include/uapi/linux/usb/ch9.h:85
    StandardRequest { name: "GET_INTERFACE", value: 0x0a }, // include/uapi/linux/usb/ch9.h:86
    StandardRequest { name: "SET_INTERFACE", value: 0x0b }, // include/uapi/linux/usb/ch9.h:87
    StandardRequest { name: "SYNCH_FRAME", value: 0x0c }, // include/uapi/linux/usb/ch9.h:88
    StandardRequest { name: "SET_SEL", value: 0x30 }, // include/uapi/linux/usb/ch9.h:89
    StandardRequest { name: "SET_ISOCH_DELAY", value: 0x31 }, // include/uapi/linux/usb/ch9.h:90
    StandardRequest { name: "SET_ENCRYPTION", value: 0x0d }, // include/uapi/linux/usb/ch9.h:92
    StandardRequest { name: "GET_ENCRYPTION", value: 0x0e }, // include/uapi/linux/usb/ch9.h:93
    StandardRequest { name: "RPIPE_ABORT", value: 0x0e }, // include/uapi/linux/usb/ch9.h:94
    StandardRequest { name: "SET_HANDSHAKE", value: 0x0f }, // include/uapi/linux/usb/ch9.h:95
    StandardRequest { name: "RPIPE_RESET", value: 0x0f }, // include/uapi/linux/usb/ch9.h:96
    StandardRequest { name: "GET_HANDSHAKE", value: 0x10 }, // include/uapi/linux/usb/ch9.h:97
    StandardRequest { name: "SET_CONNECTION", value: 0x11 }, // include/uapi/linux/usb/ch9.h:98
    StandardRequest { name: "SET_SECURITY_DATA", value: 0x12 }, // include/uapi/linux/usb/ch9.h:99
    StandardRequest { name: "GET_SECURITY_DATA", value: 0x13 }, // include/uapi/linux/usb/ch9.h:100
    StandardRequest { name: "SET_WUSB_DATA", value: 0x14 }, // include/uapi/linux/usb/ch9.h:101
    StandardRequest { name: "LOOPBACK_DATA_WRITE", value: 0x15 }, // include/uapi/linux/usb/ch9.h:102
    StandardRequest { name: "LOOPBACK_DATA_READ", value: 0x16 }, // include/uapi/linux/usb/ch9.h:103
    StandardRequest { name: "SET_INTERFACE_DS", value: 0x17 }, // include/uapi/linux/usb/ch9.h:104
    StandardRequest { name: "GET_PARTNER_PDO", value: 20 }, // include/uapi/linux/usb/ch9.h:107
    StandardRequest { name: "GET_BATTERY_STATUS", value: 21 }, // include/uapi/linux/usb/ch9.h:108
    StandardRequest { name: "SET_PDO", value: 22 }, // include/uapi/linux/usb/ch9.h:109
    StandardRequest { name: "GET_VDM", value: 23 }, // include/uapi/linux/usb/ch9.h:110
    StandardRequest { name: "SEND_VDM", value: 24 }, // include/uapi/linux/usb/ch9.h:111
];

/// Pack the three independent `bmRequestType` subfields.
pub const fn pack(direction: Direction, request_type: RequestType, recipient: Recipient) -> u8 {
    (direction as u8 & DIRECTION_MASK)
        | (request_type as u8 & TYPE_MASK)
        | (recipient as u8 & RECIPIENT_MASK)
}

/// Decode direction independently from bit 7.
pub const fn direction(packed: u8) -> Direction {
    if packed & DIRECTION_MASK != 0 { Direction::In } else { Direction::Out }
}

/// Decode the type independently from bits 6:5.
pub const fn request_type(packed: u8) -> RequestType {
    match packed & TYPE_MASK {
        0x00 => RequestType::Standard,
        0x20 => RequestType::Class,
        0x40 => RequestType::Vendor,
        _ => RequestType::Reserved,
    }
}

/// Return the raw five-bit recipient field. Values 6..=31 are reserved but remain decodable.
pub const fn recipient(packed: u8) -> u8 {
    packed & RECIPIENT_MASK
}
