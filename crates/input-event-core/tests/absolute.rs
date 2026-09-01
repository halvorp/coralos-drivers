// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for absolute-axis setup, clamping, flat handling, and Linux defuzzing.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use input_event_core::absolute::*;

#[test]
fn configure_axis_carries_all_linux_fields() {
    let info = configure_axis(0x00, -100, 100, 8, 12).unwrap();
    assert_eq!(info, AbsInfo { value: 0, minimum: -100, maximum: 100, fuzz: 8, flat: 12 });
    assert!(configure_axis(0x3f, 7, 7, 0, 0).is_ok());
    // ABS_X is 0x00 (input-event-codes.h:867), ABS_MAX is 0x3f (:924), and the metadata fields
    // are assigned at input.c:468-471. Both axis and value-range bounds are inclusive.
}

#[test]
fn malformed_axis_configuration_is_named() {
    assert_eq!(configure_axis(0x40, 0, 1, 0, 0), Err(AbsConfigError::AxisOutOfRange { axis: 0x40, max: 0x3f }));
    assert_eq!(configure_axis(0, 2, 1, 0, 0), Err(AbsConfigError::MinimumExceedsMaximum { minimum: 2, maximum: 1 }));
    assert_eq!(configure_axis(0, 0, 10, -1, 0), Err(AbsConfigError::NegativeFuzz { fuzz: -1 }));
    assert_eq!(configure_axis(0, 0, 10, 0, -1), Err(AbsConfigError::NegativeFlat { flat: -1 }));
    assert_eq!(configure_axis(0, -2, 2, 0, 5), Err(AbsConfigError::FlatExceedsRange { flat: 5, minimum: -2, maximum: 2 }));
}

#[test]
fn clamp_uses_declared_inclusive_limits() {
    let info = AbsInfo { value: 0, minimum: -10, maximum: 20, fuzz: 0, flat: 0 };
    assert_eq!(clamp_value(-11, &info), -10);
    assert_eq!(clamp_value(7, &info), 7);
    assert_eq!(clamp_value(21, &info), 20);
}

#[test]
fn flat_zeroes_the_inclusive_centre_dead_zone() {
    assert_eq!(apply_flat(-5, 5), 0);
    assert_eq!(apply_flat(0, 5), 0);
    assert_eq!(apply_flat(5, 5), 0);
    assert_eq!(apply_flat(6, 5), 6);
}

#[test]
fn defuzz_matches_all_three_linux_bands_and_strict_edges() {
    // input.c:74-82, old=100 and fuzz=20.
    assert_eq!(defuzz(105, 100, 20), 100); // inner band: old
    assert_eq!(defuzz(115, 100, 20), 103); // (3*100+115)/4
    assert_eq!(defuzz(130, 100, 20), 115); // (100+130)/2
    assert_eq!(defuzz(140, 100, 20), 140); // strict boundary is not filtered
    assert_eq!(defuzz(99, 100, 0), 99); // input.c:73: fuzz zero bypasses all bands
}

#[test]
fn filter_suppresses_unchanged_and_returns_changed_value() {
    assert_eq!(filter_value(105, 100, 20), AbsFilter::Unchanged); // input.c:194-195
    assert_eq!(filter_value(130, 100, 20), AbsFilter::Changed(115)); // input.c:197
}

#[test]
fn mt_value_range_excludes_slot_and_holes() {
    assert!(!is_mt_value(0x2f)); // ABS_MT_SLOT, input-event-codes.h:907
    assert!(is_mt_value(0x30)); // ABS_MT_TOUCH_MAJOR, :908
    assert!(is_mt_value(0x3d)); // ABS_MT_TOOL_Y, :921
    assert!(!is_mt_value(0x3e));
}
