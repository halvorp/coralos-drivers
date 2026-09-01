// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux pipe vectors.
//!
//! Ported from Linux `include/linux/usb.h:1987-:2042`.
//!
//! Copyright (C) the Linux USB core authors.

use usb_control_msg_core::{
    pipe::{self, PipeError, PipeType, PIPE_TYPES},
    request::Direction,
};

const LINUX_PIPE_TYPES: [(&str, u8); 4] = [
    ("ISOCHRONOUS", 0), ("INTERRUPT", 1), ("CONTROL", 2), ("BULK", 3),
]; // include/linux/usb.h:2003-:2006

#[test]
fn all_four_pipe_type_names_and_values_are_pinned() {
    assert_eq!(PIPE_TYPES.len(), 4);
    assert_eq!(PIPE_TYPES.iter().map(|x| (x.name, x.value)).collect::<Vec<_>>(), LINUX_PIPE_TYPES);
}

#[test]
fn pipe_fields_encode_without_overlap_and_decode() {
    // control=2 at bits 31:30, endpoint 0xf at 18:15, device 0x7f at 14:8, IN at bit 7.
    let raw = pipe::encode(0x7f, 0x0f, Direction::In, PipeType::Control).unwrap();
    assert_eq!(raw, 0x8007_ff80); // include/linux/usb.h:1993-:1998,2020-:2024
    assert_eq!(pipe::direction(raw), Direction::In);
    assert_eq!(pipe::device_address(raw), 0x7f);
    assert_eq!(pipe::endpoint(raw), 0x0f);
    assert_eq!(pipe::pipe_type(raw), PipeType::Control);
}

#[test]
fn send_and_receive_control_pipes_differ_only_at_direction_bit() {
    assert_eq!(pipe::send_control(5, 3), Ok(0x8001_8500)); // usb.h:2027-:2028
    assert_eq!(pipe::receive_control(5, 3), Ok(0x8001_8580)); // usb.h:2029-:2030
    assert_eq!(0x8001_8500u32 ^ 0x8001_8580u32, 0x80); // USB_DIR_IN, ch9.h:48
}

#[test]
fn encoder_names_values_and_bounds_that_refused() {
    assert_eq!(
        pipe::encode(0x80, 0, Direction::Out, PipeType::Control),
        Err(PipeError::DeviceAddressOutOfRange { value: 0x80, maximum: 0x7f })
    );
    assert_eq!(
        pipe::encode(1, 0x10, Direction::Out, PipeType::Control),
        Err(PipeError::EndpointOutOfRange { value: 0x10, maximum: 0x0f })
    );
}

#[test]
fn all_pipe_type_decode_arms_are_reachable() {
    assert_eq!(pipe::pipe_type(0x0000_0000), PipeType::Isochronous);
    assert_eq!(pipe::pipe_type(0x4000_0000), PipeType::Interrupt);
    assert_eq!(pipe::pipe_type(0x8000_0000), PipeType::Control);
    assert_eq!(pipe::pipe_type(0xc000_0000), PipeType::Bulk);
}
