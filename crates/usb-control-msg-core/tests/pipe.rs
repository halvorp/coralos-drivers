// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux pipe vectors.
//!
//! Ported from Linux `include/linux/usb.h:1987-:2042`.
//!
//! Copyright (C) the Linux USB core authors.

use usb_control_msg_core::{
    pipe::{self, PipeError, PipeType, DEVICE_MASK, DIRECTION_MASK, ENDPOINT_MASK, PIPE_TYPES, TYPE_MASK},
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
fn every_pipe_mask_matches_its_linux_literal() {
    assert_eq!(DIRECTION_MASK, 0x0000_0080); // include/linux/usb.h:1990-:1992,2008
    assert_eq!(DEVICE_MASK, 0x0000_7f00); // include/linux/usb.h:1993,2011
    assert_eq!(ENDPOINT_MASK, 0x0007_8000); // include/linux/usb.h:1994,2012
    assert_eq!(TYPE_MASK, 0xc000_0000); // include/linux/usb.h:1995-:1996,2014
}

#[test]
fn pipe_fields_encode_without_overlap_and_decode() {
    // control=2 at bits 31:30, endpoint 0xf at 18:15, device 0x7f at 14:8, IN at bit 7.
    let raw = pipe::encode(0x7f, 0x0f, Direction::In, PipeType::Control).unwrap();
    assert_eq!(raw & DIRECTION_MASK, 0x0000_0080); // include/linux/usb.h:1990-:1992,2008
    assert_eq!(raw & DEVICE_MASK, 0x0000_7f00); // include/linux/usb.h:1993,2011
    assert_eq!(raw & ENDPOINT_MASK, 0x0007_8000); // include/linux/usb.h:1994,2012
    assert_eq!(raw & TYPE_MASK, 0x8000_0000); // include/linux/usb.h:1995-:1996,2005,2014
    assert_eq!(raw, 0x8007_ff80); // include/linux/usb.h:1990-:1996,2020-:2024
    assert_eq!(pipe::direction(raw), Direction::In);
    assert_eq!(pipe::device_address(raw), 0x7f);
    assert_eq!(pipe::endpoint(raw), 0x0f);
    assert_eq!(pipe::pipe_type(raw), PipeType::Control);
}

#[test]
fn send_and_receive_control_pipes_differ_only_at_direction_bit() {
    let send = pipe::send_control(5, 3).unwrap();
    let receive = pipe::receive_control(5, 3).unwrap();
    assert_eq!(send, 0x8001_8500); // include/linux/usb.h:2027-:2028
    assert_eq!(receive, 0x8001_8580); // include/linux/usb.h:2029-:2030
    assert_eq!(send & DIRECTION_MASK, 0x00); // include/uapi/linux/usb/ch9.h:47
    assert_eq!(receive & DIRECTION_MASK, 0x80); // include/uapi/linux/usb/ch9.h:48
    assert_eq!(send ^ receive, DIRECTION_MASK);
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
