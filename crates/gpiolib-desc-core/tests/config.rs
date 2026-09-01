// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for direction, bias, persistence, and packed pinconf.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`, `include/linux/gpio/driver.h`, and
//! `include/linux/pinctrl/pinconf-generic.h`.
//!
//! Copyright (C) 2011 ST-Ericsson SA, Copyright (C) 2013 Intel Corporation, and the Linux GPIO
//! subsystem authors.

use gpiolib_desc_core::config::*;
use gpiolib_desc_core::flags::DescriptorFlags;

/// driver.h:45-:46. Linux's API is counterintuitive but literal: input=1, output=0.
#[test]
fn direction_count_names_and_values_match_linux() {
    let expected = [
        ("GPIO_LINE_DIRECTION_IN", 1),
        ("GPIO_LINE_DIRECTION_OUT", 0),
    ];
    assert_eq!(DIRECTIONS.len(), 2);
    assert_eq!(DIRECTIONS, expected);
}

/// pinconf-generic.h:133-:153. This pins the complete set of pinconf names used by this layer.
#[test]
fn pin_config_parameter_count_names_and_values_match_linux() {
    let expected = [
        ("PIN_CONFIG_BIAS_DISABLE", 1),
        ("PIN_CONFIG_BIAS_PULL_DOWN", 3),
        ("PIN_CONFIG_BIAS_PULL_UP", 5),
        ("PIN_CONFIG_DRIVE_OPEN_DRAIN", 6),
        ("PIN_CONFIG_DRIVE_OPEN_SOURCE", 7),
        ("PIN_CONFIG_DRIVE_PUSH_PULL", 8),
        ("PIN_CONFIG_INPUT_DEBOUNCE", 11),
        ("PIN_CONFIG_PERSIST_STATE", 21),
    ];
    assert_eq!(PIN_CONFIG_PARAMS.len(), 8);
    assert_eq!(PIN_CONFIG_PARAMS, expected);
}

/// pinconf-generic.h:164 and :170-:188 — parameter in low 8 bits, argument in upper 24 bits.
#[test]
fn packed_pinconf_uses_linux_literal_layout_in_both_directions() {
    let config = PinConfig {
        param: 11,
        argument: 0x123456,
    };
    assert_eq!(pack(config), Ok(0x1234560b));
    assert_eq!(unpack(0x1234560b), config);

    let maximum = PinConfig {
        param: 0xff,
        argument: 0x00ff_ffff,
    };
    assert_eq!(pack(maximum), Ok(0xffff_ffff));
    assert_eq!(unpack(0xffff_ffff), maximum);
}

/// pinconf-generic.h:174-:183 says the argument is 24 bits. Refuse rather than silently truncate.
#[test]
fn pinconf_argument_above_24_bits_is_named() {
    let error = pack(PinConfig {
        param: 5,
        argument: 0x0100_0000,
    })
    .unwrap_err();
    assert_eq!(
        error,
        PackError::ArgumentOutOfRange {
            argument: 0x0100_0000,
            maximum: 0x00ff_ffff
        }
    );
    let message = error.to_string();
    assert!(message.contains("0x1000000"));
    assert!(message.contains("0xffffff"));
}

/// gpiolib.c:2751-:2768. Expected parameter/argument pairs are written as Linux literals.
#[test]
fn bias_encoding_and_precedence_match_linux() {
    let mut flags = DescriptorFlags::default();
    assert_eq!(bias_config(flags), None);

    flags.pull_down = true;
    assert_eq!(
        bias_config(flags),
        Some(PinConfig {
            param: 3,
            argument: 1
        })
    );

    flags.pull_up = true;
    assert_eq!(
        bias_config(flags),
        Some(PinConfig {
            param: 5,
            argument: 1
        }),
        "pull-up precedes pull-down at gpiolib.c:2753-:2756"
    );

    flags.bias_disable = true;
    assert_eq!(
        bias_config(flags),
        Some(PinConfig {
            param: 1,
            argument: 0
        }),
        "bias-disable precedes both pulls at gpiolib.c:2751-:2756"
    );
}

/// gpiolib.c:3228-:3234 passes `!transitory` with PIN_CONFIG_PERSIST_STATE (literal 21).
#[test]
fn persistence_argument_is_the_inverse_of_transitory() {
    assert_eq!(
        persistence_config(false),
        PinConfig {
            param: 21,
            argument: 1
        }
    );
    assert_eq!(
        persistence_config(true),
        PinConfig {
            param: 21,
            argument: 0
        }
    );
}

/// gpiolib.c:426-:432 and driver.h:45-:46 permit only literal 0/1 controller directions.
#[test]
fn direction_decode_accepts_only_linux_literals() {
    assert_eq!(decode_direction(0), Ok(false));
    assert_eq!(decode_direction(1), Ok(true));
    let error = decode_direction(2).unwrap_err();
    assert_eq!(
        error,
        DirectionError::InvalidControllerDirection {
            value: 2,
            output: 0,
            input: 1
        }
    );
    assert!(error.to_string().contains("direction 2 refused"));
}
