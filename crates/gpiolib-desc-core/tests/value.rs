// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for ACTIVE_LOW, open-drain, and open-source value semantics.
//!
//! Ported from Linux `drivers/gpio/gpiolib.c`.
//!
//! Copyright (C) 2013 Intel Corporation and the Linux GPIO subsystem authors.

use gpiolib_desc_core::flags::DescriptorFlags;
use gpiolib_desc_core::value::*;

/// gpiolib.c:3034-:3039, :3525-:3531, and :3871-:3874. BOTH DIRECTIONS are independently pinned
/// for active-high and active-low. This is the hardware-critical inversion contract.
#[test]
fn logical_and_physical_mapping_is_bidirectional_for_both_polarities() {
    let vectors = [
        // logical, active_low, physical -- Linux literals from `value = !value` / `!!value`.
        (false, false, false),
        (true, false, true),
        (false, true, true),
        (true, true, false),
    ];

    for (logical, active_low, physical) in vectors {
        assert_eq!(logical_to_physical(logical, active_low), physical);
        assert_eq!(physical_to_logical(physical, active_low), logical);
        assert_eq!(
            physical_to_logical(logical_to_physical(logical, active_low), active_low),
            logical
        );
        assert_eq!(
            logical_to_physical(physical_to_logical(physical, active_low), active_low),
            physical
        );
    }
}

fn flags(active_low: bool, open_drain: bool, open_source: bool) -> DescriptorFlags {
    DescriptorFlags {
        active_low,
        open_drain,
        open_source,
        ..DescriptorFlags::default()
    }
}

/// gpiolib.c:3599-:3614, :3630-:3643, and :3873-:3881. Every polarity × drive-mode × logical
/// value combination is literal, and each action is decoded through the read direction to assert
/// its round trip. Open-drain precedence is separately represented by the final two vectors.
#[test]
fn every_flag_combination_maps_both_directions_and_round_trips() {
    let vectors = [
        // active_low, open_drain, open_source, logical, expected action
        (false, false, false, false, DriveAction::Drive(false)),
        (false, false, false, true, DriveAction::Drive(true)),
        (true, false, false, false, DriveAction::Drive(true)),
        (true, false, false, true, DriveAction::Drive(false)),
        (false, true, false, false, DriveAction::Drive(false)),
        (false, true, false, true, DriveAction::ReleaseToInput),
        (true, true, false, false, DriveAction::ReleaseToInput),
        (true, true, false, true, DriveAction::Drive(false)),
        (false, false, true, false, DriveAction::ReleaseToInput),
        (false, false, true, true, DriveAction::Drive(true)),
        (true, false, true, false, DriveAction::Drive(true)),
        (true, false, true, true, DriveAction::ReleaseToInput),
        // Both flags: gpiolib.c:3876-:3879 checks open-drain first.
        (false, true, true, false, DriveAction::Drive(false)),
        (false, true, true, true, DriveAction::ReleaseToInput),
        (true, true, true, false, DriveAction::ReleaseToInput),
        (true, true, true, true, DriveAction::Drive(false)),
    ];

    for (active_low, open_drain, open_source, logical, expected) in vectors {
        let semantics = flags(active_low, open_drain, open_source);
        let action = output_action(logical, semantics);
        assert_eq!(action, expected);
        assert_eq!(action_to_logical(action, semantics), Ok(logical));
        assert_eq!(
            action_to_physical(action, semantics),
            Ok(logical_to_physical(logical, active_low))
        );
    }
}

/// Impossible action/mode pairs are refused by name; they must not be silently clamped.
#[test]
fn impossible_drive_actions_are_named_refusals() {
    let drain =
        action_to_physical(DriveAction::Drive(true), flags(false, true, false)).unwrap_err();
    assert_eq!(drain, ActionDecodeError::OpenDrainCannotDriveHigh);
    assert!(drain.to_string().contains("cannot actively drive high"));

    let source =
        action_to_physical(DriveAction::Drive(false), flags(false, false, true)).unwrap_err();
    assert_eq!(source, ActionDecodeError::OpenSourceCannotDriveLow);
    assert!(source.to_string().contains("cannot actively drive low"));

    let push_pull =
        action_to_physical(DriveAction::ReleaseToInput, flags(false, false, false)).unwrap_err();
    assert_eq!(push_pull, ActionDecodeError::PushPullCannotRelease);
    assert!(push_pull.to_string().contains("cannot represent release"));
}
