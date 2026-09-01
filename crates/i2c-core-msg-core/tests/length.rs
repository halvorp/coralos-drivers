// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for `i2c_msg` transfer-length bounds.
//!
//! Ported from Linux `drivers/i2c/i2c-core-base.c:2324-2343` and
//! `include/uapi/linux/i2c.h:47-51,74-87,141`. Original copyright holders: Simon G. Vogl,
//! Kyösti Mälkki, Frodo Looijaard, Wolfram Sang, and the Linux I2C authors.

use i2c_core_msg_core::flags::I2C_M_RECV_LEN;
use i2c_core_msg_core::length::*;

#[test]
fn u16_transfer_length_bound_is_exact_and_never_truncates() {
    assert_eq!(I2C_MSG_LEN_MAX, 0xffff); // include/uapi/linux/i2c.h:86
    assert_eq!(checked_message_length(0), Ok(0));
    assert_eq!(checked_message_length(0xffff), Ok(0xffff)); // i2c-core-base.c:2329
    assert_eq!(
        checked_message_length(0x1_0000),
        Err(LengthRefusal::TransferTooLong {
            length: 0x1_0000,
            maximum: 0xffff,
        })
    );
}

#[test]
fn ordinary_message_capacity_is_checked_by_name_and_bound() {
    assert_eq!(validate_message_length(4, 4, 0), Ok(4));
    assert_eq!(
        validate_message_length(4, 3, 0),
        Err(LengthRefusal::MessageBufferTooSmall {
            capacity: 3,
            minimum: 4,
        })
    );
}

#[test]
fn receive_length_requires_the_initial_length_byte() {
    // include/uapi/linux/i2c.h:35,47-51.
    assert_eq!(
        validate_message_length(0, 33, I2C_M_RECV_LEN),
        Err(LengthRefusal::ReceiveLengthNeedsInitialByte {
            length: 0,
            minimum: 1,
        })
    );
    assert_eq!(validate_message_length(1, 33, I2C_M_RECV_LEN), Ok(1));
}

#[test]
fn smbus_block_bound_and_both_capacity_boundaries_are_literal() {
    assert_eq!(I2C_SMBUS_BLOCK_MAX, 32); // include/uapi/linux/i2c.h:141
    assert_eq!(I2C_RECV_LEN_MIN_CAPACITY, 33); // length byte + 32 data bytes, :47-51,141
    assert_eq!(
        validate_message_length(1, 32, I2C_M_RECV_LEN),
        Err(LengthRefusal::ReceiveLengthBufferTooSmall {
            capacity: 32,
            minimum: 33,
        })
    );
    assert_eq!(validate_message_length(1, 33, I2C_M_RECV_LEN), Ok(1));
    assert_eq!(validate_message_length(1, 34, I2C_M_RECV_LEN), Ok(1));
}
