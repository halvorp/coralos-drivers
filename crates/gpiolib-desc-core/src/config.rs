// SPDX-License-Identifier: GPL-2.0-only
//! Direction and pin-configuration encoding used by the GPIO descriptor layer.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`, `include/linux/gpio/driver.h`, and
//! `include/linux/pinctrl/pinconf-generic.h`.
//!
//! Copyright (C) 2011 ST-Ericsson SA, Copyright (C) 2013 Intel Corporation, and the Linux GPIO
//! subsystem authors.

use core::fmt;

use crate::flags::DescriptorFlags;

pub const GPIO_LINE_DIRECTION_OUT: u32 = 0; // include/linux/gpio/driver.h:46
pub const GPIO_LINE_DIRECTION_IN: u32 = 1; // include/linux/gpio/driver.h:45

/// Linux direction encodings (driver.h:45-:46).
pub const DIRECTIONS: [(&str, u32); 2] = [
    ("GPIO_LINE_DIRECTION_IN", GPIO_LINE_DIRECTION_IN),
    ("GPIO_LINE_DIRECTION_OUT", GPIO_LINE_DIRECTION_OUT),
];

// Values from enum pin_config_param: include/linux/pinctrl/pinconf-generic.h:131-:159.
pub const PIN_CONFIG_BIAS_DISABLE: u8 = 1; // pinconf-generic.h:133
pub const PIN_CONFIG_BIAS_PULL_DOWN: u8 = 3; // pinconf-generic.h:135
pub const PIN_CONFIG_BIAS_PULL_UP: u8 = 5; // pinconf-generic.h:137
pub const PIN_CONFIG_DRIVE_OPEN_DRAIN: u8 = 6; // pinconf-generic.h:138
pub const PIN_CONFIG_DRIVE_OPEN_SOURCE: u8 = 7; // pinconf-generic.h:139
pub const PIN_CONFIG_DRIVE_PUSH_PULL: u8 = 8; // pinconf-generic.h:140
pub const PIN_CONFIG_INPUT_DEBOUNCE: u8 = 11; // pinconf-generic.h:143
pub const PIN_CONFIG_PERSIST_STATE: u8 = 21; // pinconf-generic.h:153

/// Pin configuration parameters used by `gpiolib.c` in this descriptor layer.
pub const PIN_CONFIG_PARAMS: [(&str, u8); 8] = [
    ("PIN_CONFIG_BIAS_DISABLE", PIN_CONFIG_BIAS_DISABLE),
    ("PIN_CONFIG_BIAS_PULL_DOWN", PIN_CONFIG_BIAS_PULL_DOWN),
    ("PIN_CONFIG_BIAS_PULL_UP", PIN_CONFIG_BIAS_PULL_UP),
    ("PIN_CONFIG_DRIVE_OPEN_DRAIN", PIN_CONFIG_DRIVE_OPEN_DRAIN),
    ("PIN_CONFIG_DRIVE_OPEN_SOURCE", PIN_CONFIG_DRIVE_OPEN_SOURCE),
    ("PIN_CONFIG_DRIVE_PUSH_PULL", PIN_CONFIG_DRIVE_PUSH_PULL),
    ("PIN_CONFIG_INPUT_DEBOUNCE", PIN_CONFIG_INPUT_DEBOUNCE),
    ("PIN_CONFIG_PERSIST_STATE", PIN_CONFIG_PERSIST_STATE),
];

/// One generic pinconf parameter and its 24-bit argument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinConfig {
    pub param: u8,
    pub argument: u32,
}

/// Named refusal from Linux's 8-bit parameter / 24-bit argument packed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackError {
    ArgumentOutOfRange { argument: u32, maximum: u32 },
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArgumentOutOfRange { argument, maximum } => write!(
                f,
                "pin configuration argument {argument:#x} exceeds 24-bit maximum {maximum:#x}"
            ),
        }
    }
}

/// Pack generic pinconf exactly as `PIN_CONF_PACKED(p, a)` does (pinconf-generic.h:164).
pub fn pack(config: PinConfig) -> Result<u32, PackError> {
    const MAX_ARGUMENT: u32 = 0x00ff_ffff; // pinconf-generic.h:174, argument occupies upper 24 bits
    if config.argument > MAX_ARGUMENT {
        return Err(PackError::ArgumentOutOfRange {
            argument: config.argument,
            maximum: MAX_ARGUMENT,
        });
    }
    Ok((config.argument << 8) | config.param as u32)
}

/// Decode Linux's packed generic pinconf (pinconf-generic.h:176-:184).
pub fn unpack(packed: u32) -> PinConfig {
    PinConfig {
        param: (packed & 0xff) as u8,
        argument: (packed >> 8) & 0x00ff_ffff,
    }
}

/// Bias configuration selected by descriptor flags, including Linux's precedence.
///
/// Bias-disable wins, then pull-up, then pull-down (gpiolib.c:2751-:2758). Pull requests carry
/// argument 1 while disable carries 0 (gpiolib.c:2760-:2768).
pub fn bias_config(flags: DescriptorFlags) -> Option<PinConfig> {
    if flags.bias_disable {
        Some(PinConfig {
            param: PIN_CONFIG_BIAS_DISABLE,
            argument: 0,
        })
    } else if flags.pull_up {
        Some(PinConfig {
            param: PIN_CONFIG_BIAS_PULL_UP,
            argument: 1,
        })
    } else if flags.pull_down {
        Some(PinConfig {
            param: PIN_CONFIG_BIAS_PULL_DOWN,
            argument: 1,
        })
    } else {
        None
    }
}

/// Persistence pinconf argument used by `gpiod_set_transitory`.
///
/// Linux passes `!transitory`: persistent is argument 1, transitory is 0 (gpiolib.c:3225-:3234).
pub fn persistence_config(transitory: bool) -> PinConfig {
    PinConfig {
        param: PIN_CONFIG_PERSIST_STATE,
        argument: (!transitory) as u32,
    }
}

/// Normalize a controller direction result as `gpiod_get_direction` does.
///
/// Linux accepts only literal 0/1 from the chip wrapper (gpiolib.c:426-:432), then reports every
/// positive result as input and zero as output (gpiolib.c:470-:481).
pub fn decode_direction(value: u32) -> Result<bool, DirectionError> {
    match value {
        GPIO_LINE_DIRECTION_OUT => Ok(false),
        GPIO_LINE_DIRECTION_IN => Ok(true),
        _ => Err(DirectionError::InvalidControllerDirection {
            value,
            output: GPIO_LINE_DIRECTION_OUT,
            input: GPIO_LINE_DIRECTION_IN,
        }),
    }
}

/// Named refusal for a direction value outside Linux's 0/1 controller contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionError {
    InvalidControllerDirection { value: u32, output: u32, input: u32 },
}

impl fmt::Display for DirectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidControllerDirection { value, output, input } => write!(
                f,
                "GPIO direction {value} refused: controller must return output {output} or input {input}"
            ),
        }
    }
}
