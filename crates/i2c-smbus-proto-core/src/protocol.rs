// SPDX-License-Identifier: GPL-2.0-only
//! SMBus transaction-size encodings and transfer direction markers.
//!
//! Ported from Linux `include/uapi/linux/i2c.h:149-163` and consumed by
//! `drivers/i2c/i2c-core-smbus.c:323-455`. Original copyright holders: Simon G. Vogl,
//! Kyösti Mälkki, Frodo Looijaard, Mark Studebaker, Jean Delvare, and the Linux I2C authors.

/// SMBus write marker.
pub const I2C_SMBUS_WRITE: u8 = 0; // include/uapi/linux/i2c.h:151
/// SMBus read marker.
pub const I2C_SMBUS_READ: u8 = 1; // include/uapi/linux/i2c.h:150
/// Maximum payload bytes in an SMBus block.
pub const I2C_SMBUS_BLOCK_MAX: usize = 32; // include/uapi/linux/i2c.h:141
/// Client requests packet error checking.
pub const I2C_CLIENT_PEC: u16 = 0x04; // include/linux/i2c.h:333

/// A Linux SMBus transaction encoding and source-level name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionDefinition {
    /// Linux macro name without the `I2C_SMBUS_` prefix.
    pub name: &'static str,
    /// Linux's integer encoding.
    pub value: u8,
}

/// Transaction type accepted by Linux's plain-I2C SMBus emulator.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transaction {
    /// Address phase only; the direction bit itself is the data.
    Quick = 0, // include/uapi/linux/i2c.h:155
    /// Send byte or receive byte.
    Byte = 1, // include/uapi/linux/i2c.h:156
    /// Command plus one data byte.
    ByteData = 2, // include/uapi/linux/i2c.h:157
    /// Command plus a little-endian data word.
    WordData = 3, // include/uapi/linux/i2c.h:158
    /// Write a word, repeated START, then read a word.
    ProcCall = 4, // include/uapi/linux/i2c.h:159
    /// SMBus block transaction with a device-supplied/read length byte.
    BlockData = 5, // include/uapi/linux/i2c.h:160
    /// Write a block, repeated START, then read a device-sized block.
    BlockProcCall = 7, // include/uapi/linux/i2c.h:162
    /// I2C-style block with a caller-supplied fixed read length and no wire length byte on write.
    I2cBlockData = 8, // include/uapi/linux/i2c.h:163
}

/// Number of transaction types supported by `i2c_smbus_xfer_emulated`.
pub const TRANSACTION_COUNT: usize = 8; // drivers/i2c/i2c-core-smbus.c:357-451

/// Every emulated transaction in Linux switch order.
pub const TRANSACTIONS: [TransactionDefinition; TRANSACTION_COUNT] = [
    TransactionDefinition {
        name: "QUICK",
        value: Transaction::Quick as u8,
    }, // i2c-core-smbus.c:357
    TransactionDefinition {
        name: "BYTE",
        value: Transaction::Byte as u8,
    }, // i2c-core-smbus.c:364
    TransactionDefinition {
        name: "BYTE_DATA",
        value: Transaction::ByteData as u8,
    }, // i2c-core-smbus.c:371
    TransactionDefinition {
        name: "WORD_DATA",
        value: Transaction::WordData as u8,
    }, // i2c-core-smbus.c:379
    TransactionDefinition {
        name: "PROC_CALL",
        value: Transaction::ProcCall as u8,
    }, // i2c-core-smbus.c:388
    TransactionDefinition {
        name: "BLOCK_DATA",
        value: Transaction::BlockData as u8,
    }, // i2c-core-smbus.c:396
    TransactionDefinition {
        name: "BLOCK_PROC_CALL",
        value: Transaction::BlockProcCall as u8,
    }, // i2c-core-smbus.c:415
    TransactionDefinition {
        name: "I2C_BLOCK_DATA",
        value: Transaction::I2cBlockData as u8,
    }, // i2c-core-smbus.c:434
];

/// Convert Linux's integer transaction-size encoding.
///
/// Encoding 6 (`I2C_SMBUS_I2C_BLOCK_BROKEN`) is intentionally unsupported, as is Linux's
/// emulator default branch (`i2c-core-smbus.c:452-454`).
pub const fn transaction_from_encoding(value: u8) -> Option<Transaction> {
    match value {
        0 => Some(Transaction::Quick),
        1 => Some(Transaction::Byte),
        2 => Some(Transaction::ByteData),
        3 => Some(Transaction::WordData),
        4 => Some(Transaction::ProcCall),
        5 => Some(Transaction::BlockData),
        7 => Some(Transaction::BlockProcCall),
        8 => Some(Transaction::I2cBlockData),
        _ => None,
    }
}

/// Direction requested by the SMBus caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Master writes to the device.
    Write,
    /// Master reads from the device.
    Read,
}

impl Direction {
    /// Decode Linux's `I2C_SMBUS_WRITE`/`I2C_SMBUS_READ` marker.
    pub const fn from_marker(marker: u8) -> Option<Self> {
        match marker {
            I2C_SMBUS_WRITE => Some(Self::Write),
            I2C_SMBUS_READ => Some(Self::Read),
            _ => None,
        }
    }

    /// Return Linux's marker for this direction.
    pub const fn marker(self) -> u8 {
        match self {
            Self::Write => I2C_SMBUS_WRITE,
            Self::Read => I2C_SMBUS_READ,
        }
    }
}
