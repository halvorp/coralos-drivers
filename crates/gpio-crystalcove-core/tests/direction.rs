// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for direction and level encodings ported from Linux
//! `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

use gpio_crystalcove_core::direction::{
    input_control, input_level, output_control, output_value_update, ValueRefusal, CTLO_DIR_IN,
    CTLO_DIR_OUT, CTLO_DRV_CMOS, CTLO_DRV_OD, CTLO_DRV_REN, CTLO_INPUT_SET, CTLO_OUTPUT_SET,
    CTLO_RVAL_2KDW, CTLO_RVAL_2KUP, CTLO_RVAL_50KDW, CTLO_RVAL_50KUP, GPIO_VALUE_MASK,
};

/// gpio-crystalcove.c:44-58. Expected values are Linux literals, written out independently.
#[test]
fn every_direction_drive_and_resistor_literal_matches_linux() {
    assert_eq!(CTLO_DIR_IN, 0);
    assert_eq!(CTLO_DIR_OUT, 1 << 5);
    assert_eq!(CTLO_DRV_CMOS, 0);
    assert_eq!(CTLO_DRV_OD, 1 << 4);
    assert_eq!(CTLO_DRV_REN, 1 << 3);
    assert_eq!(CTLO_RVAL_2KDW, 0);
    assert_eq!(CTLO_RVAL_2KUP, 1 << 1);
    assert_eq!(CTLO_RVAL_50KDW, 2 << 1);
    assert_eq!(CTLO_RVAL_50KUP, 3 << 1);
    assert_eq!(CTLO_INPUT_SET, 0x0a);
    assert_eq!(CTLO_OUTPUT_SET, 0x2a);
    assert_eq!(GPIO_VALUE_MASK, 0x01);
}

/// gpio-crystalcove.c:57 and :141 writes CTLO_INPUT_SET, whose Linux literal expression is 0x0a.
#[test]
fn input_direction_uses_the_linux_composite() {
    assert_eq!(input_control(), 0x0a);
}

/// gpio-crystalcove.c:58 and :152 OR the initial logical value into CTLO_OUTPUT_SET.
#[test]
fn output_direction_encodes_both_logical_values() {
    assert_eq!(output_control(0), Ok(0x2a));
    assert_eq!(output_control(1), Ok(0x2b));
}

/// A wider value would spill into Linux's CTLO resistor field at bit 1 rather than being a logical
/// GPIO level. The refusal names both value and bound.
#[test]
fn output_direction_refuses_a_non_boolean_value() {
    assert_eq!(
        output_control(2),
        Err(ValueRefusal::OutputValueOutOfRange {
            value: 2,
            maximum: 1
        })
    );
}

/// gpio-crystalcove.c:178-182 updates mask literal 1 with value literal 1 or 0.
#[test]
fn set_value_produces_the_linux_mask_value_pairs() {
    assert_eq!(output_value_update(false), (1, 0));
    assert_eq!(output_value_update(true), (1, 1));
}

/// gpio-crystalcove.c:168 returns `val & 0x1`; unrelated CTLI bits must not change the level.
#[test]
fn input_level_decodes_only_bit_zero() {
    assert!(!input_level(0x00));
    assert!(input_level(0x01));
    assert!(!input_level(0xfe));
    assert!(input_level(0xff));
}
