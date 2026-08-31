// SPDX-License-Identifier: GPL-2.0-only
//! Linux vectors for trip encode/decode and register updates from `intel_soc_dts_iosf.c:43-145`.
//!
//! Copyright (c) 2015, Intel Corporation.

use soc_dts_core::trip::{
    decode_trip_temperature, encode_trip_temperature, replace_trip_byte, trip_is_writable,
    trip_register_update, InterruptMode, TripError, TripRegisterUpdate,
};

/// `(tj_max - temp) / 1000` at `intel_soc_dts_iosf.c:59`.
#[test]
fn trip_temperature_encodes_in_degrees_below_tjmax() {
    assert_eq!(encode_trip_temperature(75_000, 100_000), Ok(25));
    assert_eq!(encode_trip_temperature(100_000, 100_000), Ok(0));
}

/// The inverse of Linux's literal-1000 encoding receives its own vector.
#[test]
fn trip_temperature_decodes_from_degrees_below_tjmax() {
    assert_eq!(decode_trip_temperature(25, 100_000), Ok(75_000));
    assert_eq!(decode_trip_temperature(0xFF, 100_000), Ok(-155_000));
}

/// `sys_set_trip_temp` refuses a value above TjMax (`intel_soc_dts_iosf.c:141-142`). Other lossy
/// values are also named rather than silently truncated or masked.
#[test]
fn lossy_or_out_of_range_encodings_are_named() {
    assert_eq!(
        encode_trip_temperature(101_000, 100_000),
        Err(TripError::TripTemperatureAboveTjMax {
            temperature_mc: 101_000,
            tj_max_mc: 100_000,
        })
    );
    assert_eq!(
        encode_trip_temperature(99_500, 100_000),
        Err(TripError::TripDeltaNotWholeDegrees {
            temperature_mc: 99_500,
            tj_max_mc: 100_000,
        })
    );
    assert_eq!(
        encode_trip_temperature(-156_000, 100_000),
        Err(TripError::TripEncodingOutOfRange {
            delta_mc: 256_000,
            maximum_encoding: 0xFF,
        })
    );
}

/// `bitmap_set_value8(..., thres_index * 8)` (`intel_soc_dts_iosf.c:66-68`) replaces exactly one
/// byte and preserves the other 24 bits.
#[test]
fn replacing_each_trip_byte_preserves_every_other_bit() {
    assert_eq!(replace_trip_byte(0xAABB_CCDD, 0, 0x19), Ok(0xAABB_CC19));
    assert_eq!(replace_trip_byte(0xAABB_CCDD, 1, 0x19), Ok(0xAABB_19DD));
    assert_eq!(
        replace_trip_byte(0, 2, 0),
        Err(TripError::TripIndexOutOfRange {
            index: 2,
            maximum: 1
        })
    );
}

/// BIOS ownership check at `intel_soc_dts_iosf.c:221-230`: a non-zero corresponding byte removes
/// writable-temperature permission.
#[test]
fn firmware_programmed_trip_bytes_are_not_writable() {
    assert_eq!(trip_is_writable(0x0000_1900, 0), Ok(true));
    assert_eq!(trip_is_writable(0x0000_1900, 1), Ok(false));
    assert_eq!(
        trip_is_writable(0, 3),
        Err(TripError::TripIndexOutOfRange {
            index: 3,
            maximum: 1
        })
    );
}

/// Lines 54-57, 66-68, 90-98, and 111-113: a non-zero MSI trip sets the PTPS byte, both CPU
/// modules, AUX1, APICA+MSI, and chooses literal offset `0xB6`.
#[test]
fn nonzero_msi_trip_produces_complete_literal_register_values() {
    assert_eq!(
        trip_register_update(
            0xAABB_CCDD,
            0x0000_0000,
            0x0000_0004,
            1,
            75_000,
            100_000,
            InterruptMode::Msi,
        ),
        Ok(TripRegisterUpdate {
            ptps: 0xAABB_19DD,
            ptmc: 0x0003_0002,
            te_offset: 0xB6,
            te_value: 0x0000_4804,
        })
    );
}

/// Lines 59 and 99-104: temp zero encodes `tj_max / 1000`, clears only the selected AUX enable
/// and APICA route, and leaves MSI alone in non-MSI mode because Linux did not include it in
/// `int_enable_bit`.
#[test]
fn zero_trip_disables_only_linuxs_selected_bits() {
    assert_eq!(
        trip_register_update(
            0x1122_3344,
            0x0000_0003,
            0x0000_4A04,
            0,
            0,
            100_000,
            InterruptMode::Apica,
        ),
        Ok(TripRegisterUpdate {
            ptps: 0x1122_3364,
            ptmc: 0x0003_0002,
            te_offset: 0xB5,
            te_value: 0x0000_0A04,
        })
    );
}
