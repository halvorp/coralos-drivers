// SPDX-License-Identifier: GPL-2.0-only
//! Interrupt vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::interrupt::*;
use pinctrl_cherryview_core::regs::INVALID_HWIRQ;

#[test]
fn interrupt_line_field_ack_and_mask_vectors_match_linux() {
    assert_eq!(interrupt_line(0xa000_0000), 10); // IntSel bits 31:28, :40-41, :1177-1180
    assert_eq!(encode_interrupt_line(0x0abc_def0, 5), Ok(0x5abc_def0));
    assert_eq!(
        encode_interrupt_line(0, 16),
        Err(MappingError::InterruptLineOutOfRange {
            line: 16,
            line_count: 16
        })
    );
    assert_eq!(acknowledge_word(0x3000_0000), 0x8); // BIT(intr_line), :1180
    assert_eq!(update_interrupt_mask(0xffff, 3, true), Ok(0xfff7)); // mask clears, :1195-1196
    assert_eq!(update_interrupt_mask(0, 3, false), Ok(0x8)); // unmask sets, :1197-1198
    assert_eq!(
        update_interrupt_mask(0, 16, false),
        Err(MappingError::InterruptLineOutOfRange {
            line: 16,
            line_count: 16
        })
    );
}

#[test]
fn every_linux_trigger_encoding_is_literal() {
    // IntWakeCfg values 1,2,3,4 and low-level RXDATA inversion bit 6; :1351-1369.
    assert_eq!(encode_trigger(0xf8, Trigger::EdgeFalling), 0x09);
    assert_eq!(encode_trigger(0xf8, Trigger::EdgeRising), 0x0a);
    assert_eq!(encode_trigger(0xf8, Trigger::EdgeBoth), 0x0b);
    assert_eq!(encode_trigger(0xf8, Trigger::LevelHigh), 0x0c);
    assert_eq!(encode_trigger(0xf8, Trigger::LevelLow), 0x4c);
}

#[test]
fn mapping_keeps_free_bios_line_and_reassigns_conflicts_from_the_top() {
    let mut lines = [INVALID_HWIRQ; 16]; // intr_lines[16], :83-88; initialized invalid :1632-1634
    let kept = map_gpio_to_interrupt_line(&mut lines, 8, 42, 0x2000_1234, false).unwrap();
    assert_eq!(
        kept,
        MappingUpdate {
            padctrl0: 0x2000_1234,
            line: 2,
            changed: false
        }
    );
    assert_eq!(lines[2], 42);

    lines[1] = 11;
    let moved = map_gpio_to_interrupt_line(&mut lines, 8, 22, 0x1000_5678, false).unwrap();
    // Search community->nirqs - 1 downward, :1305-1319.
    assert_eq!(
        moved,
        MappingUpdate {
            padctrl0: 0x7000_5678,
            line: 7,
            changed: true
        }
    );
    assert_eq!(lines[7], 22);
}

#[test]
fn mapping_refusals_name_the_line_owner_pin_and_bound() {
    let mut lines = [INVALID_HWIRQ; 16];
    lines[3] = 9;
    assert_eq!(
        map_gpio_to_interrupt_line(&mut lines, 8, 12, 0x3000_0000, true),
        Err(MappingError::InterruptLineConflictOnLockedPin {
            line: 3,
            owner: 9,
            requested_pin: 12
        }) // :1298-1302
    );
    assert_eq!(
        map_gpio_to_interrupt_line(&mut lines, 8, 12, 0xf000_0000, false),
        Err(MappingError::InterruptLineOutOfRange {
            line: 15,
            line_count: 8
        })
    );
    assert_eq!(
        map_gpio_to_interrupt_line(&mut lines, 17, 12, 0, false),
        Err(MappingError::CommunityLineCountOutOfRange {
            line_count: 17,
            maximum: 16
        })
    );
    for slot in &mut lines[..8] {
        *slot = 1;
    }
    assert_eq!(
        map_gpio_to_interrupt_line(&mut lines, 8, 12, 0x3000_0000, false),
        Err(MappingError::NoFreeInterruptLine {
            requested_pin: 12,
            line_count: 8
        }) // :1308-1313
    );
}
