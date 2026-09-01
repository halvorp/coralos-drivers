// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for each independent `bmRequestType` field and every standard request.
//!
//! Ported from Linux `include/uapi/linux/usb/ch9.h:39-:113`.
//!
//! Copyright (C) the Linux USB API authors.

use usb_control_msg_core::request::{
    direction, pack, recipient, request_type, Direction, Recipient, RequestType, DIRECTIONS,
    RECIPIENTS, REQUEST_TYPES, STANDARD_REQUESTS,
};

const LINUX_DIRECTIONS: [(&str, u8); 2] = [("OUT", 0x00), ("IN", 0x80)]; // ch9.h:47-:48
const LINUX_TYPES: [(&str, u8); 4] = [
    ("STANDARD", 0x00), ("CLASS", 0x20), ("VENDOR", 0x40), ("RESERVED", 0x60),
]; // ch9.h:54-:57
const LINUX_RECIPIENTS: [(&str, u8); 6] = [
    ("DEVICE", 0x00), ("INTERFACE", 0x01), ("ENDPOINT", 0x02),
    ("OTHER", 0x03), ("PORT", 0x04), ("RPIPE", 0x05),
]; // ch9.h:63-:69

#[test]
fn every_subfield_count_name_and_literal_is_pinned() {
    assert_eq!(DIRECTIONS.len(), 2);
    assert_eq!(REQUEST_TYPES.len(), 4);
    assert_eq!(RECIPIENTS.len(), 6);
    assert_eq!(DIRECTIONS.iter().map(|x| (x.name, x.value)).collect::<Vec<_>>(), LINUX_DIRECTIONS);
    assert_eq!(REQUEST_TYPES.iter().map(|x| (x.name, x.value)).collect::<Vec<_>>(), LINUX_TYPES);
    assert_eq!(RECIPIENTS.iter().map(|x| (x.name, x.value)).collect::<Vec<_>>(), LINUX_RECIPIENTS);
}

#[test]
fn subfields_are_separate_then_pack_in_both_directions() {
    // Assert direction, type, and recipient separately before asserting the packed byte.
    assert_eq!(Direction::Out as u8, 0x00); // ch9.h:47
    assert_eq!(RequestType::Class as u8, 0x20); // ch9.h:55
    assert_eq!(Recipient::Endpoint as u8, 0x02); // ch9.h:65
    assert_eq!(pack(Direction::Out, RequestType::Class, Recipient::Endpoint), 0x22);

    assert_eq!(Direction::In as u8, 0x80); // ch9.h:48
    assert_eq!(RequestType::Vendor as u8, 0x40); // ch9.h:56
    assert_eq!(Recipient::Rpipe as u8, 0x05); // ch9.h:69
    assert_eq!(pack(Direction::In, RequestType::Vendor, Recipient::Rpipe), 0xc5);
}

#[test]
fn packed_bytes_decode_each_field_independently() {
    assert_eq!(direction(0x22), Direction::Out);
    assert_eq!(request_type(0x22), RequestType::Class);
    assert_eq!(recipient(0x22), 0x02);
    assert_eq!(direction(0xc5), Direction::In);
    assert_eq!(request_type(0xc5), RequestType::Vendor);
    assert_eq!(recipient(0xc5), 0x05);
    assert_eq!(recipient(0xff), 0x1f, "recipient mask is five bits, ch9.h:62");
}

const LINUX_STANDARD_REQUESTS: [(&str, u8); 31] = [
    ("GET_STATUS", 0x00), ("CLEAR_FEATURE", 0x01), ("SET_FEATURE", 0x03),
    ("SET_ADDRESS", 0x05), ("GET_DESCRIPTOR", 0x06), ("SET_DESCRIPTOR", 0x07),
    ("GET_CONFIGURATION", 0x08), ("SET_CONFIGURATION", 0x09),
    ("GET_INTERFACE", 0x0a), ("SET_INTERFACE", 0x0b), ("SYNCH_FRAME", 0x0c),
    ("SET_SEL", 0x30), ("SET_ISOCH_DELAY", 0x31), ("SET_ENCRYPTION", 0x0d),
    ("GET_ENCRYPTION", 0x0e), ("RPIPE_ABORT", 0x0e), ("SET_HANDSHAKE", 0x0f),
    ("RPIPE_RESET", 0x0f), ("GET_HANDSHAKE", 0x10), ("SET_CONNECTION", 0x11),
    ("SET_SECURITY_DATA", 0x12), ("GET_SECURITY_DATA", 0x13),
    ("SET_WUSB_DATA", 0x14), ("LOOPBACK_DATA_WRITE", 0x15),
    ("LOOPBACK_DATA_READ", 0x16), ("SET_INTERFACE_DS", 0x17),
    ("GET_PARTNER_PDO", 20), ("GET_BATTERY_STATUS", 21), ("SET_PDO", 22),
    ("GET_VDM", 23), ("SEND_VDM", 24),
]; // include/uapi/linux/usb/ch9.h:78-:111

#[test]
fn all_thirty_one_linux_standard_request_names_and_values_are_present() {
    assert_eq!(STANDARD_REQUESTS.len(), 31);
    let ours: Vec<(&str, u8)> = STANDARD_REQUESTS.iter().map(|x| (x.name, x.value)).collect();
    assert_eq!(ours, LINUX_STANDARD_REQUESTS);
}
