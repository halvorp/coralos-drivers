// SPDX-License-Identifier: GPL-2.0-only
//! Frozen setup-packet wire vectors.
//!
//! Ported from Linux `drivers/usb/core/message.c:150-:167` and
//! `include/uapi/linux/usb/ch9.h:194-:217`.
//!
//! Copyright (C) the Linux USB core authors and Linux USB API authors.

use usb_control_msg_core::{
    request::{Direction, Recipient, RequestType},
    setup::SetupPacket,
};

#[test]
fn constructor_writes_linux_field_order_and_little_endian_words() {
    let packet = SetupPacket::new(
        Direction::In, RequestType::Standard, Recipient::Device,
        0x06, 0x1234, 0x5678, 0x9abc,
    );
    // message.c:161-:165; ch9.h:210-:216. Literal wire bytes, not accessor-derived.
    assert_eq!(packet.as_bytes(), &[0x80, 0x06, 0x34, 0x12, 0x78, 0x56, 0xbc, 0x9a]);
}

#[test]
fn caller_supplied_bytes_decode_every_setup_field() {
    let packet = SetupPacket::from_bytes([0x42, 0x01, 0xef, 0xbe, 0x34, 0x12, 0x08, 0x00]);
    assert_eq!(packet.request_type_byte(), 0x42);
    assert_eq!(packet.direction(), Direction::Out);
    assert_eq!(packet.kind(), RequestType::Vendor);
    assert_eq!(packet.recipient_bits(), 0x02);
    assert_eq!(packet.request(), 0x01);
    assert_eq!(packet.value(), 0xbeef);
    assert_eq!(packet.index(), 0x1234);
    assert_eq!(packet.length(), 0x0008);
    assert_eq!(packet.as_bytes().len(), 8, "packed usb_ctrlrequest is eight bytes, ch9.h:210-:217");
}
