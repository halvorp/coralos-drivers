// SPDX-License-Identifier: GPL-2.0-only
//! SMBus packet-error-checking CRC-8 and message PEC handling.
//!
//! Ported from Linux `drivers/i2c/i2c-core-smbus.c:29-94,352-353,457-483`. Original copyright
//! holders: Frodo Looijaard, Mark Studebaker, Jean Delvare, and the Linux I2C authors.

use crate::protocol::Transaction;

/// Linux's shifted CRC polynomial used by its 16-bit bit-at-a-time helper.
pub const POLY: u16 = 0x1070 << 3; // drivers/i2c/i2c-core-smbus.c:29

/// Incremental SMBus CRC-8 over `bytes`, starting from `crc`.
///
/// This is the pure equivalent of Linux `i2c_smbus_pec` (`i2c-core-smbus.c:50-57`).
pub fn pec(mut crc: u8, bytes: &[u8]) -> u8 {
    for &byte in bytes {
        crc = crc8(((crc ^ byte) as u16) << 8);
    }
    crc
}

/// Compute PEC over a message's seven-bit wire-address byte followed by its data.
///
/// Linux explicitly assumes a seven-bit SMBus address (`i2c-core-smbus.c:60-68`). `read` chooses
/// the low wire-address direction bit.
pub fn message_pec(previous: u8, address: u8, read: bool, data: &[u8]) -> u8 {
    let wire_address = (address << 1) | u8::from(read);
    pec(pec(previous, &[wire_address]), data)
}

/// Whether Linux enables PEC for this transaction.
///
/// QUICK and I2C_BLOCK_DATA explicitly exclude PEC (`i2c-core-smbus.c:352-353`).
pub const fn wants_pec(client_pec: bool, transaction: Transaction) -> bool {
    client_pec && !matches!(transaction, Transaction::Quick | Transaction::I2cBlockData)
}

/// Why PEC processing refused a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PecRefusal {
    /// A write buffer had no spare byte at `length` for the PEC byte.
    AppendBufferTooSmall {
        /// Buffer capacity supplied by the caller.
        capacity: usize,
        /// Required capacity including PEC.
        minimum: usize,
    },
    /// A read message cannot contain the trailing PEC if its length is zero.
    MissingReceivedPec,
    /// Device PEC differs from the CRC calculated over address and payload.
    BadPacketErrorCode {
        /// PEC returned by the device.
        received: u8,
        /// PEC calculated by the host.
        expected: u8,
    },
}

/// Append PEC to a write message and return its new length.
///
/// Linux writes at `buf[len]` then increments `len` (`i2c-core-smbus.c:71-76`).
pub fn append_write_pec(
    address: u8,
    buffer: &mut [u8],
    length: usize,
) -> Result<usize, PecRefusal> {
    if length >= buffer.len() {
        return Err(PecRefusal::AppendBufferTooSmall {
            capacity: buffer.len(),
            minimum: length + 1,
        });
    }
    buffer[length] = message_pec(0, address, false, &buffer[..length]);
    Ok(length + 1)
}

/// Check and hide a read message's trailing PEC, returning payload length.
///
/// `partial` is the PEC of a preceding write message, if any. This mirrors Linux decrementing the
/// message length before computing/checking the CRC (`i2c-core-smbus.c:78-93`).
pub fn check_read_pec(
    partial: u8,
    address: u8,
    buffer: &[u8],
    length: usize,
) -> Result<usize, PecRefusal> {
    if length == 0 || length > buffer.len() {
        return Err(PecRefusal::MissingReceivedPec);
    }
    let payload_length = length - 1;
    let received = buffer[payload_length];
    let expected = message_pec(partial, address, true, &buffer[..payload_length]);
    if received != expected {
        return Err(PecRefusal::BadPacketErrorCode { received, expected });
    }
    Ok(payload_length)
}

fn crc8(mut data: u16) -> u8 {
    for _ in 0..8 {
        if data & 0x8000 != 0 {
            data ^= POLY;
        }
        data <<= 1;
    }
    (data >> 8) as u8
}
