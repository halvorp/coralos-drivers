// SPDX-License-Identifier: GPL-2.0-only
//! Switch-status vectors from Linux `drivers/mmc/core/sd.c` and MMC card/host headers.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::switch::*;

#[test]
fn every_consumed_switch_status_field_is_pinned() {
    let expected = [
        ("CURRENT_LIMITS", 6, 0xffff),
        ("DRIVER_STRENGTHS", 9, 0x00ff),
        ("BUS_SPEED_MODES", 13, 0x00ff),
        ("DRIVE_SELECTION", 15, 0x000f),
        ("BUS_SPEED_SELECTION", 16, 0x000f), // sd.c:361-369,406,438
    ];
    assert_eq!(SWITCH_STATUS_FIELDS.len(), 5);
    assert_eq!(
        SWITCH_STATUS_FIELDS.map(|x| (x.name, x.byte, x.mask)),
        expected
    );
}

#[test]
fn switch_status_decode_matches_linux_byte_order() {
    let mut status = [0u8; 64];
    status[13] = 0x1f;
    status[9] = 0x0d;
    status[7] = 0x34;
    status[6] = 0x12;
    let caps = decode_status(&status, true, 0);
    assert_eq!(caps.hs_max_dtr, 50_000_000); // card.h:154; sd.c:361-362
    assert_eq!(caps.sd3_bus_mode, 0x1f);
    assert_eq!(caps.sd3_drv_type, 0x0d);
    assert_eq!(caps.sd3_curr_limit, 0x1234); // sd.c:364-369
    assert_eq!(
        decode_status(&status, true, 42_000_000).hs_max_dtr,
        42_000_000
    );
    assert_eq!(decode_status(&status, false, 0).sd3_bus_mode, 0);
}

#[test]
fn selected_function_uses_each_linux_nibble() {
    let mut status = [0u8; 64];
    status[16] = 0xa3;
    status[15] = 0xb4;
    assert_eq!(selected_function(&status, 0), Ok(3)); // sd.c:406,519
    assert_eq!(selected_function(&status, 2), Ok(4)); // sd.c:438
    assert_eq!(selected_function(&status, 3), Ok(11)); // sd.c:608
    assert_eq!(
        selected_function(&status, 1),
        Err(SwitchError::UnsupportedFunctionGroup {
            value: 1,
            supported_groups: [0, 2, 3],
        })
    );
}

#[test]
fn every_uhs_priority_entry_is_pinned_by_name_and_literals() {
    let expected = [
        ("SDR104", 0x0008_0000, 0x08, 3),
        ("DDR50", 0x0010_0000, 0x10, 4),
        ("SDR50", 0x000c_0000, 0x04, 2),
        ("SDR25", 0x000e_0000, 0x02, 1),
        ("SDR12", 0x000f_0000, 0x01, 0), // sd.c:463-481
    ];
    assert_eq!(UHS_MODES.len(), 5);
    assert_eq!(
        UHS_MODES.map(|x| (x.name, x.host_cap, x.card_mode, x.bus_speed)),
        expected
    );
}

#[test]
fn uhs_selection_preserves_linux_priority_and_fallback_masks() {
    assert_eq!(select_uhs_bus_speed(0x0018_0000, 0x18), Some(3)); // SDR104 before DDR50
    assert_eq!(select_uhs_bus_speed(0x0008_0000, 0x04), Some(2)); // SDR104 host may use SDR50
    assert_eq!(select_uhs_bus_speed(0x0008_0000, 0x01), Some(0)); // and SDR12
    assert_eq!(select_uhs_bus_speed(0, 0x1f), None);
}
