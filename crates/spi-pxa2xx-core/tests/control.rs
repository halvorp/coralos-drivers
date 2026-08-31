// SPDX-License-Identifier: GPL-2.0-only
//! SSCR control-word vectors from `drivers/spi/spi-pxa2xx.c` and
//! `include/linux/pxa2xx_ssp.h`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use spi_pxa2xx_core::control::{encode_sscr0, encode_sscr1_mode, ControlError, SpiMode};

#[test]
fn sscr0_encodes_normal_and_extended_word_sizes() {
    // spi-pxa2xx.c:301-304; pxa2xx_ssp.h:50,57,60.
    assert_eq!(encode_sscr0(2, 8), Ok(0x0000_0207));
    assert_eq!(encode_sscr0(0, 16), Ok(0x0000_000f));
    assert_eq!(encode_sscr0(1, 17), Ok(0x0010_0100));
    assert_eq!(encode_sscr0(0xfff, 32), Ok(0x001f_ff0f));
}

#[test]
fn sscr0_refusals_name_the_value_and_bound() {
    // LPSS probe uses SPI_BPW_RANGE_MASK(4, 32), spi-pxa2xx.c:1326.
    assert_eq!(
        encode_sscr0(0, 3),
        Err(ControlError::BitsPerWordBelowMinimum {
            bits: 3,
            minimum: 4
        })
    );
    assert_eq!(
        encode_sscr0(0, 33),
        Err(ControlError::BitsPerWordAboveMaximum {
            bits: 33,
            maximum: 32
        })
    );
    // spi-pxa2xx.c:902 uses a 0xfff divider mask.
    assert_eq!(
        encode_sscr0(0x1000, 8),
        Err(ControlError::ClockDividerAboveMaximum {
            divider: 0x1000,
            maximum: 0x0fff
        })
    );
}

#[test]
fn all_eight_mode_combinations_map_to_linux_bits() {
    // spi-pxa2xx.c:1228-1233; pxa2xx_ssp.h:72-74.
    let vectors = [
        (
            SpiMode {
                cpha: false,
                cpol: false,
                loopback: false,
            },
            0x00,
        ),
        (
            SpiMode {
                cpha: true,
                cpol: false,
                loopback: false,
            },
            0x10,
        ),
        (
            SpiMode {
                cpha: false,
                cpol: true,
                loopback: false,
            },
            0x08,
        ),
        (
            SpiMode {
                cpha: true,
                cpol: true,
                loopback: false,
            },
            0x18,
        ),
        (
            SpiMode {
                cpha: false,
                cpol: false,
                loopback: true,
            },
            0x04,
        ),
        (
            SpiMode {
                cpha: true,
                cpol: false,
                loopback: true,
            },
            0x14,
        ),
        (
            SpiMode {
                cpha: false,
                cpol: true,
                loopback: true,
            },
            0x0c,
        ),
        (
            SpiMode {
                cpha: true,
                cpol: true,
                loopback: true,
            },
            0x1c,
        ),
    ];
    assert_eq!(vectors.len(), 8);
    for (mode, expected) in vectors {
        assert_eq!(encode_sscr1_mode(mode), expected);
    }
}
