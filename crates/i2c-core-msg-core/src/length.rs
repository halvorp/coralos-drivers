// SPDX-License-Identifier: GPL-2.0-only
//! `i2c_msg` transfer-length bounds.
//!
//! Ported from Linux `drivers/i2c/i2c-core-base.c:2324-2343` and
//! `include/uapi/linux/i2c.h:47-51,74-87,141`. Original copyright holders: Simon G. Vogl,
//! Kyösti Mälkki, Frodo Looijaard, Wolfram Sang, and the Linux I2C authors.

use crate::flags::receives_length;

/// Largest value representable by Linux `struct i2c_msg.len` (`__u16`).
pub const I2C_MSG_LEN_MAX: usize = 0xffff; // include/uapi/linux/i2c.h:86
/// SMBus block-data maximum, excluding its initial length byte.
pub const I2C_SMBUS_BLOCK_MAX: usize = 32; // include/uapi/linux/i2c.h:141
/// Required receive capacity for length byte plus maximum SMBus block data.
pub const I2C_RECV_LEN_MIN_CAPACITY: usize = 33; // include/uapi/linux/i2c.h:47-51,141

/// Why message length metadata was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LengthRefusal {
    /// Requested transfer cannot fit in Linux's `u16` message length.
    TransferTooLong {
        /// Refused byte count.
        length: usize,
        /// Inclusive `u16` bound.
        maximum: usize,
    },
    /// `I2C_M_RECV_LEN` has no initial length byte to receive.
    ReceiveLengthNeedsInitialByte {
        /// Refused initial message length.
        length: usize,
        /// Minimum initial message length.
        minimum: usize,
    },
    /// An ordinary message buffer cannot hold its declared transfer length.
    MessageBufferTooSmall {
        /// Refused buffer capacity.
        capacity: usize,
        /// Declared message length.
        minimum: usize,
    },
    /// `I2C_M_RECV_LEN` buffer cannot hold length byte plus 32 block bytes.
    ReceiveLengthBufferTooSmall {
        /// Refused buffer capacity.
        capacity: usize,
        /// Minimum capacity, excluding optional PEC.
        minimum: usize,
    },
}

/// Convert a caller byte count to Linux's `u16` `i2c_msg.len` without truncation.
pub const fn checked_message_length(length: usize) -> Result<u16, LengthRefusal> {
    if length > I2C_MSG_LEN_MAX {
        Err(LengthRefusal::TransferTooLong {
            length,
            maximum: I2C_MSG_LEN_MAX,
        })
    } else {
        Ok(length as u16)
    }
}

/// Validate transfer length and buffer capacity, including `I2C_M_RECV_LEN` semantics.
///
/// For ordinary messages `capacity` must be at least `length`. For a receive-length message Linux
/// requires room for the initial length byte and 32 block bytes; optional PEC needs one additional
/// caller-provided byte and is deliberately outside this core bound.
pub const fn validate_message_length(
    length: usize,
    capacity: usize,
    flags: u16,
) -> Result<u16, LengthRefusal> {
    let encoded = match checked_message_length(length) {
        Ok(value) => value,
        Err(error) => return Err(error),
    };

    if receives_length(flags) {
        if length < 1 {
            return Err(LengthRefusal::ReceiveLengthNeedsInitialByte { length, minimum: 1 });
        }
        if capacity < I2C_RECV_LEN_MIN_CAPACITY {
            return Err(LengthRefusal::ReceiveLengthBufferTooSmall {
                capacity,
                minimum: I2C_RECV_LEN_MIN_CAPACITY,
            });
        }
    } else if capacity < length {
        return Err(LengthRefusal::MessageBufferTooSmall {
            capacity,
            minimum: length,
        });
    }

    Ok(encoded)
}
