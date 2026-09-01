// SPDX-License-Identifier: GPL-2.0-only
//! Control-transfer length, direction, and fixed-length receive validation.
//!
//! Ported from Linux `drivers/usb/core/message.c:275-:308` and
//! `drivers/usb/core/urb.c:400-:418`.
//!
//! Copyright (C) the Linux USB core authors.

use crate::{pipe, request::Direction, setup::SetupPacket};

/// Why a control transfer was refused before submission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// `wLength` and the supplied transfer-buffer length differ (`urb.c:414-:418`).
    LengthMismatch { setup_length: u16, transfer_length: usize },
    /// Linux warns when the control pipe direction and data-stage direction differ (`urb.c:409-:413`).
    DirectionMismatch { setup_direction: Direction, pipe_direction: Direction },
    /// `usb_control_msg_recv` rejects a zero-size receive (`message.c:283-:284`).
    ReceiveLengthZero { minimum: u16 },
    /// A fixed-length receive completed with a different length (`message.c:296-:301`).
    ReceiveLengthMismatch { expected: u16, actual: u16 },
}

/// Validate setup length and pipe direction as Linux's URB submission path does.
///
/// A zero-length setup is treated as OUT regardless of bit 7 (`urb.c:409-:410`): there is no data
/// stage whose direction could be IN.
pub const fn validate(setup: &SetupPacket, pipe_value: u32, transfer_length: usize) -> Result<(), ValidationError> {
    let setup_length = setup.length();
    if setup_length as usize != transfer_length {
        return Err(ValidationError::LengthMismatch { setup_length, transfer_length });
    }

    let setup_direction = if setup_length == 0 { Direction::Out } else { setup.direction() };
    let pipe_direction = pipe::direction(pipe_value);
    if setup_direction as u8 != pipe_direction as u8 {
        return Err(ValidationError::DirectionMismatch { setup_direction, pipe_direction });
    }
    Ok(())
}

/// Enforce `usb_control_msg_recv`'s nonzero and whole-message rules.
pub const fn validate_receive_completion(expected: u16, actual: u16) -> Result<(), ValidationError> {
    if expected == 0 {
        return Err(ValidationError::ReceiveLengthZero { minimum: 1 });
    }
    if actual != expected {
        return Err(ValidationError::ReceiveLengthMismatch { expected, actual });
    }
    Ok(())
}
