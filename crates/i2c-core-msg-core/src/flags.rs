// SPDX-License-Identifier: GPL-2.0-only
//! `i2c_msg` flag values and semantics.
//!
//! Ported from Linux `include/uapi/linux/i2c.h:16-85`, originally copyrighted by Simon G. Vogl,
//! Kyösti Mälkki, and Frodo Looijaard.

/// A Linux `i2c_msg` flag and its source-level name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlagDefinition {
    /// Linux macro name.
    pub name: &'static str,
    /// Linux literal value.
    pub value: u16,
}

/// Read data from slave to master; absence means write.
pub const I2C_M_RD: u16 = 0x0001; // include/uapi/linux/i2c.h:77
/// Use a 10-bit slave address.
pub const I2C_M_TEN: u16 = 0x0010; // include/uapi/linux/i2c.h:78
/// First received byte supplies an SMBus block message's length.
pub const I2C_M_RECV_LEN: u16 = 0x0400; // include/uapi/linux/i2c.h:80
/// Skip the master's ACK/NACK bit in a read message.
pub const I2C_M_NO_RD_ACK: u16 = 0x0800; // include/uapi/linux/i2c.h:81
/// Treat a NACK from the client as an ACK.
pub const I2C_M_IGNORE_NAK: u16 = 0x1000; // include/uapi/linux/i2c.h:82
/// Skip the repeated START sequence.
pub const I2C_M_NOSTART: u16 = 0x4000; // include/uapi/linux/i2c.h:84
/// Force a STOP condition after this message.
pub const I2C_M_STOP: u16 = 0x8000; // include/uapi/linux/i2c.h:85

/// The seven message flags in this port's scope, in Linux declaration order.
pub const MESSAGE_FLAGS: [FlagDefinition; 7] = [
    FlagDefinition {
        name: "I2C_M_RD",
        value: I2C_M_RD,
    }, // include/uapi/linux/i2c.h:77
    FlagDefinition {
        name: "I2C_M_TEN",
        value: I2C_M_TEN,
    }, // include/uapi/linux/i2c.h:78
    FlagDefinition {
        name: "I2C_M_RECV_LEN",
        value: I2C_M_RECV_LEN,
    }, // include/uapi/linux/i2c.h:80
    FlagDefinition {
        name: "I2C_M_NO_RD_ACK",
        value: I2C_M_NO_RD_ACK,
    }, // include/uapi/linux/i2c.h:81
    FlagDefinition {
        name: "I2C_M_IGNORE_NAK",
        value: I2C_M_IGNORE_NAK,
    }, // include/uapi/linux/i2c.h:82
    FlagDefinition {
        name: "I2C_M_NOSTART",
        value: I2C_M_NOSTART,
    }, // include/uapi/linux/i2c.h:84
    FlagDefinition {
        name: "I2C_M_STOP",
        value: I2C_M_STOP,
    }, // include/uapi/linux/i2c.h:85
];

/// Whether a message is a read. If this flag is absent Linux interprets it as a write.
pub const fn is_read(flags: u16) -> bool {
    flags & I2C_M_RD != 0
}

/// Whether a message uses a 10-bit slave address.
pub const fn is_ten_bit(flags: u16) -> bool {
    flags & I2C_M_TEN != 0
}

/// Whether the first received byte supplies the block length.
pub const fn receives_length(flags: u16) -> bool {
    flags & I2C_M_RECV_LEN != 0
}

/// Whether the master ACK/NACK bit is skipped for a read.
pub const fn skips_read_ack(flags: u16) -> bool {
    flags & I2C_M_NO_RD_ACK != 0
}

/// Whether a client NACK is treated as ACK.
pub const fn ignores_nak(flags: u16) -> bool {
    flags & I2C_M_IGNORE_NAK != 0
}

/// Whether the repeated START before this message is skipped.
pub const fn omits_start(flags: u16) -> bool {
    flags & I2C_M_NOSTART != 0
}

/// Whether a STOP is forced after this message.
pub const fn forces_stop(flags: u16) -> bool {
    flags & I2C_M_STOP != 0
}
