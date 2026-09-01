// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux SMBus PEC computation and placement.
//!
//! Ported from Linux `drivers/i2c/i2c-core-smbus.c:29-94,352-353`. Original copyright holders:
//! Frodo Looijaard, Mark Studebaker, Jean Delvare, and the Linux I2C authors.

use i2c_smbus_proto_core::pec::*;
use i2c_smbus_proto_core::protocol::Transaction;

#[test]
fn polynomial_and_incremental_crc8_vectors_are_exact() {
    assert_eq!(POLY, 0x8380); // `(0x1070U << 3)`, i2c-core-smbus.c:29
    assert_eq!(pec(0, &[]), 0x00);
    assert_eq!(pec(0, &[0x00]), 0x00);
    assert_eq!(pec(0, &[0x12, 0x34]), 0xf1);
    assert_eq!(pec(0, &[0x58, 0x10, 0x59, 0xcd]), 0x32);
    // Same bytes split at the write/read boundary: i2c_smbus_pec is incremental (:43-56).
    assert_eq!(pec(pec(0, &[0x58, 0x10]), &[0x59, 0xcd]), 0x32);
}

#[test]
fn message_pec_includes_wire_address_and_direction() {
    // Seven-bit address 0x2c gives 0x58 write and 0x59 read (:60-68).
    assert_eq!(message_pec(0, 0x2c, false, &[0x10, 0xab]), 0x7a);
    assert_eq!(message_pec(0, 0x2c, false, &[0x10]), 0xd4);
    assert_eq!(message_pec(0xd4, 0x2c, true, &[0xcd]), 0x32);
}

#[test]
fn pec_exclusions_cover_every_transaction_family_member() {
    // i2c-core-smbus.c:352-353 excludes exactly QUICK and I2C_BLOCK_DATA.
    assert!(!wants_pec(true, Transaction::Quick));
    assert!(wants_pec(true, Transaction::Byte));
    assert!(wants_pec(true, Transaction::ByteData));
    assert!(wants_pec(true, Transaction::WordData));
    assert!(wants_pec(true, Transaction::ProcCall));
    assert!(wants_pec(true, Transaction::BlockData));
    assert!(wants_pec(true, Transaction::BlockProcCall));
    assert!(!wants_pec(true, Transaction::I2cBlockData));
    assert!(!wants_pec(false, Transaction::WordData));
}

#[test]
fn write_only_pec_is_appended_after_payload() {
    let mut buffer = [0x10, 0xab, 0x00];
    assert_eq!(append_write_pec(0x2c, &mut buffer, 2), Ok(3));
    assert_eq!(buffer, [0x10, 0xab, 0x7a]); // address 0x58, command 0x10, data 0xab
    let mut full = [0x10, 0xab];
    assert_eq!(
        append_write_pec(0x2c, &mut full, 2),
        Err(PecRefusal::AppendBufferTooSmall {
            capacity: 2,
            minimum: 3
        })
    );
}

#[test]
fn read_pec_is_checked_then_hidden_from_the_payload_length() {
    assert_eq!(check_read_pec(0xd4, 0x2c, &[0xcd, 0x32], 2), Ok(1));
    assert_eq!(
        check_read_pec(0xd4, 0x2c, &[0xcd, 0x33], 2),
        Err(PecRefusal::BadPacketErrorCode {
            received: 0x33,
            expected: 0x32
        })
    );
    assert_eq!(
        check_read_pec(0, 0x2c, &[], 0),
        Err(PecRefusal::MissingReceivedPec)
    );
}
