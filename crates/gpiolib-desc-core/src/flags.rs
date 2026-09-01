// SPDX-License-Identifier: GPL-2.0-only
//! GPIO lookup, request, and descriptor flags and Linux's configuration precedence.
//!
//! Ported from Linux `include/linux/gpio/machine.h`, `include/linux/gpio/consumer.h`,
//! `drivers/gpio/gpiolib.h`, and `drivers/gpio/gpiolib.c` (`gpiod_configure_flags`).
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use core::fmt;

// Lookup flags: include/linux/gpio/machine.h:8-:19.
pub const GPIO_ACTIVE_HIGH: u32 = 0 << 0; // machine.h:8
pub const GPIO_ACTIVE_LOW: u32 = 1 << 0; // machine.h:9
pub const GPIO_OPEN_DRAIN: u32 = 1 << 1; // machine.h:10
pub const GPIO_OPEN_SOURCE: u32 = 1 << 2; // machine.h:11
pub const GPIO_PERSISTENT: u32 = 0 << 3; // machine.h:12
pub const GPIO_TRANSITORY: u32 = 1 << 3; // machine.h:13
pub const GPIO_PULL_UP: u32 = 1 << 4; // machine.h:14
pub const GPIO_PULL_DOWN: u32 = 1 << 5; // machine.h:15
pub const GPIO_PULL_DISABLE: u32 = 1 << 6; // machine.h:16
pub const GPIO_LOOKUP_FLAGS_DEFAULT: u32 = GPIO_ACTIVE_HIGH | GPIO_PERSISTENT; // machine.h:18

/// Linux lookup flags, in declaration order (machine.h:8-:18).
pub const LOOKUP_FLAGS: [(&str, u32); 10] = [
    ("GPIO_ACTIVE_HIGH", GPIO_ACTIVE_HIGH),
    ("GPIO_ACTIVE_LOW", GPIO_ACTIVE_LOW),
    ("GPIO_OPEN_DRAIN", GPIO_OPEN_DRAIN),
    ("GPIO_OPEN_SOURCE", GPIO_OPEN_SOURCE),
    ("GPIO_PERSISTENT", GPIO_PERSISTENT),
    ("GPIO_TRANSITORY", GPIO_TRANSITORY),
    ("GPIO_PULL_UP", GPIO_PULL_UP),
    ("GPIO_PULL_DOWN", GPIO_PULL_DOWN),
    ("GPIO_PULL_DISABLE", GPIO_PULL_DISABLE),
    ("GPIO_LOOKUP_FLAGS_DEFAULT", GPIO_LOOKUP_FLAGS_DEFAULT),
];

// Request bits: include/linux/gpio/consumer.h:30-:35.
pub const GPIOD_FLAGS_BIT_DIR_SET: u32 = 1 << 0; // consumer.h:30
pub const GPIOD_FLAGS_BIT_DIR_OUT: u32 = 1 << 1; // consumer.h:31
pub const GPIOD_FLAGS_BIT_DIR_VAL: u32 = 1 << 2; // consumer.h:32
pub const GPIOD_FLAGS_BIT_OPEN_DRAIN: u32 = 1 << 3; // consumer.h:33
pub const GPIOD_FLAGS_BIT_NONEXCLUSIVE: u32 = 1 << 4; // consumer.h:35

/// Linux descriptor request bit masks, in declaration order (consumer.h:30-:35).
pub const REQUEST_FLAG_BITS: [(&str, u32); 5] = [
    ("GPIOD_FLAGS_BIT_DIR_SET", GPIOD_FLAGS_BIT_DIR_SET),
    ("GPIOD_FLAGS_BIT_DIR_OUT", GPIOD_FLAGS_BIT_DIR_OUT),
    ("GPIOD_FLAGS_BIT_DIR_VAL", GPIOD_FLAGS_BIT_DIR_VAL),
    ("GPIOD_FLAGS_BIT_OPEN_DRAIN", GPIOD_FLAGS_BIT_OPEN_DRAIN),
    ("GPIOD_FLAGS_BIT_NONEXCLUSIVE", GPIOD_FLAGS_BIT_NONEXCLUSIVE),
];

// Request flags: include/linux/gpio/consumer.h:50-:56.
pub const GPIOD_ASIS: u32 = 0; // consumer.h:50
pub const GPIOD_IN: u32 = GPIOD_FLAGS_BIT_DIR_SET; // consumer.h:51
pub const GPIOD_OUT_LOW: u32 = GPIOD_FLAGS_BIT_DIR_SET | GPIOD_FLAGS_BIT_DIR_OUT; // consumer.h:52
pub const GPIOD_OUT_HIGH: u32 =
    GPIOD_FLAGS_BIT_DIR_SET | GPIOD_FLAGS_BIT_DIR_OUT | GPIOD_FLAGS_BIT_DIR_VAL; // consumer.h:53-:54
pub const GPIOD_OUT_LOW_OPEN_DRAIN: u32 = GPIOD_OUT_LOW | GPIOD_FLAGS_BIT_OPEN_DRAIN; // consumer.h:55
pub const GPIOD_OUT_HIGH_OPEN_DRAIN: u32 = GPIOD_OUT_HIGH | GPIOD_FLAGS_BIT_OPEN_DRAIN; // consumer.h:56

/// Linux descriptor request flags, in declaration order (consumer.h:50-:56).
pub const REQUEST_FLAGS: [(&str, u32); 6] = [
    ("GPIOD_ASIS", GPIOD_ASIS),
    ("GPIOD_IN", GPIOD_IN),
    ("GPIOD_OUT_LOW", GPIOD_OUT_LOW),
    ("GPIOD_OUT_HIGH", GPIOD_OUT_HIGH),
    ("GPIOD_OUT_LOW_OPEN_DRAIN", GPIOD_OUT_LOW_OPEN_DRAIN),
    ("GPIOD_OUT_HIGH_OPEN_DRAIN", GPIOD_OUT_HIGH_OPEN_DRAIN),
];

// Internal flag bit numbers: drivers/gpio/gpiolib.h:190-:211.
pub const GPIOD_FLAG_REQUESTED: u8 = 0; // gpiolib.h:189
pub const GPIOD_FLAG_IS_OUT: u8 = 1; // gpiolib.h:190
pub const GPIOD_FLAG_EXPORT: u8 = 2; // gpiolib.h:191
pub const GPIOD_FLAG_SYSFS: u8 = 3; // gpiolib.h:192
pub const GPIOD_FLAG_ACTIVE_LOW: u8 = 6; // gpiolib.h:193
pub const GPIOD_FLAG_OPEN_DRAIN: u8 = 7; // gpiolib.h:194
pub const GPIOD_FLAG_OPEN_SOURCE: u8 = 8; // gpiolib.h:195
pub const GPIOD_FLAG_USED_AS_IRQ: u8 = 9; // gpiolib.h:196
pub const GPIOD_FLAG_IRQ_IS_ENABLED: u8 = 10; // gpiolib.h:197
pub const GPIOD_FLAG_IS_HOGGED: u8 = 11; // gpiolib.h:198
pub const GPIOD_FLAG_TRANSITORY: u8 = 12; // gpiolib.h:199
pub const GPIOD_FLAG_PULL_UP: u8 = 13; // gpiolib.h:200
pub const GPIOD_FLAG_PULL_DOWN: u8 = 14; // gpiolib.h:201
pub const GPIOD_FLAG_BIAS_DISABLE: u8 = 15; // gpiolib.h:202
pub const GPIOD_FLAG_EDGE_RISING: u8 = 16; // gpiolib.h:203
pub const GPIOD_FLAG_EDGE_FALLING: u8 = 17; // gpiolib.h:204
pub const GPIOD_FLAG_EVENT_CLOCK_REALTIME: u8 = 18; // gpiolib.h:205
pub const GPIOD_FLAG_EVENT_CLOCK_HTE: u8 = 19; // gpiolib.h:206
pub const GPIOD_FLAG_SHARED: u8 = 20; // gpiolib.h:207
pub const GPIOD_FLAG_SHARED_PROXY: u8 = 21; // gpiolib.h:208

/// Linux internal descriptor flag bit numbers, in declaration order (gpiolib.h:189-:208).
pub const DESCRIPTOR_FLAG_BITS: [(&str, u8); 20] = [
    ("GPIOD_FLAG_REQUESTED", GPIOD_FLAG_REQUESTED),
    ("GPIOD_FLAG_IS_OUT", GPIOD_FLAG_IS_OUT),
    ("GPIOD_FLAG_EXPORT", GPIOD_FLAG_EXPORT),
    ("GPIOD_FLAG_SYSFS", GPIOD_FLAG_SYSFS),
    ("GPIOD_FLAG_ACTIVE_LOW", GPIOD_FLAG_ACTIVE_LOW),
    ("GPIOD_FLAG_OPEN_DRAIN", GPIOD_FLAG_OPEN_DRAIN),
    ("GPIOD_FLAG_OPEN_SOURCE", GPIOD_FLAG_OPEN_SOURCE),
    ("GPIOD_FLAG_USED_AS_IRQ", GPIOD_FLAG_USED_AS_IRQ),
    ("GPIOD_FLAG_IRQ_IS_ENABLED", GPIOD_FLAG_IRQ_IS_ENABLED),
    ("GPIOD_FLAG_IS_HOGGED", GPIOD_FLAG_IS_HOGGED),
    ("GPIOD_FLAG_TRANSITORY", GPIOD_FLAG_TRANSITORY),
    ("GPIOD_FLAG_PULL_UP", GPIOD_FLAG_PULL_UP),
    ("GPIOD_FLAG_PULL_DOWN", GPIOD_FLAG_PULL_DOWN),
    ("GPIOD_FLAG_BIAS_DISABLE", GPIOD_FLAG_BIAS_DISABLE),
    ("GPIOD_FLAG_EDGE_RISING", GPIOD_FLAG_EDGE_RISING),
    ("GPIOD_FLAG_EDGE_FALLING", GPIOD_FLAG_EDGE_FALLING),
    (
        "GPIOD_FLAG_EVENT_CLOCK_REALTIME",
        GPIOD_FLAG_EVENT_CLOCK_REALTIME,
    ),
    ("GPIOD_FLAG_EVENT_CLOCK_HTE", GPIOD_FLAG_EVENT_CLOCK_HTE),
    ("GPIOD_FLAG_SHARED", GPIOD_FLAG_SHARED),
    ("GPIOD_FLAG_SHARED_PROXY", GPIOD_FLAG_SHARED_PROXY),
];

/// Stable, host-testable subset of descriptor state affected by `gpiod_configure_flags`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DescriptorFlags {
    pub active_low: bool,
    pub open_drain: bool,
    pub open_source: bool,
    pub transitory: bool,
    pub pull_up: bool,
    pub pull_down: bool,
    pub bias_disable: bool,
}

/// Direction request selected by the `GPIOD_FLAGS_BIT_DIR_*` encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectionRequest {
    AsIs,
    Input,
    Output(bool),
}

/// Pure result of Linux's `gpiod_configure_flags` flag processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Configuration {
    pub flags: DescriptorFlags,
    pub direction: DirectionRequest,
    /// Consumer-side open-drain was enforced because firmware did not declare it.
    pub warned_open_drain: bool,
}

/// Named refusal from descriptor flag configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigureError {
    /// More than one of pull-up, pull-down, and pull-disable was requested.
    ConflictingBias { lookup_flags: u32 },
}

impl fmt::Display for ConfigureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConflictingBias { lookup_flags } => write!(
                f,
                "GPIO bias refused: lookup flags {lookup_flags:#x} enable multiple of pull-up, pull-down, and pull-disable"
            ),
        }
    }
}

/// Apply Linux's lookup/request flag precedence without touching a descriptor or hardware.
///
/// Lookup open-drain wins over consumer open-drain; consumer open-drain is accepted but records the
/// warning emitted by Linux (gpiolib.c:4909-:4920). Bias choices are mutually exclusive and are
/// rejected before any direction operation (gpiolib.c:4926-:4940).
pub fn configure_flags(lflags: u32, dflags: u32) -> Result<Configuration, ConfigureError> {
    let pull_count = (lflags & GPIO_PULL_UP != 0) as u8
        + (lflags & GPIO_PULL_DOWN != 0) as u8
        + (lflags & GPIO_PULL_DISABLE != 0) as u8;
    if pull_count > 1 {
        return Err(ConfigureError::ConflictingBias {
            lookup_flags: lflags,
        });
    }

    let firmware_open_drain = lflags & GPIO_OPEN_DRAIN != 0;
    let consumer_open_drain = dflags & GPIOD_FLAGS_BIT_OPEN_DRAIN != 0;
    let flags = DescriptorFlags {
        active_low: lflags & GPIO_ACTIVE_LOW != 0,
        open_drain: firmware_open_drain || consumer_open_drain,
        open_source: lflags & GPIO_OPEN_SOURCE != 0,
        transitory: lflags & GPIO_TRANSITORY != 0,
        pull_up: lflags & GPIO_PULL_UP != 0,
        pull_down: lflags & GPIO_PULL_DOWN != 0,
        bias_disable: lflags & GPIO_PULL_DISABLE != 0,
    };

    let direction = if dflags & GPIOD_FLAGS_BIT_DIR_SET == 0 {
        DirectionRequest::AsIs
    } else if dflags & GPIOD_FLAGS_BIT_DIR_OUT != 0 {
        DirectionRequest::Output(dflags & GPIOD_FLAGS_BIT_DIR_VAL != 0)
    } else {
        DirectionRequest::Input
    };

    Ok(Configuration {
        flags,
        direction,
        warned_open_drain: consumer_open_drain && !firmware_open_drain,
    })
}
