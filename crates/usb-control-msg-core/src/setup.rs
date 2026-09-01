// SPDX-License-Identifier: GPL-2.0-only
//! The packed eight-byte USB SETUP packet.
//!
//! Ported from Linux `drivers/usb/core/message.c:150-:167` and
//! `include/uapi/linux/usb/ch9.h:194-:217`.
//!
//! Copyright (C) the Linux USB core authors and Linux USB API authors.

use crate::request::{self, Direction, Recipient, RequestType};

/// Linux's packed `struct usb_ctrlrequest`, represented without alignment-dependent fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetupPacket {
    bytes: [u8; 8],
}

impl SetupPacket {
    /// Build the exact wire image. Linux converts all three `u16` fields to little endian.
    pub const fn new(
        direction: Direction,
        request_type: RequestType,
        recipient: Recipient,
        request: u8,
        value: u16,
        index: u16,
        length: u16,
    ) -> Self {
        let value = value.to_le_bytes();
        let index = index.to_le_bytes();
        let length = length.to_le_bytes();
        Self {
            bytes: [
                request::pack(direction, request_type, recipient),
                request,
                value[0],
                value[1],
                index[0],
                index[1],
                length[0],
                length[1],
            ],
        }
    }

    /// Decode a caller-supplied SETUP wire image.
    pub const fn from_bytes(bytes: [u8; 8]) -> Self {
        Self { bytes }
    }

    pub const fn as_bytes(&self) -> &[u8; 8] { &self.bytes }
    pub const fn request_type_byte(&self) -> u8 { self.bytes[0] }
    pub const fn direction(&self) -> Direction { request::direction(self.bytes[0]) }
    pub const fn kind(&self) -> RequestType { request::request_type(self.bytes[0]) }
    pub const fn recipient_bits(&self) -> u8 { request::recipient(self.bytes[0]) }
    pub const fn request(&self) -> u8 { self.bytes[1] }
    pub const fn value(&self) -> u16 { u16::from_le_bytes([self.bytes[2], self.bytes[3]]) }
    pub const fn index(&self) -> u16 { u16::from_le_bytes([self.bytes[4], self.bytes[5]]) }
    pub const fn length(&self) -> u16 { u16::from_le_bytes([self.bytes[6], self.bytes[7]]) }
}
