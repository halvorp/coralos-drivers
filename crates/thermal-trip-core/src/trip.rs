// SPDX-License-Identifier: GPL-2.0-only
//! Trip types, validation, ordering, and directional crossing decisions.
//!
//! Ported from Linux `drivers/thermal/thermal_core.c`, `drivers/thermal/thermal_trip.c`,
//! `drivers/thermal/thermal_sysfs.c`, `include/linux/thermal.h`, and
//! `include/uapi/linux/thermal.h`.
//!
//! Copyright (C) 2008 Intel Corp
//! Copyright (C) 2008 Zhang Rui <rui.zhang@intel.com>
//! Copyright (C) 2008 Sujith Thomas <sujith.thomas@intel.com>
//! Copyright 2022 Linaro Limited

use core::fmt;

/// Linux's invalid/uninitialized temperature sentinel, in millidegrees Celsius.
///
/// `include/linux/thermal.h:30` defines `THERMAL_TEMP_INVALID` as `-274000`.
pub const THERMAL_TEMP_INVALID: i32 = -274_000;

/// Number of trip types Linux defines.
///
/// `include/uapi/linux/thermal.h:14-19` defines four enum members.
pub const TRIP_TYPE_COUNT: usize = 4;

/// Thermal trip-point type and its Linux discriminant.
///
/// `include/uapi/linux/thermal.h:14-19`.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TripType {
    /// Cooling devices are activated at this trip. `include/uapi/linux/thermal.h:15`.
    Active = 0,
    /// Passive cooling/polling is active while reached. `include/uapi/linux/thermal.h:16`.
    Passive = 1,
    /// A hot notification/callback is issued on an upward crossing. `include/uapi/linux/thermal.h:17`.
    Hot = 2,
    /// The critical protection callback is issued on an upward crossing. `include/uapi/linux/thermal.h:18`.
    Critical = 3,
}

/// Every trip type in Linux enum order.
///
/// `include/uapi/linux/thermal.h:14-19`.
pub const TRIP_TYPES: [TripType; TRIP_TYPE_COUNT] = [
    TripType::Active,
    TripType::Passive,
    TripType::Hot,
    TripType::Critical,
];

impl TripType {
    /// Linux's user-visible name for this trip type.
    ///
    /// `drivers/thermal/thermal_trip.c:12-17`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Passive => "passive",
            Self::Hot => "hot",
            Self::Critical => "critical",
        }
    }

    /// Whether Linux forwards crossings of this type to a thermal governor.
    ///
    /// `drivers/thermal/thermal_core.c:456-465` excludes hot and critical trips.
    pub const fn governor_managed(self) -> bool {
        !matches!(self, Self::Hot | Self::Critical)
    }

    /// Type-specific action taken on an upward crossing.
    ///
    /// `drivers/thermal/thermal_core.c:475-480`: passive increments the passive-trip count, hot
    /// invokes the hot handler, critical invokes protection, and active has no core-side action.
    pub const fn upward_action(self) -> UpwardAction {
        match self {
            Self::Active => UpwardAction::None,
            Self::Passive => UpwardAction::EnterPassive,
            Self::Hot => UpwardAction::NotifyHot,
            Self::Critical => UpwardAction::ProtectCritical,
        }
    }
}

/// Core-side action associated with an upward crossing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpwardAction {
    /// No governor-independent special action.
    None,
    /// Increment the zone's passive-trip count.
    EnterPassive,
    /// Invoke the zone's hot callback when present.
    NotifyHot,
    /// Invoke the zone's critical protection callback.
    ProtectCritical,
}

/// A thermal trip point, in millidegrees Celsius.
///
/// This is the arithmetic subset of Linux `struct thermal_trip` (`include/linux/thermal.h:65-78`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trip {
    /// Upward crossing threshold.
    temperature: i32,
    /// Amount subtracted from `temperature` for the downward threshold.
    hysteresis: i32,
    /// Trip semantics.
    trip_type: TripType,
}

impl Trip {
    /// Construct and validate a trip point.
    pub fn new(
        temperature: i32,
        hysteresis: i32,
        trip_type: TripType,
    ) -> Result<Self, ValidationError> {
        validate(temperature, hysteresis)?;
        Ok(Self {
            temperature,
            hysteresis,
            trip_type,
        })
    }

    /// Configured temperature in millidegrees Celsius.
    ///
    /// `include/linux/thermal.h:65-75`.
    pub const fn temperature(self) -> i32 {
        self.temperature
    }

    /// Configured hysteresis in millidegrees Celsius.
    ///
    /// `include/linux/thermal.h:65-75`.
    pub const fn hysteresis(self) -> i32 {
        self.hysteresis
    }

    /// Configured trip type.
    ///
    /// `include/linux/thermal.h:65-75`.
    pub const fn trip_type(self) -> TripType {
        self.trip_type
    }

    /// Whether this trip is disabled by Linux's exact sentinel.
    ///
    /// `drivers/thermal/thermal_core.c:1420-1426` uses equality with `THERMAL_TEMP_INVALID` when
    /// classifying trips. Values below the sentinel are refused by [`validate`].
    pub const fn is_invalid(self) -> bool {
        self.temperature == THERMAL_TEMP_INVALID
    }

    /// Upward crossing threshold.
    ///
    /// `drivers/thermal/thermal_core.c:435-439` sets a high trip's threshold to its temperature.
    pub const fn upward_threshold(self) -> i32 {
        self.temperature
    }

    /// Downward crossing threshold, or Linux's invalid-list threshold for a disabled trip.
    ///
    /// `drivers/thermal/thermal_core.c:442-453` sets a reached trip's threshold to
    /// `temperature - hysteresis`, but sets an invalid trip's threshold to `INT_MAX`. Handling the
    /// sentinel first also prevents an arbitrary permitted invalid-trip hysteresis from overflowing.
    pub const fn downward_threshold(self) -> i32 {
        if self.is_invalid() {
            i32::MAX
        } else {
            self.temperature - self.hysteresis
        }
    }
}

/// Why a trip point was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    /// Hysteresis was negative; Linux accepts only values at least zero.
    NegativeHysteresis {
        /// Refused value.
        hysteresis: i32,
        /// Inclusive lower bound.
        minimum: i32,
    },
    /// A non-sentinel temperature did not leave a valid downward threshold.
    TemperatureAtOrBelowMinimum {
        /// Refused temperature.
        temperature: i32,
        /// Hysteresis used to determine the exclusive minimum.
        hysteresis: i32,
        /// Temperature must be strictly greater than this bound.
        exclusive_minimum: i32,
    },
    /// `temperature - hysteresis` would be at or below the invalid sentinel.
    DownwardThresholdAtOrBelowInvalid {
        /// Trip temperature.
        temperature: i32,
        /// Refused hysteresis.
        hysteresis: i32,
        /// Computed downward threshold.
        downward_threshold: i32,
        /// Threshold must be strictly greater than this bound.
        exclusive_minimum: i32,
    },
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::NegativeHysteresis { hysteresis, minimum } => write!(
                f,
                "thermal trip refused hysteresis {hysteresis}: minimum is {minimum}"
            ),
            Self::TemperatureAtOrBelowMinimum {
                temperature,
                hysteresis,
                exclusive_minimum,
            } => write!(
                f,
                "thermal trip refused temperature {temperature} with hysteresis {hysteresis}: temperature must be greater than {exclusive_minimum} or equal THERMAL_TEMP_INVALID"
            ),
            Self::DownwardThresholdAtOrBelowInvalid {
                temperature,
                hysteresis,
                downward_threshold,
                exclusive_minimum,
            } => write!(
                f,
                "thermal trip refused hysteresis {hysteresis} at temperature {temperature}: downward threshold {downward_threshold} must be greater than {exclusive_minimum}"
            ),
        }
    }
}

/// Validate Linux's temperature/hysteresis invariants.
///
/// `drivers/thermal/thermal_sysfs.c:156-176` refuses negative hysteresis and requires every valid
/// trip's `temperature - hysteresis` to be strictly above `THERMAL_TEMP_INVALID`. Linux explicitly
/// permits any nonnegative hysteresis while temperature equals the invalid sentinel (:164-172).
pub fn validate(temperature: i32, hysteresis: i32) -> Result<(), ValidationError> {
    if hysteresis < 0 {
        return Err(ValidationError::NegativeHysteresis {
            hysteresis,
            minimum: 0,
        });
    }

    if temperature == THERMAL_TEMP_INVALID {
        return Ok(());
    }

    // This is Linux's overflow-safe arrangement from thermal_sysfs.c:119-122.
    let exclusive_minimum = THERMAL_TEMP_INVALID + hysteresis;
    if temperature <= exclusive_minimum {
        return Err(ValidationError::TemperatureAtOrBelowMinimum {
            temperature,
            hysteresis,
            exclusive_minimum,
        });
    }

    Ok(())
}

/// Validate a new hysteresis for an existing trip temperature.
///
/// This mirrors the write path in `drivers/thermal/thermal_sysfs.c:149-178`, including its special
/// allowance for invalid trips and its direct downward-threshold check at lines 175-176.
pub fn validate_hysteresis(temperature: i32, hysteresis: i32) -> Result<(), ValidationError> {
    if hysteresis < 0 {
        return Err(ValidationError::NegativeHysteresis {
            hysteresis,
            minimum: 0,
        });
    }
    if temperature == THERMAL_TEMP_INVALID {
        return Ok(());
    }

    let downward_threshold = temperature.checked_sub(hysteresis).unwrap_or(i32::MIN);
    if downward_threshold <= THERMAL_TEMP_INVALID {
        return Err(ValidationError::DownwardThresholdAtOrBelowInvalid {
            temperature,
            hysteresis,
            downward_threshold,
            exclusive_minimum: THERMAL_TEMP_INVALID,
        });
    }
    Ok(())
}

/// Whether a not-yet-reached trip crosses upward at `temperature`.
///
/// `drivers/thermal/thermal_core.c:580-586` crosses when the high threshold is less than or equal
/// to the zone temperature. Invalid trips never enter the high list (`thermal_core.c:1419-1421`).
pub const fn crossed_up(trip: Trip, temperature: i32) -> bool {
    !trip.is_invalid() && trip.upward_threshold() <= temperature
}

/// Whether an already-reached trip crosses downward at `temperature`.
///
/// `drivers/thermal/thermal_core.c:568-578` crosses down only when the low threshold is strictly
/// greater than the zone temperature. Equality stays reached. This strict, hysteretic direction is
/// intentionally different from [`crossed_up`].
pub const fn crossed_down(trip: Trip, temperature: i32) -> bool {
    !trip.is_invalid() && trip.downward_threshold() > temperature
}

/// Current state of a trip in Linux's high/reached partition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripState {
    /// The trip has not crossed upward yet.
    Below,
    /// The trip has crossed upward and has not crossed downward yet.
    Reached,
}

/// A crossing emitted while updating one trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crossing {
    /// No threshold was crossed.
    None,
    /// Upward threshold was reached.
    Upward,
    /// Temperature fell strictly below the downward threshold.
    Downward,
}

/// Apply one caller-supplied temperature to one trip's prior state.
///
/// This is the governor-independent decision in `thermal_zone_handle_trips`
/// (`drivers/thermal/thermal_core.c:561-590`). The state matters: while reached, the trip compares
/// against its lower hysteresis threshold rather than its upward temperature.
pub const fn update(trip: Trip, state: TripState, temperature: i32) -> (TripState, Crossing) {
    if trip.is_invalid() {
        return (TripState::Below, Crossing::None);
    }

    match state {
        TripState::Below if crossed_up(trip, temperature) => (TripState::Reached, Crossing::Upward),
        TripState::Reached if crossed_down(trip, temperature) => {
            (TripState::Below, Crossing::Downward)
        }
        _ => (state, Crossing::None),
    }
}

/// Sort trip indexes into Linux processing order without allocation.
///
/// `drivers/thermal/thermal_core.c:414-446` keeps both high and reached lists in ascending
/// threshold order. `reached` selects downward thresholds; otherwise upward thresholds are used.
/// Equal thresholds retain input order, matching Linux's insertion-after-equals behavior at lines
/// 425-432.
pub fn order_by_threshold(
    trips: &[Trip],
    reached: bool,
    indexes: &mut [usize],
) -> Result<(), OrderError> {
    if indexes.len() < trips.len() {
        return Err(OrderError::OutputTooShort {
            provided: indexes.len(),
            required: trips.len(),
        });
    }

    for (index, slot) in indexes[..trips.len()].iter_mut().enumerate() {
        *slot = index;
    }

    // Stable insertion sort is no_std, allocation-free, and mechanically matches insertion into
    // Linux's sorted linked lists.
    for right in 1..trips.len() {
        let index = indexes[right];
        let threshold = threshold_for_order(trips[index], reached);
        let mut position = right;
        while position > 0 {
            let previous = indexes[position - 1];
            let previous_threshold = threshold_for_order(trips[previous], reached);
            if previous_threshold <= threshold {
                break;
            }
            indexes[position] = previous;
            position -= 1;
        }
        indexes[position] = index;
    }

    Ok(())
}

/// Linux puts invalid trips on a separate list with `INT_MAX` as their threshold
/// (`drivers/thermal/thermal_core.c:449-453`), so they sort after every valid trip.
const fn threshold_for_order(trip: Trip, reached: bool) -> i32 {
    if trip.is_invalid() {
        i32::MAX
    } else if reached {
        trip.downward_threshold()
    } else {
        trip.upward_threshold()
    }
}

/// Why threshold ordering was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderError {
    /// Caller did not provide one output slot per trip.
    OutputTooShort {
        /// Number of output slots supplied.
        provided: usize,
        /// Number of slots required.
        required: usize,
    },
}

impl fmt::Display for OrderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::OutputTooShort { provided, required } => write!(
                f,
                "thermal trip ordering refused output length {provided}: at least {required} slots required"
            ),
        }
    }
}

/// First trip crossed upward at a caller-supplied temperature, in threshold order.
///
/// This is the pure counterpart of the ascending `trips_high` walk in
/// `drivers/thermal/thermal_core.c:580-586`. The returned index refers to the caller's slice.
pub fn first_crossed_up(trips: &[Trip], temperature: i32) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for (index, trip) in trips.iter().copied().enumerate() {
        if crossed_up(trip, temperature) {
            let threshold = trip.upward_threshold();
            if best.map_or(true, |(_, best_threshold)| threshold < best_threshold) {
                best = Some((index, threshold));
            }
        }
    }
    best.map(|(index, _)| index)
}

/// First already-reached trip crossed downward at a caller-supplied temperature.
///
/// This is the pure counterpart of the reverse `trips_reached` walk in
/// `drivers/thermal/thermal_core.c:568-578`: the highest crossed low threshold is handled first.
/// The returned index refers to the caller's slice.
pub fn first_crossed_down(trips: &[Trip], temperature: i32) -> Option<usize> {
    let mut best: Option<(usize, i32)> = None;
    for (index, trip) in trips.iter().copied().enumerate() {
        if crossed_down(trip, temperature) {
            let threshold = trip.downward_threshold();
            if best.map_or(true, |(_, best_threshold)| threshold > best_threshold) {
                best = Some((index, threshold));
            }
        }
    }
    best.map(|(index, _)| index)
}
