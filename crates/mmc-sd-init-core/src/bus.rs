// SPDX-License-Identifier: GPL-2.0-only
//! SD bus-width selection, kept separate from command transport.
//!
//! Ported from Linux `drivers/mmc/core/sd.c`, `drivers/mmc/core/sd_ops.c`,
//! `include/linux/mmc/card.h`, `include/linux/mmc/host.h`, and
//! `include/linux/mmc/sd.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

use crate::scr::SD_SCR_BUS_WIDTH_4;

pub const MMC_BUS_WIDTH_1: u8 = 0; // include/linux/mmc/host.h:49
pub const MMC_BUS_WIDTH_4: u8 = 2; // include/linux/mmc/host.h:50
pub const SD_BUS_WIDTH_1: u32 = 0; // include/linux/mmc/sd.h:78
pub const SD_BUS_WIDTH_4: u32 = 2; // include/linux/mmc/sd.h:79

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusWidthDef {
    pub name: &'static str,
    pub host_width: u8,
    pub command_argument: u32,
}

pub const BUS_WIDTHS: [BusWidthDef; 2] = [
    BusWidthDef {
        name: "1-bit",
        host_width: MMC_BUS_WIDTH_1,
        command_argument: SD_BUS_WIDTH_1,
    }, // sd_ops.c:128-131
    BusWidthDef {
        name: "4-bit",
        host_width: MMC_BUS_WIDTH_4,
        command_argument: SD_BUS_WIDTH_4,
    }, // sd_ops.c:132-134
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusWidthError {
    UnsupportedHostWidth {
        value: u8,
        supported_1_bit: u8,
        supported_4_bit: u8,
    },
}

/// Choose 4-bit only when both host and SCR permit it; otherwise retain 1-bit.
pub fn select_bus_width(host_supports_4_bit: bool, scr_bus_widths: u8) -> u8 {
    if host_supports_4_bit && scr_bus_widths & SD_SCR_BUS_WIDTH_4 != 0 {
        MMC_BUS_WIDTH_4
    } else {
        MMC_BUS_WIDTH_1
    } // drivers/mmc/core/sd.c:1550-1560
}

/// Convert Linux's host bus-width representation to the ACMD6 argument.
pub fn app_set_bus_width_argument(host_width: u8) -> Result<u32, BusWidthError> {
    match host_width {
        MMC_BUS_WIDTH_1 => Ok(SD_BUS_WIDTH_1), // drivers/mmc/core/sd_ops.c:128-131
        MMC_BUS_WIDTH_4 => Ok(SD_BUS_WIDTH_4), // drivers/mmc/core/sd_ops.c:132-134
        value => Err(BusWidthError::UnsupportedHostWidth {
            value,
            supported_1_bit: MMC_BUS_WIDTH_1,
            supported_4_bit: MMC_BUS_WIDTH_4,
        }), // drivers/mmc/core/sd_ops.c:135-136
    }
}
