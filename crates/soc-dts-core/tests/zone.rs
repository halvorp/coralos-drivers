// SPDX-License-Identifier: GPL-2.0-only
//! Linux vectors for thermal-zone and interrupt math from `intel_soc_dts_iosf.c:256-355`.
//!
//! Copyright (c) 2015, Intel Corporation.

use soc_dts_core::zone::{
    deassert_apic, has_trip_event, initial_trips, TripKind, ZoneError, ZoneTrip,
};

/// `intel_soc_dts_iosf.c:337-350`: without a critical trip both Linux trips are writable passive
/// trips at literal temperature zero, with indices zero and one.
#[test]
fn ordinary_zone_has_two_named_passive_trips() {
    let expected = [
        ZoneTrip {
            name: "passive",
            kind: TripKind::Passive,
            writable_temperature: true,
            temperature_mc: 0,
            index: 0,
        },
        ZoneTrip {
            name: "passive",
            kind: TripKind::Passive,
            writable_temperature: true,
            temperature_mc: 0,
            index: 1,
        },
    ];
    let got = initial_trips(100_000, false, 0).unwrap();
    assert_eq!(got.len(), 2); // intel_soc_dts_iosf.h:16
    assert_eq!(got.map(|trip| trip.name), ["passive", "passive"]);
    assert_eq!(got, expected);
}

/// Critical temperature is `tj_max - crit_offset` (`intel_soc_dts_iosf.c:344-346`), and that trip
/// is not writable.
#[test]
fn critical_zone_subtracts_the_offset_from_tjmax() {
    let got = initial_trips(100_000, true, 5_000).unwrap();
    assert_eq!(
        got[1],
        ZoneTrip {
            name: "critical",
            kind: TripKind::Critical,
            writable_temperature: false,
            temperature_mc: 95_000,
            index: 1,
        }
    );
}

/// Invalid offset arithmetic is named rather than silently producing a critical point above TjMax
/// or below the representable physical range.
#[test]
fn invalid_critical_offsets_name_value_and_bound() {
    assert_eq!(
        initial_trips(100_000, true, -1),
        Err(ZoneError::CriticalOffsetNegative {
            offset_mc: -1,
            minimum_mc: 0
        })
    );
    assert_eq!(
        initial_trips(100_000, true, 100_001),
        Err(ZoneError::CriticalOffsetAboveTjMax {
            offset_mc: 100_001,
            tj_max_mc: 100_000,
        })
    );
}

/// Interrupt handling ORs literal BIT(4) into PTMC (`intel_soc_dts_iosf.c:265-269`).
#[test]
fn interrupt_deasserts_apic_without_disturbing_other_bits() {
    assert_eq!(deassert_apic(0xA500_0001), 0xA500_0011);
    assert_eq!(deassert_apic(0xA500_0011), 0xA500_0011);
}

/// PTTSS is actionable iff either bit in Linux's literal `0x03` mask is set
/// (`intel_soc_dts_iosf.c:40-41,271-285`).
#[test]
fn only_the_two_trip_status_bits_request_zone_updates() {
    assert!(!has_trip_event(0));
    assert!(has_trip_event(0x01));
    assert!(has_trip_event(0x02));
    assert!(has_trip_event(0x03));
    assert!(!has_trip_event(0x04));
    assert!(!has_trip_event(0xFFFF_FFFC));
}
