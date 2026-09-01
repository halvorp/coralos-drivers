// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux control length and direction validation.
//!
//! Ported from Linux `drivers/usb/core/message.c:275-:308` and
//! `drivers/usb/core/urb.c:400-:418`.
//!
//! Copyright (C) the Linux USB core authors.

use usb_control_msg_core::{
    pipe,
    request::{Direction, Recipient, RequestType},
    setup::SetupPacket,
    validation::{validate, validate_receive_completion, ValidationError},
};

fn setup(direction: Direction, length: u16) -> SetupPacket {
    SetupPacket::new(direction, RequestType::Standard, Recipient::Device, 0x00, 0, 0, length)
}

#[test]
fn equal_setup_and_buffer_lengths_are_accepted_in_both_directions() {
    assert_eq!(validate(&setup(Direction::Out, 8), pipe::send_control(1, 0).unwrap(), 8), Ok(()));
    assert_eq!(validate(&setup(Direction::In, 8), pipe::receive_control(1, 0).unwrap(), 8), Ok(()));
}

#[test]
fn unequal_length_names_both_values() {
    assert_eq!(
        validate(&setup(Direction::In, 8), pipe::receive_control(1, 0).unwrap(), 7),
        Err(ValidationError::LengthMismatch { setup_length: 8, transfer_length: 7 })
    ); // urb.c:414-:418 returns -EBADR
}

#[test]
fn direction_mismatch_names_setup_and_pipe_directions() {
    assert_eq!(
        validate(&setup(Direction::In, 8), pipe::send_control(1, 0).unwrap(), 8),
        Err(ValidationError::DirectionMismatch {
            setup_direction: Direction::In,
            pipe_direction: Direction::Out,
        })
    ); // urb.c:409-:413, "BOGUS control dir..."
    assert_eq!(
        validate(&setup(Direction::Out, 8), pipe::receive_control(1, 0).unwrap(), 8),
        Err(ValidationError::DirectionMismatch {
            setup_direction: Direction::Out,
            pipe_direction: Direction::In,
        })
    );
}

#[test]
fn zero_length_has_no_in_data_stage_and_linux_treats_it_as_out() {
    // `is_out = !(bRequestType & USB_DIR_IN) || !wLength`, urb.c:409-:410.
    assert_eq!(validate(&setup(Direction::In, 0), pipe::send_control(1, 0).unwrap(), 0), Ok(()));
    assert_eq!(
        validate(&setup(Direction::In, 0), pipe::receive_control(1, 0).unwrap(), 0),
        Err(ValidationError::DirectionMismatch {
            setup_direction: Direction::Out,
            pipe_direction: Direction::In,
        })
    );
}

#[test]
fn fixed_receive_requires_nonzero_and_exact_completion() {
    assert_eq!(
        validate_receive_completion(0, 0),
        Err(ValidationError::ReceiveLengthZero { minimum: 1 })
    ); // message.c:283-:284
    assert_eq!(validate_receive_completion(8, 8), Ok(())); // message.c:296-:299
    assert_eq!(
        validate_receive_completion(8, 7),
        Err(ValidationError::ReceiveLengthMismatch { expected: 8, actual: 7 })
    ); // message.c:299-:301, -EREMOTEIO
    assert_eq!(
        validate_receive_completion(8, 9),
        Err(ValidationError::ReceiveLengthMismatch { expected: 8, actual: 9 })
    );
}
