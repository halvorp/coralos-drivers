// SPDX-License-Identifier: GPL-2.0-only
//! Trip-point byte encoding/decoding and pure register updates ported from Linux
//! `drivers/thermal/intel/intel_soc_dts_iosf.c:43-129` (`update_trip_temp`).
//!
//! Copyright (c) 2015, Intel Corporation.

use crate::registers::{bit, offset, TRIP_COUNT};

/// Interrupt routing relevant to Linux's trip update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptMode {
    /// Any Linux mode except MSI: APICA is enabled by itself. // intel_soc_dts_iosf.c:54-57
    Apica,
    /// MSI mode: APICA and MSI are both enabled. // intel_soc_dts_iosf.c:56-57
    Msi,
}

/// Why a trip operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TripError {
    TripIndexOutOfRange {
        index: usize,
        maximum: usize,
    },
    TripTemperatureAboveTjMax {
        temperature_mc: i32,
        tj_max_mc: i32,
    },
    TripDeltaNotWholeDegrees {
        temperature_mc: i32,
        tj_max_mc: i32,
    },
    TripEncodingOutOfRange {
        delta_mc: i32,
        maximum_encoding: u16,
    },
    DecodedTemperatureOutOfRange {
        encoded: u8,
        tj_max_mc: i32,
    },
}

/// The complete register values a caller should write for one trip update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TripRegisterUpdate {
    pub ptps: u32,
    pub ptmc: u32,
    pub te_offset: u32,
    pub te_value: u32,
}

fn check_index(trip_index: usize) -> Result<(), TripError> {
    if trip_index >= TRIP_COUNT {
        return Err(TripError::TripIndexOutOfRange {
            index: trip_index,
            maximum: TRIP_COUNT - 1,
        });
    }
    Ok(())
}

/// Encode a trip temperature as the byte Linux stores in PTPS.
///
/// This is `(tj_max - temp) / 1000` from `intel_soc_dts_iosf.c:59`, constrained before the Linux
/// `& 0xFF` at line 67 so a caller gets a named refusal instead of a silently wrapped trip.
pub fn encode_trip_temperature(temperature_mc: i32, tj_max_mc: i32) -> Result<u8, TripError> {
    if temperature_mc > tj_max_mc {
        return Err(TripError::TripTemperatureAboveTjMax {
            temperature_mc,
            tj_max_mc,
        }); // intel_soc_dts_iosf.c:141-142
    }
    let delta_mc = tj_max_mc - temperature_mc;
    if delta_mc % 1000 != 0 {
        return Err(TripError::TripDeltaNotWholeDegrees {
            temperature_mc,
            tj_max_mc,
        });
    }
    let encoded = delta_mc / 1000; // intel_soc_dts_iosf.c:59
    if encoded > 0xFF {
        return Err(TripError::TripEncodingOutOfRange {
            delta_mc,
            maximum_encoding: 0xFF,
        }); // intel_soc_dts_iosf.c:67
    }
    Ok(encoded as u8)
}

/// Decode one PTPS trip byte into milli-Celsius, the inverse of Linux's line 59 encoding.
pub fn decode_trip_temperature(encoded: u8, tj_max_mc: i32) -> Result<i32, TripError> {
    tj_max_mc
        .checked_sub(encoded as i32 * 1000)
        .ok_or(TripError::DecodedTemperatureOutOfRange { encoded, tj_max_mc })
}

/// Replace exactly one of PTPS's two OSPM trip bytes.
///
/// Mirrors `bitmap_set_value8(..., thres_index * 8)` at `intel_soc_dts_iosf.c:66-68` while taking
/// the old register value as input instead of performing MMIO.
pub fn replace_trip_byte(ptps: u32, trip_index: usize, encoded: u8) -> Result<u32, TripError> {
    check_index(trip_index)?;
    let shift = trip_index * 8; // intel_soc_dts_iosf.c:67
    let mask = 0xFFu32 << shift; // intel_soc_dts_iosf.c:67
    Ok((ptps & !mask) | ((encoded as u32) << shift))
}

/// Whether Linux leaves a trip writable after discovering firmware already programmed its byte.
///
/// `intel_soc_dts_iosf.c:221-230` clears `THERMAL_TRIP_FLAG_RW_TEMP` when the corresponding PTPS
/// byte is non-zero.
pub fn trip_is_writable(ptps: u32, trip_index: usize) -> Result<bool, TripError> {
    check_index(trip_index)?;
    Ok(ptps & (0xFFu32 << (trip_index * 8)) == 0) // intel_soc_dts_iosf.c:227-229
}

/// Compute every register value Linux changes in `update_trip_temp`, without touching hardware.
pub fn trip_register_update(
    ptps: u32,
    ptmc: u32,
    te_value: u32,
    trip_index: usize,
    temperature_mc: i32,
    tj_max_mc: i32,
    interrupt_mode: InterruptMode,
) -> Result<TripRegisterUpdate, TripError> {
    check_index(trip_index)?;
    let encoded = encode_trip_temperature(temperature_mc, tj_max_mc)?;
    let ptps = replace_trip_byte(ptps, trip_index, encoded)?; // intel_soc_dts_iosf.c:66-68

    let mut ptmc = ptmc | bit::CPU_MODULE0_ENABLE | bit::CPU_MODULE1_ENABLE; // intel_soc_dts_iosf.c:90-92
    let mut te_value = te_value;
    let aux_enable = if trip_index == 0 {
        bit::AUX0_ENABLE
    } else {
        bit::AUX1_ENABLE
    };
    let mut interrupt_enable = bit::TE_APICA_ENABLE; // intel_soc_dts_iosf.c:54
    if interrupt_mode == InterruptMode::Msi {
        interrupt_enable |= bit::TE_MSI_ENABLE; // intel_soc_dts_iosf.c:56-57
    }

    if temperature_mc != 0 {
        ptmc |= aux_enable; // intel_soc_dts_iosf.c:93-98
        te_value |= interrupt_enable;
    } else {
        ptmc &= !aux_enable; // intel_soc_dts_iosf.c:99-104
        te_value &= !interrupt_enable;
    }

    Ok(TripRegisterUpdate {
        ptps,
        ptmc,
        te_offset: offset::TE_AUX0 + trip_index as u32, // intel_soc_dts_iosf.c:83-85,111-113
        te_value,
    })
}
