// SPDX-License-Identifier: GPL-2.0-only
//! Logical/physical GPIO value mapping and open-drain/open-source emulation decisions.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`: `gpiod_direction_output_nonotify`,
//! `gpio_set_open_drain_value_commit`, `gpio_set_open_source_value_commit`,
//! `gpiod_get_value`, and `gpiod_set_value_nocheck`.
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use crate::flags::DescriptorFlags;

/// Convert a consumer's logical value to the raw physical line level.
///
/// Linux inverts exactly when ACTIVE_LOW is set (gpiolib.c:3034-:3039 and :3871-:3874).
pub fn logical_to_physical(logical: bool, active_low: bool) -> bool {
    logical ^ active_low
}

/// Convert a raw physical line level to the consumer's logical value.
///
/// This is the inverse path in `gpiod_get_value` (gpiolib.c:3525-:3531). It intentionally uses the
/// same XOR relation so set/get round trips cannot disagree in only one direction.
pub fn physical_to_logical(physical: bool, active_low: bool) -> bool {
    physical ^ active_low
}

/// Physical action needed to produce a value under GPIO drive semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveAction {
    /// Actively drive this physical level.
    Drive(bool),
    /// Release the pin by switching it to input/high-impedance mode.
    ReleaseToInput,
}

/// Map a logical value through ACTIVE_LOW and then apply drive-mode precedence.
///
/// Open-drain wins over open-source because Linux checks it first with `if ... else if`
/// (gpiolib.c:3873-:3881). Open-drain releases high and drives low (gpiolib.c:3599-:3614);
/// open-source drives high and releases low (gpiolib.c:3630-:3643).
pub fn output_action(logical: bool, flags: DescriptorFlags) -> DriveAction {
    let physical = logical_to_physical(logical, flags.active_low);
    if flags.open_drain {
        if physical {
            DriveAction::ReleaseToInput
        } else {
            DriveAction::Drive(false)
        }
    } else if flags.open_source {
        if physical {
            DriveAction::Drive(true)
        } else {
            DriveAction::ReleaseToInput
        }
    } else {
        DriveAction::Drive(physical)
    }
}

/// Recover the physical line level represented by an action under the selected drive mode.
///
/// A released open-drain line represents high; a released open-source line represents low. Invalid
/// action/mode pairs are named rather than silently guessed.
pub fn action_to_physical(
    action: DriveAction,
    flags: DescriptorFlags,
) -> Result<bool, ActionDecodeError> {
    if flags.open_drain {
        match action {
            DriveAction::Drive(false) => Ok(false),
            DriveAction::ReleaseToInput => Ok(true),
            DriveAction::Drive(true) => Err(ActionDecodeError::OpenDrainCannotDriveHigh),
        }
    } else if flags.open_source {
        match action {
            DriveAction::Drive(true) => Ok(true),
            DriveAction::ReleaseToInput => Ok(false),
            DriveAction::Drive(false) => Err(ActionDecodeError::OpenSourceCannotDriveLow),
        }
    } else {
        match action {
            DriveAction::Drive(value) => Ok(value),
            DriveAction::ReleaseToInput => Err(ActionDecodeError::PushPullCannotRelease),
        }
    }
}

/// Recover a logical value from a drive action, applying the read-side ACTIVE_LOW mapping.
pub fn action_to_logical(
    action: DriveAction,
    flags: DescriptorFlags,
) -> Result<bool, ActionDecodeError> {
    action_to_physical(action, flags)
        .map(|physical| physical_to_logical(physical, flags.active_low))
}

/// Named refusal for an action impossible under its selected drive mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionDecodeError {
    OpenDrainCannotDriveHigh,
    OpenSourceCannotDriveLow,
    PushPullCannotRelease,
}

impl core::fmt::Display for ActionDecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::OpenDrainCannotDriveHigh => {
                f.write_str("open-drain action refused: open drain cannot actively drive high")
            }
            Self::OpenSourceCannotDriveLow => {
                f.write_str("open-source action refused: open source cannot actively drive low")
            }
            Self::PushPullCannotRelease => {
                f.write_str("push-pull action refused: push-pull cannot represent release-to-input")
            }
        }
    }
}
