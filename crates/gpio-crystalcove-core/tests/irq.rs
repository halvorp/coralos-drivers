// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for IRQ encodings ported from Linux `drivers/gpio/gpio-crystalcove.c`.
//!
//! Copyright (C) 2012, 2014 Intel Corporation. All rights reserved.
//! Original author: Yang, Bin <bin.yang@intel.com>.

use gpio_crystalcove_core::irq::{
    irq_mask_register, irq_mask_update, irq_status_register, irq_type_update, irq_type_value,
    pending_bitmap, IrqRefusal, IrqType, CTLI_INTCNT_BE, CTLI_INTCNT_DIS, CTLI_INTCNT_MASK,
    CTLI_INTCNT_NE, CTLI_INTCNT_PE, IRQ_TYPE_NAMES, UPDATE_IRQ_MASK, UPDATE_IRQ_TYPE,
};

/// gpio-crystalcove.c:24-25 and :39-42. Expected values are Linux literals.
#[test]
fn irq_update_and_detect_literals_match_linux() {
    assert_eq!(UPDATE_IRQ_TYPE, 1 << 0);
    assert_eq!(UPDATE_IRQ_MASK, 1 << 1);
    assert_eq!(CTLI_INTCNT_DIS, 0);
    assert_eq!(CTLI_INTCNT_NE, 1 << 1);
    assert_eq!(CTLI_INTCNT_PE, 2 << 1);
    assert_eq!(CTLI_INTCNT_BE, 3 << 1);
    assert_eq!(CTLI_INTCNT_MASK, 0x06);
}

/// gpio-crystalcove.c:194-204 accepts exactly four named cases. This frozen expectation list is
/// literal and independent of the production list.
#[test]
fn irq_type_count_and_names_match_linux_switch_cases() {
    const LINUX_NAMES: [&str; 4] = [
        "IRQ_TYPE_NONE",
        "IRQ_TYPE_EDGE_BOTH",
        "IRQ_TYPE_EDGE_RISING",
        "IRQ_TYPE_EDGE_FALLING",
    ];
    assert_eq!(IRQ_TYPE_NAMES.len(), 4);
    assert_eq!(IRQ_TYPE_NAMES, LINUX_NAMES);
}

/// gpio-crystalcove.c:194-204 maps none/both/rising/falling to 0/6/4/2.
#[test]
fn every_irq_type_maps_to_its_linux_intcnt_value() {
    assert_eq!(irq_type_value(IrqType::None), Ok(0));
    assert_eq!(irq_type_value(IrqType::EdgeBoth), Ok(6));
    assert_eq!(irq_type_value(IrqType::EdgeRising), Ok(4));
    assert_eq!(irq_type_value(IrqType::EdgeFalling), Ok(2));
}

/// gpio-crystalcove.c:206-207 rejects any switch value other than the four accepted types.
#[test]
fn unsupported_irq_type_is_a_named_refusal() {
    assert_eq!(
        irq_type_value(IrqType::Unsupported(4)),
        Err(IrqRefusal::UnsupportedIrqType { irq_type: 4 })
    );
}

/// gpio-crystalcove.c:126-130 updates mask CTLI_INTCNT_BE (literal 6) with the encoded type.
#[test]
fn irq_type_update_carries_linux_mask_and_value() {
    assert_eq!(irq_type_update(IrqType::None), Ok((6, 0)));
    assert_eq!(irq_type_update(IrqType::EdgeBoth), Ok((6, 6)));
    assert_eq!(irq_type_update(IrqType::EdgeRising), Ok((6, 4)));
    assert_eq!(irq_type_update(IrqType::EdgeFalling), Ok((6, 2)));
    assert_eq!(
        irq_type_update(IrqType::Unsupported(0x80)),
        Err(IrqRefusal::UnsupportedIrqType { irq_type: 0x80 })
    );
}

/// gpio-crystalcove.c:117-118 chooses literal registers 0x19/0x1a and BIT(gpio % 8).
#[test]
fn irq_mask_register_decodes_banks_and_pin_bits() {
    assert_eq!(irq_mask_register(0), Ok((0x19, 0x01)));
    assert_eq!(irq_mask_register(7), Ok((0x19, 0x80)));
    assert_eq!(irq_mask_register(8), Ok((0x1a, 0x01)));
    assert_eq!(irq_mask_register(15), Ok((0x1a, 0x80)));
    assert_eq!(
        irq_mask_register(16),
        Err(IrqRefusal::GpioOutOfRange {
            gpio: 16,
            maximum: 15
        })
    );
}

/// gpio-crystalcove.c:120-123 writes the mask bit to mask and zero to unmask.
#[test]
fn irq_mask_update_encodes_mask_and_unmask() {
    assert_eq!(irq_mask_update(9, true), Ok((0x1a, 0x02, 0x02)));
    assert_eq!(irq_mask_update(9, false), Ok((0x1a, 0x02, 0x00)));
    assert_eq!(
        irq_mask_update(0x5e, true),
        Err(IrqRefusal::GpioOutOfRange {
            gpio: 0x5e,
            maximum: 15
        })
    );
}

/// gpio-crystalcove.c:315-327 chooses literal status registers 0x0b/0x0c and the bank-local bit.
#[test]
fn irq_status_register_decodes_banks_and_pin_bits() {
    assert_eq!(irq_status_register(0), Ok((0x0b, 0x01)));
    assert_eq!(irq_status_register(7), Ok((0x0b, 0x80)));
    assert_eq!(irq_status_register(8), Ok((0x0c, 0x01)));
    assert_eq!(irq_status_register(15), Ok((0x0c, 0x80)));
    assert_eq!(
        irq_status_register(95),
        Err(IrqRefusal::GpioOutOfRange {
            gpio: 95,
            maximum: 15
        })
    );
}

/// gpio-crystalcove.c:292 computes `p0 | p1 << 8`.
#[test]
fn pending_bitmap_places_bank_one_in_the_high_byte() {
    assert_eq!(pending_bitmap(0x00, 0x00), 0x0000);
    assert_eq!(pending_bitmap(0xa5, 0x5a), 0x5aa5);
    assert_eq!(pending_bitmap(0xff, 0xff), 0xffff);
}
