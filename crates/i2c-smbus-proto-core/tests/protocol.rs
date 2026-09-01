// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for SMBus encodings and direction markers.
//!
//! Ported from Linux `include/uapi/linux/i2c.h:141,149-163`, `include/linux/i2c.h:333`, and
//! `drivers/i2c/i2c-core-smbus.c:356-455`. Original copyright holders: Simon G. Vogl, Kyösti
//! Mälkki, Frodo Looijaard, Mark Studebaker, Jean Delvare, and the Linux I2C authors.

use i2c_smbus_proto_core::protocol::*;

/// Written out literally: never generated from `TRANSACTIONS`, because that would let a deletion
/// erase both production and its test case.
const LINUX_EMULATED_NAMES: [&str; 8] = [
    "QUICK",
    "BYTE",
    "BYTE_DATA",
    "WORD_DATA",
    "PROC_CALL",
    "BLOCK_DATA",
    "BLOCK_PROC_CALL",
    "I2C_BLOCK_DATA",
]; // drivers/i2c/i2c-core-smbus.c:357-451

#[test]
fn transaction_count_names_and_encodings_match_linux_literals() {
    assert_eq!(TRANSACTION_COUNT, 8); // eight supported switch cases, i2c-core-smbus.c:357-451
    assert_eq!(TRANSACTIONS.len(), 8);
    let names: Vec<&str> = TRANSACTIONS.iter().map(|entry| entry.name).collect();
    assert_eq!(names, LINUX_EMULATED_NAMES);
    let values: Vec<u8> = TRANSACTIONS.iter().map(|entry| entry.value).collect();
    assert_eq!(values, [0, 1, 2, 3, 4, 5, 7, 8]); // include/uapi/linux/i2c.h:155-163
}

#[test]
fn every_encoding_drives_the_member_it_names() {
    assert_eq!(transaction_from_encoding(0), Some(Transaction::Quick));
    assert_eq!(transaction_from_encoding(1), Some(Transaction::Byte));
    assert_eq!(transaction_from_encoding(2), Some(Transaction::ByteData));
    assert_eq!(transaction_from_encoding(3), Some(Transaction::WordData));
    assert_eq!(transaction_from_encoding(4), Some(Transaction::ProcCall));
    assert_eq!(transaction_from_encoding(5), Some(Transaction::BlockData));
    assert_eq!(
        transaction_from_encoding(7),
        Some(Transaction::BlockProcCall)
    );
    assert_eq!(
        transaction_from_encoding(8),
        Some(Transaction::I2cBlockData)
    );
    assert_eq!(transaction_from_encoding(6), None); // I2C_BLOCK_BROKEN, uapi i2c.h:161
    assert_eq!(transaction_from_encoding(9), None); // unsupported default, smbus.c:452-454
}

#[test]
fn zero_valued_write_and_quick_are_pinned_by_behavior_not_only_value() {
    assert_eq!(I2C_SMBUS_WRITE, 0); // include/uapi/linux/i2c.h:151
    assert_eq!(Transaction::Quick as u8, 0); // include/uapi/linux/i2c.h:155
    assert_ne!(I2C_SMBUS_WRITE, I2C_SMBUS_READ);
    assert_ne!(Transaction::Quick as u8, Transaction::Byte as u8);
    assert_eq!(Direction::from_marker(0), Some(Direction::Write));
    assert_eq!(transaction_from_encoding(0), Some(Transaction::Quick));
}

#[test]
fn direction_markers_round_trip_and_reject_non_markers() {
    assert_eq!(I2C_SMBUS_READ, 1); // include/uapi/linux/i2c.h:150
    assert_eq!(Direction::from_marker(0), Some(Direction::Write));
    assert_eq!(Direction::from_marker(1), Some(Direction::Read));
    assert_eq!(Direction::from_marker(2), None);
    assert_eq!(Direction::Write.marker(), 0);
    assert_eq!(Direction::Read.marker(), 1);
}

#[test]
fn block_and_client_pec_literals_are_exact() {
    assert_eq!(I2C_SMBUS_BLOCK_MAX, 32); // include/uapi/linux/i2c.h:141
    assert_eq!(I2C_CLIENT_PEC, 0x04); // include/linux/i2c.h:333
}
