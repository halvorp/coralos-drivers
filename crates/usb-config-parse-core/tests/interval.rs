// SPDX-License-Identifier: GPL-2.0-only
//! Literal `bInterval` vectors from Linux `drivers/usb/core/config.c:363-433`.
//!
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core authors and contributors.

use usb_config_parse_core::decode::TransferType;
use usb_config_parse_core::interval::{
    encode_interval, InterruptIntervalEncoding, Interval, Speed, WIRED_SPEEDS,
};

const STANDARD: InterruptIntervalEncoding = InterruptIntervalEncoding::Standard;

/// Linux's five wired speed names used by this parser, independently frozen from
/// `include/uapi/linux/usb/ch9.h:1203-1207`.
#[test]
fn speed_count_and_names_are_pinned() {
    let speeds = [
        Speed::Low,
        Speed::Full,
        Speed::High,
        Speed::Super,
        Speed::SuperPlus,
    ];
    let linux_names = ["LOW", "FULL", "HIGH", "SUPER", "SUPER_PLUS"];
    assert_eq!(WIRED_SPEEDS.len(), 5);
    assert_eq!(
        WIRED_SPEEDS
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>(),
        linux_names
    );
    assert_eq!(
        WIRED_SPEEDS
            .iter()
            .map(|(_, speed)| *speed)
            .collect::<Vec<_>>(),
        speeds
    );
}

#[test]
fn full_and_low_interrupt_interval_is_linear_frames() {
    assert_eq!(
        encode_interval(Speed::Full, TransferType::Interrupt, 10, STANDARD),
        Interval {
            encoded: 10,
            replaced: false
        }
    );
    assert_eq!(
        encode_interval(Speed::Low, TransferType::Interrupt, 0, STANDARD),
        Interval {
            encoded: 10,
            replaced: true
        }
    ); // config.c:405-412,426-432
    assert_eq!(
        encode_interval(Speed::Full, TransferType::Interrupt, 255, STANDARD),
        Interval {
            encoded: 255,
            replaced: false
        }
    );
}

#[test]
fn high_and_super_interrupt_interval_uses_exponent_range() {
    for speed in [Speed::High, Speed::Super, Speed::SuperPlus] {
        assert_eq!(
            encode_interval(speed, TransferType::Interrupt, 1, STANDARD),
            Interval {
                encoded: 1,
                replaced: false
            }
        );
        assert_eq!(
            encode_interval(speed, TransferType::Interrupt, 16, STANDARD),
            Interval {
                encoded: 16,
                replaced: false
            }
        );
        assert_eq!(
            encode_interval(speed, TransferType::Interrupt, 0, STANDARD),
            Interval {
                encoded: 7,
                replaced: true
            }
        ); // config.c:381-384
        assert_eq!(
            encode_interval(speed, TransferType::Interrupt, 17, STANDARD),
            Interval {
                encoded: 8,
                replaced: true
            }
        ); // fls(17 * 8) = 8, config.c:381
    }
}

#[test]
fn isochronous_invalid_defaults_differ_at_high_speed() {
    assert_eq!(
        encode_interval(Speed::High, TransferType::Isochronous, 0, STANDARD),
        Interval {
            encoded: 7,
            replaced: true
        }
    ); // config.c:418-420
    assert_eq!(
        encode_interval(Speed::Full, TransferType::Isochronous, 17, STANDARD),
        Interval {
            encoded: 4,
            replaced: true
        }
    ); // config.c:421-423
    assert_eq!(
        encode_interval(Speed::Full, TransferType::Isochronous, 16, STANDARD),
        Interval {
            encoded: 16,
            replaced: false
        }
    );
}

#[test]
fn nonperiodic_endpoints_keep_the_byte() {
    assert_eq!(
        encode_interval(Speed::High, TransferType::Control, 0, STANDARD),
        Interval {
            encoded: 0,
            replaced: false
        }
    );
    assert_eq!(
        encode_interval(Speed::Low, TransferType::Bulk, 255, STANDARD),
        Interval {
            encoded: 255,
            replaced: false
        }
    );
}

/// Quirk formulas and clamping are from `config.c:390-403`.
#[test]
fn interrupt_linear_quirks_encode_frames_and_microframes() {
    assert_eq!(
        encode_interval(
            Speed::High,
            TransferType::Interrupt,
            8,
            InterruptIntervalEncoding::LinearFrames,
        ),
        Interval {
            encoded: 7,
            replaced: true
        }
    ); // clamp(fls(8) + 3, 1, 16) = 7
    assert_eq!(
        encode_interval(
            Speed::High,
            TransferType::Interrupt,
            8,
            InterruptIntervalEncoding::LinearMicroframes,
        ),
        Interval {
            encoded: 4,
            replaced: true
        }
    ); // clamp(fls(8), 1, 16) = 4
    assert_eq!(
        encode_interval(
            Speed::Super,
            TransferType::Interrupt,
            0,
            InterruptIntervalEncoding::LinearMicroframes,
        ),
        Interval {
            encoded: 1,
            replaced: true
        }
    );
}
