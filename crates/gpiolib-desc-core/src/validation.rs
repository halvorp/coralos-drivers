// SPDX-License-Identifier: GPL-2.0-only
//! Host-testable GPIO descriptor validation rules.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`: `gpio_device_get_desc`, `validate_desc`,
//! `gpiod_is_equal`, and the output/IRQ guard in `gpiod_direction_output_nonotify`.
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use core::fmt;

/// Pointer-independent representation of the three states accepted by Linux's `validate_desc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorRef {
    /// A NULL descriptor means an optional GPIO was absent (gpiolib.c:377-:384).
    OptionalAbsent,
    /// An ERR_PTR carries its negative errno through validation (gpiolib.c:382-:386).
    Error(i32),
    /// A non-NULL, non-error descriptor is valid (gpiolib.c:388).
    Valid { id: usize },
}

/// Result corresponding to Linux's 0 / negative errno / 1 validation convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    OptionalAbsent,
    Error(i32),
    Valid,
}

/// Classify a descriptor exactly as `validate_desc` does (gpiolib.c:377-:388).
pub fn validate_descriptor(desc: DescriptorRef) -> Validation {
    match desc {
        DescriptorRef::OptionalAbsent => Validation::OptionalAbsent,
        DescriptorRef::Error(errno) => Validation::Error(errno),
        DescriptorRef::Valid { .. } => Validation::Valid,
    }
}

/// Linux descriptor equality: the first must validate and the second must be valid, then IDs match
/// (gpiolib.c:403-:415).
pub fn descriptors_equal(desc: DescriptorRef, other: DescriptorRef) -> bool {
    matches!(validate_descriptor(desc), Validation::Valid)
        && matches!(validate_descriptor(other), Validation::Valid)
        && desc == other
}

/// Validate a hardware offset against a GPIO device's line count.
///
/// Linux refuses `hwnum >= ngpio` with `-EINVAL` (gpiolib.c:207-:211).
pub fn validate_hardware_offset(hwnum: u32, ngpio: u32) -> Result<(), DescriptorError> {
    if hwnum >= ngpio {
        Err(DescriptorError::HardwareOffsetOutOfRange { hwnum, ngpio })
    } else {
        Ok(())
    }
}

/// Validate the output-direction IRQ rule from `gpiod_direction_output_nonotify`.
///
/// A line both used as an IRQ and currently IRQ-enabled must not become output
/// (gpiolib.c:3041-:3048).
pub fn validate_output_request(
    used_as_irq: bool,
    irq_enabled: bool,
) -> Result<(), DescriptorError> {
    if used_as_irq && irq_enabled {
        Err(DescriptorError::EnabledIrqCannotBecomeOutput)
    } else {
        Ok(())
    }
}

/// Named descriptor refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorError {
    HardwareOffsetOutOfRange { hwnum: u32, ngpio: u32 },
    EnabledIrqCannotBecomeOutput,
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HardwareOffsetOutOfRange { hwnum, ngpio } => write!(
                f,
                "GPIO hardware offset {hwnum} refused: device line count is {ngpio}, so the offset must be below {ngpio}"
            ),
            Self::EnabledIrqCannotBecomeOutput => f.write_str(
                "GPIO output request refused: the line is tied to an enabled IRQ",
            ),
        }
    }
}
