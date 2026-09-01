// SPDX-License-Identifier: GPL-2.0-only
//! I2C address validation, reserved ranges, and wire encoding.
//!
//! Ported from Linux `drivers/i2c/i2c-core-base.c:51-57,751-783` and
//! `include/linux/i2c.h:948-965`. Original copyright holders: Simon G. Vogl, Kyösti Mälkki,
//! Frodo Looijaard, Rodolfo Giometti, Michael Lawnick, Wolfram Sang, and the Linux I2C authors.

use crate::flags::{is_read, is_ten_bit};

/// Highest non-reserved strict 7-bit address.
pub const I2C_ADDR_7BITS_MAX: u16 = 0x77; // drivers/i2c/i2c-core-base.c:54
/// Number of addresses through the highest strict 7-bit address, including zero.
pub const I2C_ADDR_7BITS_COUNT: u16 = 0x78; // drivers/i2c/i2c-core-base.c:55
/// 10-bit address upper bound (`0x000..=0x3ff`).
pub const I2C_ADDR_10BITS_MAX: u16 = 0x03ff; // drivers/i2c/i2c-core-base.c:756
/// First strict, non-reserved 7-bit address.
pub const I2C_ADDR_7BITS_MIN_STRICT: u16 = 0x08; // drivers/i2c/i2c-core-base.c:780

/// One reserved 7-bit address range and Linux's reason for reserving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservedRange {
    /// Inclusive first address.
    pub first: u16,
    /// Inclusive last address.
    pub last: u16,
    /// Linux's address-map name.
    pub name: &'static str,
}

/// Every strict reserved 7-bit range, split by Linux's named protocol purpose.
pub const RESERVED_7BIT_RANGES: [ReservedRange; 7] = [
    ReservedRange {
        first: 0x00,
        last: 0x00,
        name: "General call address / START byte",
    }, // drivers/i2c/i2c-core-base.c:773
    ReservedRange {
        first: 0x01,
        last: 0x01,
        name: "CBUS address",
    }, // drivers/i2c/i2c-core-base.c:774
    ReservedRange {
        first: 0x02,
        last: 0x02,
        name: "Reserved for different bus format",
    }, // drivers/i2c/i2c-core-base.c:775
    ReservedRange {
        first: 0x03,
        last: 0x03,
        name: "Reserved for future purposes",
    }, // drivers/i2c/i2c-core-base.c:776
    ReservedRange {
        first: 0x04,
        last: 0x07,
        name: "Hs-mode master code",
    }, // drivers/i2c/i2c-core-base.c:777
    ReservedRange {
        first: 0x78,
        last: 0x7b,
        name: "10-bit slave addressing",
    }, // drivers/i2c/i2c-core-base.c:778
    ReservedRange {
        first: 0x7c,
        last: 0x7f,
        name: "Reserved for future purposes",
    }, // drivers/i2c/i2c-core-base.c:779
];

/// Why an I2C address was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressRefusal {
    /// A 7-bit value exceeds `0x7f`.
    SevenBitOutOfRange {
        /// Refused address.
        address: u16,
        /// Inclusive upper bound.
        maximum: u16,
    },
    /// A 10-bit value exceeds `0x3ff`.
    TenBitOutOfRange {
        /// Refused address.
        address: u16,
        /// Inclusive upper bound.
        maximum: u16,
    },
    /// Strict validation refused a protocol-reserved range.
    ReservedSevenBit {
        /// Refused address.
        address: u16,
        /// Inclusive range start.
        first: u16,
        /// Inclusive range end.
        last: u16,
        /// Named protocol reason.
        reason: &'static str,
    },
}

/// Validated address mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    /// Ordinary 7-bit address.
    SevenBit,
    /// 10-bit address selected by `I2C_M_TEN`.
    TenBit,
}

/// Apply Linux's permissive numeric address check.
///
/// Linux uses this when registering an explicitly enumerated client: 10-bit accepts every value
/// through `0x3ff`; 7-bit accepts through `0x7f` but refuses general call `0x00`. It deliberately
/// does not enforce the rest of the address-map constraints (i2c-core-base.c:748-763). Ordinary
/// bus traffic should use [`validate_address`] or [`validate_7bit_strict`] instead.
pub const fn validate_address_permissive(
    address: u16,
    flags: u16,
) -> Result<AddressMode, AddressRefusal> {
    if is_ten_bit(flags) {
        if address > I2C_ADDR_10BITS_MAX {
            Err(AddressRefusal::TenBitOutOfRange {
                address,
                maximum: I2C_ADDR_10BITS_MAX,
            })
        } else {
            Ok(AddressMode::TenBit)
        }
    } else if address > 0x7f {
        Err(AddressRefusal::SevenBitOutOfRange {
            address,
            maximum: 0x7f,
        })
    } else if address == 0x00 {
        Err(AddressRefusal::ReservedSevenBit {
            address,
            first: 0x00,
            last: 0x00,
            reason: "General call address / START byte",
        })
    } else {
        Ok(AddressMode::SevenBit)
    }
}

/// Validate the address mode selected by flags, enforcing strict reserved ranges for 7-bit use.
///
/// Ten-bit addresses have no reserved values and accept `0x000..=0x3ff` (i2c-core-base.c:754-757).
/// Seven-bit traffic is limited to `0x08..=0x77` by the strict Linux rule (:766-783).
pub fn validate_address(address: u16, flags: u16) -> Result<AddressMode, AddressRefusal> {
    if is_ten_bit(flags) {
        validate_address_permissive(address, flags)
    } else {
        validate_7bit_strict(address)?;
        Ok(AddressMode::SevenBit)
    }
}

/// Strictly validate a 7-bit address for ordinary/probed use.
///
/// Linux allows only `0x08..=0x77`; the reserved ranges are split here so every refusal names the
/// bus-protocol purpose that would otherwise be emitted onto the shared bus.
pub fn validate_7bit_strict(address: u16) -> Result<(), AddressRefusal> {
    if address > 0x7f {
        return Err(AddressRefusal::SevenBitOutOfRange {
            address,
            maximum: 0x7f,
        });
    }
    for range in RESERVED_7BIT_RANGES {
        if address >= range.first && address <= range.last {
            return Err(AddressRefusal::ReservedSevenBit {
                address,
                first: range.first,
                last: range.last,
                reason: range.name,
            });
        }
    }
    Ok(())
}

/// Encode a strictly validated 7-bit address and the message direction as the wire address byte.
///
/// Linux's arithmetic is `(addr << 1) | I2C_M_RD` (include/linux/i2c.h:948-951). This safe port
/// first applies Linux's strict reserved-address check (i2c-core-base.c:769-783), so a caller cannot
/// accidentally put a protocol-reserved address on a shared bus.
pub fn encode_7bit(address: u16, flags: u16) -> Result<u8, AddressRefusal> {
    validate_7bit_strict(address)?;
    Ok(((address << 1) as u8) | (is_read(flags) as u8))
}

/// Encode a validated 10-bit address as `(high address byte, low address byte)`.
///
/// The high byte is `11110 A9 A8 R/W`; the low byte is `A7..A0` (include/linux/i2c.h:953-965).
pub const fn encode_10bit(address: u16, flags: u16) -> Result<(u8, u8), AddressRefusal> {
    if address > I2C_ADDR_10BITS_MAX {
        return Err(AddressRefusal::TenBitOutOfRange {
            address,
            maximum: I2C_ADDR_10BITS_MAX,
        });
    }
    let high = 0xf0 | (((address & 0x0300) >> 7) as u8) | (is_read(flags) as u8);
    Ok((high, (address & 0x00ff) as u8))
}
