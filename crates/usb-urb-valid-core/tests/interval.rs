// SPDX-License-Identifier: GPL-2.0-only
//! Frozen interval vectors from Linux `include/linux/usb.h:1760-1787`,
//! `drivers/usb/core/devio.c:1900-1907`, and `drivers/usb/core/urb.c:543-584`.
//!
//! Copyright (C) the Linux USB core and Linux USB API authors.

use usb_urb_valid_core::{
    interval::{
        decode_descriptor_interval, decode_iso_descriptor_interval, encode_descriptor_exponent,
        normalize_interval, IntervalError,
    },
    Speed, TransferType,
};

#[test]
fn logarithmic_encode_and_decode_are_pinned_at_every_wired_speed() {
    // `1 << (interval - 1)`, include/linux/usb.h:1781-1785 and devio.c:1900-1907.
    // ISO is logarithmic at every speed; high-or-faster interrupt is logarithmic.
    for speed in [
        Speed::Low,
        Speed::Full,
        Speed::High,
        Speed::Super,
        Speed::SuperPlus,
    ] {
        assert_eq!(decode_iso_descriptor_interval(4), 8, "{speed:?}");
        assert_eq!(encode_descriptor_exponent(8), 4, "{speed:?}");
    }
    let vectors = [
        (Speed::High, TransferType::Interrupt, 4, 8),
        (Speed::Super, TransferType::Interrupt, 4, 8),
        (Speed::SuperPlus, TransferType::Interrupt, 4, 8),
    ];
    for (speed, kind, encoded, linear) in vectors {
        assert_eq!(decode_descriptor_interval(speed, kind, encoded), linear);
        assert_eq!(encode_descriptor_exponent(linear), encoded);
    }
    // Full/low interrupt is linear frames, include/linux/usb.h:1786-1787.
    assert_eq!(
        decode_descriptor_interval(Speed::Low, TransferType::Interrupt, 4),
        4
    );
    assert_eq!(
        decode_descriptor_interval(Speed::Full, TransferType::Interrupt, 4),
        4
    );
}

#[test]
fn exponent_off_by_one_vectors_cover_every_bit_position() {
    // Literal descriptor exponents 1..=16 decode to 2^(n-1), usb.h:1781-1785.
    let expected_linear = [
        1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096, 8192, 16384, 32768,
    ];
    for (index, linear) in expected_linear.into_iter().enumerate() {
        let exponent = (index + 1) as u8;
        assert_eq!(
            decode_descriptor_interval(Speed::High, TransferType::Interrupt, exponent),
            linear
        );
        assert_eq!(encode_descriptor_exponent(linear), exponent);
    }
}

#[test]
fn descriptor_exponent_clamps_are_pinned_from_both_sides() {
    // usb.h:1782-1785 clamps interrupt exponent to 1..=16; devio.c:1904-1905 caps ISO at 15 shift.
    assert_eq!(
        decode_descriptor_interval(Speed::High, TransferType::Interrupt, 0),
        1
    );
    assert_eq!(
        decode_descriptor_interval(Speed::High, TransferType::Interrupt, 1),
        1
    );
    assert_eq!(
        decode_descriptor_interval(Speed::High, TransferType::Interrupt, 2),
        2
    );
    assert_eq!(
        decode_descriptor_interval(Speed::Super, TransferType::Interrupt, 14),
        8192
    );
    assert_eq!(
        decode_descriptor_interval(Speed::Super, TransferType::Interrupt, 15),
        16384
    );
    assert_eq!(
        decode_descriptor_interval(Speed::Super, TransferType::Interrupt, 16),
        32768
    );
    assert_eq!(
        decode_descriptor_interval(Speed::Super, TransferType::Interrupt, 17),
        32768
    );
    assert_eq!(decode_iso_descriptor_interval(0), 0); // devio.c:1900 skips a zero bInterval
    assert_eq!(decode_iso_descriptor_interval(1), 1);
    assert_eq!(decode_iso_descriptor_interval(2), 2);
    assert_eq!(decode_iso_descriptor_interval(15), 16384);
    assert_eq!(decode_iso_descriptor_interval(16), 32768);
    assert_eq!(decode_iso_descriptor_interval(17), 32768);
    assert_eq!(encode_descriptor_exponent(0), 1);
    assert_eq!(encode_descriptor_exponent(1), 1);
    assert_eq!(encode_descriptor_exponent(2), 2);
    assert_eq!(encode_descriptor_exponent(32768), 16);
    assert_eq!(encode_descriptor_exponent(65536), 16);
}

#[test]
fn encode_rounds_non_power_of_two_linear_values_down_like_submission() {
    assert_eq!(encode_descriptor_exponent(3), 2);
    assert_eq!(encode_descriptor_exponent(7), 3);
    assert_eq!(encode_descriptor_exponent(9), 4);
    assert_eq!(encode_descriptor_exponent(32767), 15);
}

#[test]
fn high_speed_periodic_clamp_is_pinned_from_both_sides() {
    // urb.c:559-564 clamps above 1024*8, then rounds down.
    for kind in [TransferType::Interrupt, TransferType::Isochronous] {
        assert_eq!(normalize_interval(Speed::High, kind, 8191), Ok(4096));
        assert_eq!(normalize_interval(Speed::High, kind, 8192), Ok(8192));
        assert_eq!(normalize_interval(Speed::High, kind, 8193), Ok(8192));
    }
}

#[test]
fn super_periodic_maximum_refuses_from_just_above_the_boundary() {
    // urb.c:552-558 rejects rather than clamps above 2^15.
    for speed in [Speed::Super, Speed::SuperPlus] {
        for kind in [TransferType::Interrupt, TransferType::Isochronous] {
            assert_eq!(normalize_interval(speed, kind, 32767), Ok(16384));
            assert_eq!(normalize_interval(speed, kind, 32768), Ok(32768));
            assert_eq!(
                normalize_interval(speed, kind, 32769),
                Err(IntervalError::AboveMaximum {
                    interval: 32769,
                    maximum: 32768,
                    speed,
                    transfer_type: kind,
                })
            );
        }
    }
}

#[test]
fn low_and_full_interrupt_bound_and_scheduler_cap_are_distinct() {
    // urb.c:565-572: request bound 255, scheduler max 128, then round down.
    for speed in [Speed::Low, Speed::Full] {
        assert_eq!(
            normalize_interval(speed, TransferType::Interrupt, 127),
            Ok(64)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Interrupt, 128),
            Ok(128)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Interrupt, 129),
            Ok(128)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Interrupt, 255),
            Ok(128)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Interrupt, 256),
            Err(IntervalError::AboveMaximum {
                interval: 256,
                maximum: 255,
                speed,
                transfer_type: TransferType::Interrupt,
            })
        );
    }
}

#[test]
fn low_and_full_isochronous_clamp_is_pinned_from_both_sides() {
    // urb.c:572-577 clamps above 1024 rather than rejecting.
    for speed in [Speed::Low, Speed::Full] {
        assert_eq!(
            normalize_interval(speed, TransferType::Isochronous, 1023),
            Ok(512)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Isochronous, 1024),
            Ok(1024)
        );
        assert_eq!(
            normalize_interval(speed, TransferType::Isochronous, 1025),
            Ok(1024)
        );
    }
}

#[test]
fn periodic_zero_and_negative_intervals_name_the_value_and_minimum() {
    for kind in [TransferType::Interrupt, TransferType::Isochronous] {
        assert_eq!(
            normalize_interval(Speed::High, kind, 0),
            Err(IntervalError::NotPositive {
                interval: 0,
                minimum: 1
            })
        ); // urb.c:546-548
        assert_eq!(
            normalize_interval(Speed::High, kind, -1),
            Err(IntervalError::NotPositive {
                interval: -1,
                minimum: 1
            })
        );
    }
}

#[test]
fn nonperiodic_types_leave_interval_untouched() {
    assert_eq!(
        normalize_interval(Speed::Low, TransferType::Control, 0),
        Ok(0)
    );
    assert_eq!(
        normalize_interval(Speed::SuperPlus, TransferType::Bulk, 123),
        Ok(123)
    );
}
