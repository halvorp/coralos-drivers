// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for I2C address validation and wire encoding.
//!
//! Ported from Linux `drivers/i2c/i2c-core-base.c:51-57,751-783` and
//! `include/linux/i2c.h:948-965`. Original copyright holders: Simon G. Vogl, Kyösti Mälkki,
//! Frodo Looijaard, Rodolfo Giometti, Michael Lawnick, Wolfram Sang, and Linux I2C authors.

use i2c_core_msg_core::address::*;
use i2c_core_msg_core::flags::{I2C_M_RD, I2C_M_TEN};

/// Literal expected ranges; never generated from `RESERVED_7BIT_RANGES`.
const LINUX_RESERVED_RANGES: [(u16, u16, &str); 7] = [
    (0x00, 0x00, "General call address / START byte"), // drivers/i2c/i2c-core-base.c:773
    (0x01, 0x01, "CBUS address"),                      // drivers/i2c/i2c-core-base.c:774
    (0x02, 0x02, "Reserved for different bus format"), // drivers/i2c/i2c-core-base.c:775
    (0x03, 0x03, "Reserved for future purposes"),      // drivers/i2c/i2c-core-base.c:776
    (0x04, 0x07, "Hs-mode master code"),               // drivers/i2c/i2c-core-base.c:777
    (0x78, 0x7b, "10-bit slave addressing"),           // drivers/i2c/i2c-core-base.c:778
    (0x7c, 0x7f, "Reserved for future purposes"),      // drivers/i2c/i2c-core-base.c:779
];

#[test]
fn all_seven_reserved_ranges_are_pinned_by_count_name_and_value() {
    assert_eq!(RESERVED_7BIT_RANGES.len(), 7);
    let got: Vec<(u16, u16, &str)> = RESERVED_7BIT_RANGES
        .iter()
        .map(|range| (range.first, range.last, range.name))
        .collect();
    assert_eq!(got, LINUX_RESERVED_RANGES);
    assert_eq!(I2C_ADDR_7BITS_MIN_STRICT, 0x08); // drivers/i2c/i2c-core-base.c:780
    assert_eq!(I2C_ADDR_7BITS_MAX, 0x77); // drivers/i2c/i2c-core-base.c:54
    assert_eq!(I2C_ADDR_7BITS_COUNT, 0x78); // drivers/i2c/i2c-core-base.c:55
    assert_eq!(I2C_ADDR_10BITS_MAX, 0x03ff); // drivers/i2c/i2c-core-base.c:756
}

#[test]
fn every_low_reserved_address_is_refused_by_value_and_named() {
    let expected = [
        (0x00, 0x00, 0x00, "General call address / START byte"),
        (0x01, 0x01, 0x01, "CBUS address"),
        (0x02, 0x02, 0x02, "Reserved for different bus format"),
        (0x03, 0x03, 0x03, "Reserved for future purposes"),
        (0x04, 0x04, 0x07, "Hs-mode master code"),
        (0x05, 0x04, 0x07, "Hs-mode master code"),
        (0x06, 0x04, 0x07, "Hs-mode master code"),
        (0x07, 0x04, 0x07, "Hs-mode master code"),
    ]; // drivers/i2c/i2c-core-base.c:773-777
    for (address, first, last, reason) in expected {
        assert_eq!(
            validate_7bit_strict(address),
            Err(AddressRefusal::ReservedSevenBit {
                address,
                first,
                last,
                reason
            })
        );
    }
}

#[test]
fn every_high_reserved_address_is_refused_by_value_and_named() {
    let expected = [
        (0x78, 0x78, 0x7b, "10-bit slave addressing"),
        (0x79, 0x78, 0x7b, "10-bit slave addressing"),
        (0x7a, 0x78, 0x7b, "10-bit slave addressing"),
        (0x7b, 0x78, 0x7b, "10-bit slave addressing"),
        (0x7c, 0x7c, 0x7f, "Reserved for future purposes"),
        (0x7d, 0x7c, 0x7f, "Reserved for future purposes"),
        (0x7e, 0x7c, 0x7f, "Reserved for future purposes"),
        (0x7f, 0x7c, 0x7f, "Reserved for future purposes"),
    ]; // drivers/i2c/i2c-core-base.c:778-779
    for (address, first, last, reason) in expected {
        assert_eq!(
            validate_7bit_strict(address),
            Err(AddressRefusal::ReservedSevenBit {
                address,
                first,
                last,
                reason
            })
        );
    }
}

#[test]
fn strict_boundaries_on_both_sides_of_each_outer_reserved_range_are_pinned() {
    assert_eq!(
        validate_7bit_strict(0x07),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x07,
            first: 0x04,
            last: 0x07,
            reason: "Hs-mode master code"
        })
    );
    assert_eq!(validate_7bit_strict(0x08), Ok(())); // i2c-core-base.c:780
    assert_eq!(validate_7bit_strict(0x77), Ok(())); // i2c-core-base.c:780
    assert_eq!(
        validate_7bit_strict(0x78),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x78,
            first: 0x78,
            last: 0x7b,
            reason: "10-bit slave addressing"
        })
    );
    assert_eq!(
        validate_7bit_strict(0x7f),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x7f,
            first: 0x7c,
            last: 0x7f,
            reason: "Reserved for future purposes"
        })
    );
    assert_eq!(
        validate_7bit_strict(0x80),
        Err(AddressRefusal::SevenBitOutOfRange {
            address: 0x80,
            maximum: 0x7f
        })
    );
}

#[test]
fn permissive_width_validation_matches_linux() {
    assert_eq!(
        validate_address_permissive(0x01, 0),
        Ok(AddressMode::SevenBit)
    ); // i2c-core-base.c:759-761
    assert_eq!(
        validate_address_permissive(0x7f, 0),
        Ok(AddressMode::SevenBit)
    );
    assert_eq!(
        validate_address_permissive(0x00, 0),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x00,
            first: 0x00,
            last: 0x00,
            reason: "General call address / START byte"
        })
    );
    assert_eq!(
        validate_address_permissive(0x80, 0),
        Err(AddressRefusal::SevenBitOutOfRange {
            address: 0x80,
            maximum: 0x7f
        })
    );
    assert_eq!(
        validate_address_permissive(0x000, I2C_M_TEN),
        Ok(AddressMode::TenBit)
    ); // i2c-core-base.c:754-757
    assert_eq!(
        validate_address_permissive(0x3ff, I2C_M_TEN),
        Ok(AddressMode::TenBit)
    );
    assert_eq!(
        validate_address_permissive(0x400, I2C_M_TEN),
        Err(AddressRefusal::TenBitOutOfRange {
            address: 0x400,
            maximum: 0x3ff
        })
    );
}

#[test]
fn mode_selected_validation_is_strict_for_seven_bit_and_full_range_for_ten_bit() {
    assert_eq!(
        validate_address(0x07, 0),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x07,
            first: 0x04,
            last: 0x07,
            reason: "Hs-mode master code"
        })
    ); // drivers/i2c/i2c-core-base.c:766-783
    assert_eq!(validate_address(0x08, 0), Ok(AddressMode::SevenBit));
    assert_eq!(validate_address(0x000, I2C_M_TEN), Ok(AddressMode::TenBit));
    assert_eq!(validate_address(0x3ff, I2C_M_TEN), Ok(AddressMode::TenBit));
    assert_eq!(
        validate_address(0x400, I2C_M_TEN),
        Err(AddressRefusal::TenBitOutOfRange {
            address: 0x400,
            maximum: 0x3ff
        })
    ); // drivers/i2c/i2c-core-base.c:754-757
}

#[test]
fn seven_bit_wire_encoding_includes_direction() {
    assert_eq!(encode_7bit(0x08, 0), Ok(0x10)); // include/linux/i2c.h:948-951
    assert_eq!(encode_7bit(0x08, I2C_M_RD), Ok(0x11));
    assert_eq!(encode_7bit(0x77, 0), Ok(0xee));
    assert_eq!(encode_7bit(0x77, I2C_M_RD), Ok(0xef));
    assert_eq!(
        encode_7bit(0x07, 0),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x07,
            first: 0x04,
            last: 0x07,
            reason: "Hs-mode master code"
        })
    );
    assert_eq!(
        encode_7bit(0x78, I2C_M_RD),
        Err(AddressRefusal::ReservedSevenBit {
            address: 0x78,
            first: 0x78,
            last: 0x7b,
            reason: "10-bit slave addressing"
        })
    );
    assert_eq!(
        encode_7bit(0x80, 0),
        Err(AddressRefusal::SevenBitOutOfRange {
            address: 0x80,
            maximum: 0x7f
        })
    );
}

#[test]
fn ten_bit_wire_encoding_pins_a9_a8_direction_and_low_byte() {
    // include/linux/i2c.h:953-965: 11110 A9 A8 R/W, then A7..A0.
    assert_eq!(encode_10bit(0x000, 0), Ok((0xf0, 0x00)));
    assert_eq!(encode_10bit(0x000, I2C_M_RD), Ok((0xf1, 0x00)));
    assert_eq!(encode_10bit(0x100, 0), Ok((0xf2, 0x00)));
    assert_eq!(encode_10bit(0x200, I2C_M_RD), Ok((0xf5, 0x00)));
    assert_eq!(encode_10bit(0x3ff, 0), Ok((0xf6, 0xff)));
    assert_eq!(encode_10bit(0x3ff, I2C_M_RD), Ok((0xf7, 0xff)));
    assert_eq!(
        encode_10bit(0x400, 0),
        Err(AddressRefusal::TenBitOutOfRange {
            address: 0x400,
            maximum: 0x3ff
        })
    );
}
