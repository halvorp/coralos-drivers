// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for thermal trip types, validation, ordering, and directional crossings.
//!
//! Ported from Linux `drivers/thermal/thermal_core.c`, `drivers/thermal/thermal_trip.c`,
//! `drivers/thermal/thermal_sysfs.c`, `include/linux/thermal.h`, and
//! `include/uapi/linux/thermal.h`.
//!
//! Copyright (C) 2008 Intel Corp
//! Copyright (C) 2008 Zhang Rui <rui.zhang@intel.com>
//! Copyright (C) 2008 Sujith Thomas <sujith.thomas@intel.com>
//! Copyright 2022 Linaro Limited

use thermal_trip_core::trip::{
    crossed_down, crossed_up, first_crossed_down, first_crossed_up, order_by_threshold, update,
    validate, validate_hysteresis, Crossing, OrderError, Trip, TripState, TripType, UpwardAction,
    ValidationError, THERMAL_TEMP_INVALID, TRIP_TYPES, TRIP_TYPE_COUNT,
};

/// Frozen from include/uapi/linux/thermal.h:14-19 and drivers/thermal/thermal_trip.c:12-17.
/// Deliberately literal: deriving this list from `TRIP_TYPES` would let a deleted entry delete its
/// own test vector.
const LINUX_TRIP_TYPES: [(i32, &str); 4] =
    [(0, "active"), (1, "passive"), (2, "hot"), (3, "critical")];

#[test]
fn linux_trip_type_count_values_and_names_are_pinned() {
    assert_eq!(TRIP_TYPE_COUNT, 4); // include/uapi/linux/thermal.h:14-19
    assert_eq!(TRIP_TYPES.len(), 4); // include/uapi/linux/thermal.h:14-19
    let ours: Vec<(i32, &str)> = TRIP_TYPES
        .iter()
        .copied()
        .map(|trip_type| (trip_type as i32, trip_type.name()))
        .collect();
    assert_eq!(ours, LINUX_TRIP_TYPES);
}

#[test]
fn trip_type_semantics_match_the_core() {
    // thermal_core.c:456-465 excludes only hot and critical from governor callbacks.
    assert!(TripType::Active.governor_managed());
    assert!(TripType::Passive.governor_managed());
    assert!(!TripType::Hot.governor_managed());
    assert!(!TripType::Critical.governor_managed());

    // thermal_core.c:475-480.
    assert_eq!(TripType::Active.upward_action(), UpwardAction::None);
    assert_eq!(
        TripType::Passive.upward_action(),
        UpwardAction::EnterPassive
    );
    assert_eq!(TripType::Hot.upward_action(), UpwardAction::NotifyHot);
    assert_eq!(
        TripType::Critical.upward_action(),
        UpwardAction::ProtectCritical
    );
}

#[test]
fn construction_and_thresholds_use_linux_literals() {
    let trip = Trip::new(80_000, 5_000, TripType::Passive).unwrap();
    assert_eq!(trip.temperature(), 80_000);
    assert_eq!(trip.hysteresis(), 5_000);
    assert_eq!(trip.trip_type(), TripType::Passive);
    assert_eq!(trip.upward_threshold(), 80_000); // thermal_core.c:438
    assert_eq!(trip.downward_threshold(), 75_000); // thermal_core.c:445
    assert!(!trip.is_invalid());

    let invalid = Trip::new(-274_000, 10_000, TripType::Active).unwrap(); // thermal.h:30
    assert!(invalid.is_invalid());
    assert_eq!(invalid.downward_threshold(), i32::MAX); // thermal_core.c:449-453
}

#[test]
fn validation_names_each_refused_value_and_bound() {
    assert_eq!(
        validate(80_000, -1),
        Err(ValidationError::NegativeHysteresis {
            hysteresis: -1,
            minimum: 0
        })
    ); // thermal_sysfs.c:156-157
    assert_eq!(
        validate(-269_000, 5_000),
        Err(ValidationError::TemperatureAtOrBelowMinimum {
            temperature: -269_000,
            hysteresis: 5_000,
            exclusive_minimum: -269_000,
        })
    ); // thermal_sysfs.c:119-122: -274000 + 5000
    let message = validate(-269_000, 5_000).unwrap_err().to_string();
    assert!(message.contains("temperature -269000"));
    assert!(message.contains("hysteresis 5000"));
    assert!(message.contains("greater than -269000"));

    // One millidegree above the exclusive bound is accepted.
    assert_eq!(validate(-268_999, 5_000), Ok(()));
    // thermal_sysfs.c:164-172 explicitly permits hysteresis while the trip is invalid.
    assert_eq!(validate(THERMAL_TEMP_INVALID, i32::MAX), Ok(()));
}

#[test]
fn validate_hysteresis_checks_the_directional_low_threshold() {
    assert_eq!(validate_hysteresis(80_000, 5_000), Ok(()));
    assert_eq!(validate_hysteresis(THERMAL_TEMP_INVALID, 500_000), Ok(())); // sysfs.c:164-172
    assert_eq!(
        validate_hysteresis(-270_000, 4_000),
        Err(ValidationError::DownwardThresholdAtOrBelowInvalid {
            temperature: -270_000,
            hysteresis: 4_000,
            downward_threshold: -274_000,
            exclusive_minimum: -274_000,
        })
    ); // thermal_sysfs.c:175-176
    assert_eq!(
        validate_hysteresis(80_000, -2),
        Err(ValidationError::NegativeHysteresis {
            hysteresis: -2,
            minimum: 0
        })
    );
}

#[test]
fn upward_crossing_is_inclusive_at_trip_temperature() {
    let trip = Trip::new(80_000, 5_000, TripType::Active).unwrap();
    assert!(!crossed_up(trip, 79_999));
    assert!(crossed_up(trip, 80_000)); // thermal_core.c:581-586: threshold <= temperature
    assert!(crossed_up(trip, 80_001));
}

#[test]
fn downward_crossing_is_strict_and_uses_hysteresis() {
    let trip = Trip::new(80_000, 5_000, TripType::Active).unwrap();
    assert!(!crossed_down(trip, 80_000));
    assert!(!crossed_down(trip, 75_001));
    assert!(!crossed_down(trip, 75_000)); // thermal_core.c:569-573 breaks on equality
    assert!(crossed_down(trip, 74_999)); // low threshold is 80000 - 5000 (core.c:445)
}

#[test]
fn hysteresis_is_directional_and_does_not_oscillate() {
    let trip = Trip::new(80_000, 5_000, TripType::Passive).unwrap();

    let (state, crossing) = update(trip, TripState::Below, 80_000);
    assert_eq!((state, crossing), (TripState::Reached, Crossing::Upward));

    // Still reached throughout the hysteresis band. A symmetric implementation using 80_000 in
    // both directions would cross down immediately and oscillate here.
    for temperature in [79_999, 77_500, 75_001, 75_000] {
        assert_eq!(
            update(trip, TripState::Reached, temperature),
            (TripState::Reached, Crossing::None),
            "temperature {temperature} must stay reached"
        );
    }
    assert_eq!(
        update(trip, TripState::Reached, 74_999),
        (TripState::Below, Crossing::Downward)
    );
}

#[test]
fn invalid_trip_never_crosses() {
    let trip = Trip::new(-274_000, 5_000, TripType::Critical).unwrap(); // thermal.h:30
    assert!(!crossed_up(trip, i32::MAX));
    assert!(!crossed_down(trip, i32::MIN));
    assert_eq!(
        update(trip, TripState::Reached, 100_000),
        (TripState::Below, Crossing::None)
    );
}

#[test]
fn ordering_uses_the_threshold_for_the_current_direction() {
    let trips = [
        Trip::new(80_000, 20_000, TripType::Active).unwrap(),
        Trip::new(70_000, 1_000, TripType::Passive).unwrap(),
        Trip::new(90_000, 30_000, TripType::Hot).unwrap(),
        Trip::new(70_000, 2_000, TripType::Critical).unwrap(),
    ];
    let mut indexes = [usize::MAX; 4];

    order_by_threshold(&trips, false, &mut indexes).unwrap();
    assert_eq!(indexes, [1, 3, 0, 2]); // core.c:438 and :425-432; equal 70000 stays stable

    order_by_threshold(&trips, true, &mut indexes).unwrap();
    assert_eq!(indexes, [0, 2, 3, 1]); // core.c:445: lows 60000, 60000, 68000, 69000
}

#[test]
fn invalid_trips_sort_after_valid_thresholds() {
    let trips = [
        Trip::new(THERMAL_TEMP_INVALID, i32::MAX, TripType::Active).unwrap(),
        Trip::new(80_000, 5_000, TripType::Passive).unwrap(),
    ];
    let mut indexes = [usize::MAX; 2];
    order_by_threshold(&trips, false, &mut indexes).unwrap();
    assert_eq!(indexes, [1, 0]); // thermal_core.c:449-453: invalid threshold is INT_MAX
    order_by_threshold(&trips, true, &mut indexes).unwrap();
    assert_eq!(indexes, [1, 0]);
}

#[test]
fn ordering_refuses_a_short_output_by_name() {
    let trips = [
        Trip::new(70_000, 1_000, TripType::Active).unwrap(),
        Trip::new(80_000, 1_000, TripType::Passive).unwrap(),
    ];
    let mut output = [0usize; 1];
    let error = order_by_threshold(&trips, false, &mut output).unwrap_err();
    assert_eq!(
        error,
        OrderError::OutputTooShort {
            provided: 1,
            required: 2
        }
    );
    assert_eq!(
        error.to_string(),
        "thermal trip ordering refused output length 1: at least 2 slots required"
    );
}

#[test]
fn first_crossed_trip_follows_linux_processing_order() {
    let trips = [
        Trip::new(90_000, 20_000, TripType::Critical).unwrap(),
        Trip::new(70_000, 5_000, TripType::Active).unwrap(),
        Trip::new(80_000, 2_000, TripType::Passive).unwrap(),
    ];

    assert_eq!(first_crossed_up(&trips, 69_999), None);
    assert_eq!(first_crossed_up(&trips, 85_000), Some(1)); // core.c:580-586, ascending highs

    assert_eq!(first_crossed_down(&trips, 78_000), None); // equality at low 78000 stays reached
    assert_eq!(first_crossed_down(&trips, 64_999), Some(2)); // core.c:568-578, reverse lows
}

#[test]
fn zero_hysteresis_still_has_asymmetric_boundary_comparisons() {
    let trip = Trip::new(80_000, 0, TripType::Active).unwrap();
    assert!(crossed_up(trip, 80_000)); // thermal_core.c:582: equality crosses upward
    assert!(!crossed_down(trip, 80_000)); // thermal_core.c:570: equality does not cross downward
    assert!(crossed_down(trip, 79_999));
}
