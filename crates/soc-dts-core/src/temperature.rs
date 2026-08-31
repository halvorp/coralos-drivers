// SPDX-License-Identifier: GPL-2.0-only
//! Current-temperature register decoding ported from Linux
//! `drivers/thermal/intel/intel_soc_dts_iosf.c:151-170` (`sys_get_curr_temp`).
//!
//! Copyright (c) 2015, Intel Corporation.

use crate::registers::{SENSOR_COUNT, TJMAX_ENCODING};

/// Why a temperature-register decode was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// No byte exists for this DTS sensor in the Linux-defined sensor set.
    SensorIndexOutOfRange { index: usize, maximum: usize },
}

/// Decode one sensor byte from `SOC_DTS_OFFSET_TEMP` into milli-Celsius.
///
/// Linux extracts byte `id`, subtracts `SOC_DTS_TJMAX_ENCODING`, then computes
/// `tj_max - out * 1000` (`intel_soc_dts_iosf.c:166-168`). The C subtraction is unsigned because
/// `out` is `u32`; wrapping is retained here before conversion to the thermal API's signed `int`.
pub fn decode_temperature(
    temp_register: u32,
    sensor_index: usize,
    tj_max_mc: i32,
) -> Result<i32, DecodeError> {
    if sensor_index >= SENSOR_COUNT {
        return Err(DecodeError::SensorIndexOutOfRange {
            index: sensor_index,
            maximum: SENSOR_COUNT - 1,
        });
    }

    let raw = ((temp_register >> (sensor_index * 8)) & 0xFF) as u8; // intel_soc_dts_iosf.c:166-167
    let delta = (raw as u32).wrapping_sub(TJMAX_ENCODING as u32); // intel_soc_dts_iosf.c:167
    let temperature = (tj_max_mc as u32).wrapping_sub(delta.wrapping_mul(1000)); // intel_soc_dts_iosf.c:168
    Ok(temperature as i32) // intel_soc_dts_iosf.c:168 assigns the u32 expression to `int`
}
