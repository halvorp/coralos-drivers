// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux baud vectors and directional midpoint probes.
//!
//! Copyright (C) 1991-1994 Linus Torvalds.

use serial_termios_core::baud::{
    baud_tolerance, closest_standard_baud, encode_baud, within_baud_tolerance, BaudEncoding,
    BaudRate, BAUD_RATES, BAUD_RATE_COUNT, BOTHER, CBAUD,
};

/// drivers/tty/tty_baudrate.c:20-:42, non-SPARC branch. This list is deliberately handwritten;
/// it is never generated from `BAUD_RATES`, so deleting a production entry leaves this test red.
#[test]
fn all_linux_baud_names_rates_and_bits_are_frozen() {
    let expected = [
        ("B0", 0, 0x0000),
        ("B50", 50, 0x0001),
        ("B75", 75, 0x0002),
        ("B110", 110, 0x0003),
        ("B134", 134, 0x0004),
        ("B150", 150, 0x0005),
        ("B200", 200, 0x0006),
        ("B300", 300, 0x0007),
        ("B600", 600, 0x0008),
        ("B1200", 1_200, 0x0009),
        ("B1800", 1_800, 0x000a),
        ("B2400", 2_400, 0x000b),
        ("B4800", 4_800, 0x000c),
        ("B9600", 9_600, 0x000d),
        ("B19200", 19_200, 0x000e),
        ("B38400", 38_400, 0x000f),
        ("B57600", 57_600, 0x1001),
        ("B115200", 115_200, 0x1002),
        ("B230400", 230_400, 0x1003),
        ("B460800", 460_800, 0x1004),
        ("B500000", 500_000, 0x1005),
        ("B576000", 576_000, 0x1006),
        ("B921600", 921_600, 0x1007),
        ("B1000000", 1_000_000, 0x1008),
        ("B1152000", 1_152_000, 0x1009),
        ("B1500000", 1_500_000, 0x100a),
        ("B2000000", 2_000_000, 0x100b),
        ("B2500000", 2_500_000, 0x100c),
        ("B3000000", 3_000_000, 0x100d),
        ("B3500000", 3_500_000, 0x100e),
        ("B4000000", 4_000_000, 0x100f),
    ];

    assert_eq!(
        BAUD_RATE_COUNT, 31,
        "tty_baudrate.c:44 ARRAY_SIZE(baud_table)"
    );
    assert_eq!(BAUD_RATES.len(), 31);
    assert_eq!(expected.len(), 31);
    for (entry, &(name, rate, bits)) in BAUD_RATES.iter().zip(expected.iter()) {
        assert_eq!(
            (entry.name, entry.rate, entry.cflag_bits),
            (name, rate, bits)
        );
    }
    // include/uapi/asm-generic/termbits.h:95, :108.
    assert_eq!(CBAUD, 0x0000_100f);
    assert_eq!(BOTHER, 0x0000_1000);
}

fn baud(name: &'static str, rate: u32, cflag_bits: u32) -> BaudRate {
    BaudRate {
        name,
        rate,
        cflag_bits,
    }
}

/// include/linux/util_macros.h:40-:49 uses `x <= midpoint` and advances only for a STRICTLY
/// smaller right distance. At the exact 122 midpoint, Linux therefore chooses B110, not B134.
/// The one-unit probes pin both directions around that midpoint.
#[test]
fn exact_midpoint_goes_left_and_either_side_goes_to_its_neighbour() {
    assert_eq!(closest_standard_baud(121), baud("B110", 110, 0x0003));
    assert_eq!(closest_standard_baud(122), baud("B110", 110, 0x0003));
    assert_eq!(closest_standard_baud(123), baud("B134", 134, 0x0004));

    // Repeat across a much wider pair so an implementation cannot accidentally pass only because
    // the low rates differ by a few units: (57,600 + 115,200) / 2 = 86,400 literally.
    assert_eq!(
        closest_standard_baud(86_399),
        baud("B57600", 57_600, 0x1001)
    );
    assert_eq!(
        closest_standard_baud(86_400),
        baud("B57600", 57_600, 0x1001)
    );
    assert_eq!(
        closest_standard_baud(86_401),
        baud("B115200", 115_200, 0x1002)
    );
}

/// include/linux/util_macros.h:36-:52 leaves values outside the array clamped to an endpoint.
#[test]
fn closest_selection_clamps_at_both_table_ends() {
    assert_eq!(closest_standard_baud(0), baud("B0", 0, 0x0000));
    assert_eq!(
        closest_standard_baud(u32::MAX),
        baud("B4000000", 4_000_000, 0x100f)
    );
}

/// tty_baudrate.c:126 computes `close = baud / 50`; :170-:171 includes both endpoints.
#[test]
fn tolerance_is_two_percent_rounded_down_and_inclusive() {
    assert_eq!(baud_tolerance(9_795, false), 195);
    assert!(
        within_baud_tolerance(9_795, 9_600, false),
        "lower endpoint is inclusive"
    );
    assert!(
        !within_baud_tolerance(9_796, 9_600, false),
        "196 exceeds floor(9796/50)=195"
    );
    assert!(
        within_baud_tolerance(9_600, 9_792, false),
        "upper endpoint is inclusive"
    );
    assert!(!within_baud_tolerance(9_600, 9_793, false));

    // tty_baudrate.c:141-:148: an original BOTHER request requires precision.
    assert_eq!(baud_tolerance(9_600, true), 0);
    assert!(within_baud_tolerance(9_600, 9_600, true));
    assert!(!within_baud_tolerance(9_600, 9_601, true));
}

/// tty_baudrate.c:168-:192 selects a standard token inside the window, otherwise BOTHER.
#[test]
fn baud_encoding_applies_the_tolerance_policy() {
    assert_eq!(
        encode_baud(9_795, false),
        BaudEncoding::Standard(baud("B9600", 9_600, 0x000d))
    );
    assert_eq!(
        encode_baud(9_796, false),
        BaudEncoding::Other {
            cflag_bits: 0x1000,
            rate: 9_796
        }
    );
    assert_eq!(
        encode_baud(9_601, true),
        BaudEncoding::Other {
            cflag_bits: 0x1000,
            rate: 9_601
        }
    );
    assert_eq!(
        encode_baud(115_200, true),
        BaudEncoding::Standard(baud("B115200", 115_200, 0x1002))
    );
}
