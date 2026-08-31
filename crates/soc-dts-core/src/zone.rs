// SPDX-License-Identifier: GPL-2.0-only
//! Thermal-zone setup and interrupt math ported from Linux
//! `drivers/thermal/intel/intel_soc_dts_iosf.c:256-287,297-355`.
//!
//! Copyright (c) 2015, Intel Corporation.

use crate::registers::{bit, TRIP_MASK};

/// Linux thermal trip kind used by this driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripKind {
    Passive,
    Critical,
}

/// Pure description of one thermal-zone trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZoneTrip {
    pub name: &'static str,
    pub kind: TripKind,
    pub writable_temperature: bool,
    pub temperature_mc: i32,
    pub index: usize,
}

/// Why thermal-zone construction was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneError {
    CriticalOffsetNegative { offset_mc: i32, minimum_mc: i32 },
    CriticalOffsetAboveTjMax { offset_mc: i32, tj_max_mc: i32 },
}

/// Build Linux's two initial trips for one sensor.
///
/// Trip 0 is always writable/passive at zero (`intel_soc_dts_iosf.c:337-338`). Trip 1 is either
/// critical at `tj_max - crit_offset` (`:344-346`) or writable/passive at zero (`:347-350`).
pub fn initial_trips(
    tj_max_mc: i32,
    critical_trip: bool,
    critical_offset_mc: i32,
) -> Result<[ZoneTrip; 2], ZoneError> {
    if critical_trip && critical_offset_mc < 0 {
        return Err(ZoneError::CriticalOffsetNegative {
            offset_mc: critical_offset_mc,
            minimum_mc: 0,
        });
    }
    if critical_trip && critical_offset_mc > tj_max_mc {
        return Err(ZoneError::CriticalOffsetAboveTjMax {
            offset_mc: critical_offset_mc,
            tj_max_mc,
        });
    }

    let passive = ZoneTrip {
        name: "passive",
        kind: TripKind::Passive,
        writable_temperature: true,
        temperature_mc: 0,
        index: 0,
    }; // intel_soc_dts_iosf.c:337-338
    let second = if critical_trip {
        ZoneTrip {
            name: "critical",
            kind: TripKind::Critical,
            writable_temperature: false,
            temperature_mc: tj_max_mc - critical_offset_mc,
            index: 1,
        } // intel_soc_dts_iosf.c:344-346
    } else {
        ZoneTrip {
            name: "passive",
            kind: TripKind::Passive,
            writable_temperature: true,
            temperature_mc: 0,
            index: 1,
        } // intel_soc_dts_iosf.c:347-350
    };
    Ok([passive, second])
}

/// Set the APIC deassert bit in PTMC before processing a thermal interrupt.
pub fn deassert_apic(ptmc: u32) -> u32 {
    ptmc | bit::PTMC_APIC_DEASSERT // intel_soc_dts_iosf.c:265-269
}

/// Whether either of the two OSPM trip sticky bits requests thermal-zone notification.
pub fn has_trip_event(pttss: u32) -> bool {
    pttss & TRIP_MASK != 0 // intel_soc_dts_iosf.c:271-285
}
