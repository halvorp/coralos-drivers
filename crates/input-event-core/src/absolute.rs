// SPDX-License-Identifier: GPL-2.0-only
//! Absolute-axis filtering and metadata from Linux `drivers/input/input.c`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use crate::codes::{ABS_MAX, ABS_MT_SLOT, ABS_MT_TOOL_Y, ABS_MT_TOUCH_MAJOR};

/// Linux's absolute-axis metadata (`input_set_abs_params`, input.c:455-473).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbsInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
}

/// Named refusal for absolute-axis configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsConfigError {
    AxisOutOfRange { axis: u16, max: u16 },
    MinimumExceedsMaximum { minimum: i32, maximum: i32 },
    NegativeFuzz { fuzz: i32 },
    NegativeFlat { flat: i32 },
    FlatExceedsRange { flat: i32, minimum: i32, maximum: i32 },
}

/// Validate and construct metadata corresponding to `input_set_abs_params` (input.c:455-473).
///
/// Linux's in-kernel callers are trusted; this pure boundary names malformed ranges instead of
/// indexing beyond `ABS_CNT` or silently accepting impossible flat/fuzz values.
pub fn configure_axis(
    axis: u16,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
) -> Result<AbsInfo, AbsConfigError> {
    if axis > ABS_MAX {
        return Err(AbsConfigError::AxisOutOfRange { axis, max: ABS_MAX });
    }
    if minimum > maximum {
        return Err(AbsConfigError::MinimumExceedsMaximum { minimum, maximum });
    }
    if fuzz < 0 {
        return Err(AbsConfigError::NegativeFuzz { fuzz });
    }
    if flat < 0 {
        return Err(AbsConfigError::NegativeFlat { flat });
    }
    let range = (maximum as i64) - (minimum as i64);
    if (flat as i64) > range {
        return Err(AbsConfigError::FlatExceedsRange { flat, minimum, maximum });
    }
    Ok(AbsInfo { value: 0, minimum, maximum, fuzz, flat })
}

/// Clamp an absolute value to its declared inclusive bounds.
///
/// The bounds come from `input_set_abs_params` (input.c:468-471). This is explicit preprocessing:
/// Linux `input_handle_abs_event` itself defuzzes but does not silently clamp the device's report.
pub fn clamp_value(value: i32, info: &AbsInfo) -> i32 {
    value.clamp(info.minimum, info.maximum)
}

/// Apply Linux's three defuzz bands (`input_defuzz_abs_event`, input.c:71-85).
///
/// Arithmetic is widened so this host-testable port preserves the mathematical C expressions at
/// extreme inputs rather than introducing debug-build overflow panics.
pub fn defuzz(value: i32, old_value: i32, fuzz: i32) -> i32 {
    if fuzz != 0 {
        let value = value as i64;
        let old = old_value as i64;
        let fuzz = fuzz as i64;
        if value > old - fuzz / 2 && value < old + fuzz / 2 {
            return old_value;
        }
        if value > old - fuzz && value < old + fuzz {
            return ((old * 3 + value) / 4) as i32;
        }
        if value > old - fuzz * 2 && value < old + fuzz * 2 {
            return ((old + value) / 2) as i32;
        }
    }
    value
}

/// Apply an axis's flat dead zone around zero.
///
/// Linux stores `flat` at input.c:471 and consumers use it as the centre dead zone. This helper is
/// deliberately separate from routing because input.c preserves raw absolute values for handlers.
pub fn apply_flat(value: i32, flat: i32) -> i32 {
    if -(flat as i64) <= value as i64 && (value as i64) <= flat as i64 { 0 } else { value }
}

/// Whether a code is a multi-touch value rather than the slot selector (input.c:174-184).
pub fn is_mt_value(code: u16) -> bool {
    (ABS_MT_TOUCH_MAJOR..=ABS_MT_TOOL_Y).contains(&code) && code != ABS_MT_SLOT
}

/// Result of filtering a non-slotted absolute value (input.c:186-199).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbsFilter {
    Unchanged,
    Changed(i32),
}

/// Defuzz an absolute report and suppress it if the stored value does not change.
pub fn filter_value(value: i32, old_value: i32, fuzz: i32) -> AbsFilter {
    let filtered = defuzz(value, old_value, fuzz);
    if filtered == old_value { AbsFilter::Unchanged } else { AbsFilter::Changed(filtered) }
}
