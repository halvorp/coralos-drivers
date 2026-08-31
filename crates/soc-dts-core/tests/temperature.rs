// SPDX-License-Identifier: GPL-2.0-only
//! Linux vectors for current-temperature decoding from `intel_soc_dts_iosf.c:151-170`.
//!
//! Copyright (c) 2015, Intel Corporation.

use soc_dts_core::temperature::{decode_temperature, DecodeError};

/// Linux extracts byte `id`, subtracts literal `0x7F`, and subtracts the result times literal
/// `1000` from TjMax (`intel_soc_dts_iosf.c:166-168`).
#[test]
fn each_sensor_byte_decodes_independently() {
    // DTS0 raw=0x80 => 100000 - (0x80 - 0x7F) * 1000 = 99000.
    // DTS1 raw=0x85 => 100000 - (0x85 - 0x7F) * 1000 = 94000.
    let register = 0x0000_8580;
    assert_eq!(decode_temperature(register, 0, 100_000), Ok(99_000));
    assert_eq!(decode_temperature(register, 1, 100_000), Ok(94_000));
}

/// Literal `0x7F` is exactly TjMax, while `0xFF` is 128 C below it
/// (`intel_soc_dts_iosf.c:38,167-168`).
#[test]
fn endpoint_encodings_match_linux_math() {
    assert_eq!(decode_temperature(0x0000_007F, 0, 100_000), Ok(100_000));
    assert_eq!(decode_temperature(0x0000_00FF, 0, 100_000), Ok(-28_000));
}

/// Only DTS0 and DTS1 exist (`intel_soc_dts_iosf.h:12-13`); a caller typo is named rather than
/// shifting by an arbitrary amount.
#[test]
fn a_nonexistent_sensor_is_refused_with_its_bound() {
    assert_eq!(
        decode_temperature(0, 2, 100_000),
        Err(DecodeError::SensorIndexOutOfRange {
            index: 2,
            maximum: 1
        })
    );
}

/// The Linux expression uses unsigned `out`; below `0x7F`, subtraction and multiplication wrap
/// modulo 2^32 before assignment to signed `int` (`intel_soc_dts_iosf.c:155,167-168`). For one
/// degree above TjMax this yields TjMax + 1000, not a giant negative value.
#[test]
fn raw_below_tjmax_encoding_retains_linux_unsigned_math() {
    assert_eq!(decode_temperature(0x0000_007E, 0, 100_000), Ok(101_000));
}
