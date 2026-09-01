// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for GPIO flag values, names, and precedence.
//!
//! Ported from Linux `include/linux/gpio/machine.h`, `include/linux/gpio/consumer.h`,
//! `drivers/gpio/gpiolib.h`, and `drivers/gpio/gpiolib.c`.
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use gpiolib_desc_core::flags::*;

/// machine.h:8-:18. Literal names and values are independent of the production table.
#[test]
fn lookup_flag_count_names_and_values_match_linux() {
    let expected = [
        ("GPIO_ACTIVE_HIGH", 0),
        ("GPIO_ACTIVE_LOW", 1),
        ("GPIO_OPEN_DRAIN", 2),
        ("GPIO_OPEN_SOURCE", 4),
        ("GPIO_PERSISTENT", 0),
        ("GPIO_TRANSITORY", 8),
        ("GPIO_PULL_UP", 16),
        ("GPIO_PULL_DOWN", 32),
        ("GPIO_PULL_DISABLE", 64),
        ("GPIO_LOOKUP_FLAGS_DEFAULT", 0),
    ];
    assert_eq!(LOOKUP_FLAGS.len(), 10);
    assert_eq!(LOOKUP_FLAGS, expected);
}

/// consumer.h:30-:35 and :50-:56. Literal expected names and values pin both constituent bits and
/// enum names without deriving either list from production.
#[test]
fn request_flag_count_names_and_values_match_linux() {
    let expected_bits = [
        ("GPIOD_FLAGS_BIT_DIR_SET", 1),
        ("GPIOD_FLAGS_BIT_DIR_OUT", 2),
        ("GPIOD_FLAGS_BIT_DIR_VAL", 4),
        ("GPIOD_FLAGS_BIT_OPEN_DRAIN", 8),
        ("GPIOD_FLAGS_BIT_NONEXCLUSIVE", 16),
    ];
    assert_eq!(REQUEST_FLAG_BITS.len(), 5);
    assert_eq!(REQUEST_FLAG_BITS, expected_bits);

    let expected_flags = [
        ("GPIOD_ASIS", 0),
        ("GPIOD_IN", 1),
        ("GPIOD_OUT_LOW", 3),
        ("GPIOD_OUT_HIGH", 7),
        ("GPIOD_OUT_LOW_OPEN_DRAIN", 11),
        ("GPIOD_OUT_HIGH_OPEN_DRAIN", 15),
    ];
    assert_eq!(REQUEST_FLAGS.len(), 6);
    assert_eq!(REQUEST_FLAGS, expected_flags);
}

/// gpiolib.h:189-:208. Gaps at bits 4 and 5 are intentional Linux literals.
#[test]
fn descriptor_flag_count_names_and_bit_numbers_match_linux() {
    let expected = [
        ("GPIOD_FLAG_REQUESTED", 0),
        ("GPIOD_FLAG_IS_OUT", 1),
        ("GPIOD_FLAG_EXPORT", 2),
        ("GPIOD_FLAG_SYSFS", 3),
        ("GPIOD_FLAG_ACTIVE_LOW", 6),
        ("GPIOD_FLAG_OPEN_DRAIN", 7),
        ("GPIOD_FLAG_OPEN_SOURCE", 8),
        ("GPIOD_FLAG_USED_AS_IRQ", 9),
        ("GPIOD_FLAG_IRQ_IS_ENABLED", 10),
        ("GPIOD_FLAG_IS_HOGGED", 11),
        ("GPIOD_FLAG_TRANSITORY", 12),
        ("GPIOD_FLAG_PULL_UP", 13),
        ("GPIOD_FLAG_PULL_DOWN", 14),
        ("GPIOD_FLAG_BIAS_DISABLE", 15),
        ("GPIOD_FLAG_EDGE_RISING", 16),
        ("GPIOD_FLAG_EDGE_FALLING", 17),
        ("GPIOD_FLAG_EVENT_CLOCK_REALTIME", 18),
        ("GPIOD_FLAG_EVENT_CLOCK_HTE", 19),
        ("GPIOD_FLAG_SHARED", 20),
        ("GPIOD_FLAG_SHARED_PROXY", 21),
    ];
    assert_eq!(DESCRIPTOR_FLAG_BITS.len(), 20);
    assert_eq!(DESCRIPTOR_FLAG_BITS, expected);
}

/// gpiolib.c:4906-:4924 and :4946-:4957.
#[test]
fn configure_flags_applies_polarity_drive_and_direction_literals() {
    let cfg = configure_flags(1 | 2 | 4 | 8, 1 | 2 | 4 | 8).unwrap();
    assert_eq!(
        cfg,
        Configuration {
            flags: DescriptorFlags {
                active_low: true,
                open_drain: true,
                open_source: true,
                transitory: true,
                pull_up: false,
                pull_down: false,
                bias_disable: false,
            },
            direction: DirectionRequest::Output(true),
            warned_open_drain: false,
        }
    );

    assert_eq!(
        configure_flags(0, 0).unwrap().direction,
        DirectionRequest::AsIs
    );
    assert_eq!(
        configure_flags(0, 1).unwrap().direction,
        DirectionRequest::Input
    );
    assert_eq!(
        configure_flags(0, 3).unwrap().direction,
        DirectionRequest::Output(false)
    );
    assert_eq!(
        configure_flags(0, 7).unwrap().direction,
        DirectionRequest::Output(true)
    );
}

/// gpiolib.c:4909-:4920. Firmware open-drain suppresses the consumer-enforcement warning; the
/// consumer bit still enforces open drain if firmware omitted it.
#[test]
fn firmware_open_drain_has_precedence_over_consumer_enforcement() {
    let firmware = configure_flags(2, 8).unwrap();
    assert!(firmware.flags.open_drain);
    assert!(!firmware.warned_open_drain);

    let consumer = configure_flags(0, 8).unwrap();
    assert!(consumer.flags.open_drain);
    assert!(consumer.warned_open_drain);
}

/// gpiolib.c:4926-:4940. Every pair and the triple are invalid; each singleton is retained.
#[test]
fn bias_flags_are_mutually_exclusive_and_refusal_is_named() {
    assert_eq!(configure_flags(16, 0).unwrap().flags.pull_up, true);
    assert_eq!(configure_flags(32, 0).unwrap().flags.pull_down, true);
    assert_eq!(configure_flags(64, 0).unwrap().flags.bias_disable, true);

    for literal in [48, 80, 96, 112] {
        let error = configure_flags(literal, 0).unwrap_err();
        assert_eq!(
            error,
            ConfigureError::ConflictingBias {
                lookup_flags: literal
            }
        );
        let message = error.to_string();
        assert!(message.contains("GPIO bias refused"));
        assert!(message.contains("multiple"));
    }
}
