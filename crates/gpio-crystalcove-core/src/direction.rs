// SPDX-License-Identifier: GPL-2.0-only
//! Crystal Cove GPIO direction, drive and level register encodings.
//!
//! Mechanically ported from Linux `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

pub const CTLO_DIR_IN: u8 = 0; // gpio-crystalcove.c:44
pub const CTLO_DIR_OUT: u8 = 1 << 5; // gpio-crystalcove.c:45
pub const CTLO_DRV_CMOS: u8 = 0; // gpio-crystalcove.c:47
pub const CTLO_DRV_OD: u8 = 1 << 4; // gpio-crystalcove.c:48
pub const CTLO_DRV_REN: u8 = 1 << 3; // gpio-crystalcove.c:50
pub const CTLO_RVAL_2KDW: u8 = 0; // gpio-crystalcove.c:52
pub const CTLO_RVAL_2KUP: u8 = 1 << 1; // gpio-crystalcove.c:53
pub const CTLO_RVAL_50KDW: u8 = 2 << 1; // gpio-crystalcove.c:54
pub const CTLO_RVAL_50KUP: u8 = 3 << 1; // gpio-crystalcove.c:55
pub const CTLO_INPUT_SET: u8 = CTLO_DRV_CMOS | CTLO_DRV_REN | CTLO_RVAL_2KUP; // gpio-crystalcove.c:57
pub const CTLO_OUTPUT_SET: u8 = CTLO_DIR_OUT | CTLO_INPUT_SET; // gpio-crystalcove.c:58
pub const GPIO_VALUE_MASK: u8 = 0x1; // gpio-crystalcove.c:168

/// Why an output value could not be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueRefusal {
    /// The value bit has width one. Linux receives a logical `int value`; accepting wider values
    /// here would spill into the resistor field when ORed at gpio-crystalcove.c:152.
    OutputValueOutOfRange { value: u8, maximum: u8 },
}

/// Complete CTLO value used when selecting input direction.
pub fn input_control() -> u8 {
    CTLO_INPUT_SET // gpio-crystalcove.c:141
}

/// Complete CTLO value used when selecting output direction and its initial logical value.
pub fn output_control(value: u8) -> Result<u8, ValueRefusal> {
    if value > 1 {
        return Err(ValueRefusal::OutputValueOutOfRange { value, maximum: 1 });
    }
    Ok(CTLO_OUTPUT_SET | value) // gpio-crystalcove.c:152
}

/// Encode the mask/value pair used to update an output's logical value.
pub fn output_value_update(value: bool) -> (u8, u8) {
    (GPIO_VALUE_MASK, if value { GPIO_VALUE_MASK } else { 0 }) // gpio-crystalcove.c:178-182
}

/// Decode the logical input level from CTLI.
pub fn input_level(ctli: u8) -> bool {
    ctli & GPIO_VALUE_MASK != 0 // gpio-crystalcove.c:168
}
