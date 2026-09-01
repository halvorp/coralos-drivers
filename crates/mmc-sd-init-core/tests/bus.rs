// SPDX-License-Identifier: GPL-2.0-only
//! Bus-width vectors from Linux `drivers/mmc/core/sd.c`, `sd_ops.c`, and MMC/SD headers.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use mmc_sd_init_core::{bus::*, scr::SD_SCR_BUS_WIDTH_4};

#[test]
fn every_supported_bus_width_is_pinned_by_name_and_values() {
    let expected = [
        ("1-bit", 0, 0), // sd_ops.c:128-131
        ("4-bit", 2, 2), // sd_ops.c:132-134
    ];
    assert_eq!(BUS_WIDTHS.len(), 2);
    assert_eq!(
        BUS_WIDTHS.map(|x| (x.name, x.host_width, x.command_argument)),
        expected
    );
}

#[test]
fn width_selection_requires_both_host_and_card_support() {
    assert_eq!(select_bus_width(true, SD_SCR_BUS_WIDTH_4), 2); // sd.c:1553-1560
    assert_eq!(select_bus_width(false, SD_SCR_BUS_WIDTH_4), 0);
    assert_eq!(select_bus_width(true, 0), 0);
}

#[test]
fn acmd6_arguments_match_linux_and_refusal_names_supported_values() {
    assert_eq!(app_set_bus_width_argument(0), Ok(0)); // sd_ops.c:129-131
    assert_eq!(app_set_bus_width_argument(2), Ok(2)); // sd_ops.c:132-134
    assert_eq!(
        app_set_bus_width_argument(3),
        Err(BusWidthError::UnsupportedHostWidth {
            value: 3,
            supported_1_bit: 0,
            supported_4_bit: 2,
        })
    ); // sd_ops.c:135-136
}
