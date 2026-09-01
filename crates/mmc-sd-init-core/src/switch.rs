// SPDX-License-Identifier: GPL-2.0-only
//! SD switch-function status decode and UHS bus-speed selection.
//!
//! Ported from Linux `drivers/mmc/core/sd.c` and `include/linux/mmc/card.h`.
//! Copyright (C) 2003-2004 Russell King; 2004 Ian Molton; 2005-2007 Pierre Ossman.

pub const HIGH_SPEED_MAX_DTR: u32 = 50_000_000; // include/linux/mmc/card.h:154
pub const UHS_SDR104_BUS_SPEED: u8 = 3; // include/linux/mmc/card.h:166
pub const UHS_DDR50_BUS_SPEED: u8 = 4; // include/linux/mmc/card.h:167
pub const UHS_SDR50_BUS_SPEED: u8 = 2; // include/linux/mmc/card.h:165
pub const UHS_SDR25_BUS_SPEED: u8 = 1; // include/linux/mmc/card.h:164
pub const UHS_SDR12_BUS_SPEED: u8 = 0; // include/linux/mmc/card.h:162
pub const SD_MODE_HIGH_SPEED: u8 = 1 << 1; // include/linux/mmc/card.h:163,169
pub const SD_MODE_UHS_SDR12: u8 = 1 << 0; // include/linux/mmc/card.h:170
pub const SD_MODE_UHS_SDR25: u8 = 1 << 1; // include/linux/mmc/card.h:171
pub const SD_MODE_UHS_SDR50: u8 = 1 << 2; // include/linux/mmc/card.h:172
pub const SD_MODE_UHS_SDR104: u8 = 1 << 3; // include/linux/mmc/card.h:173
pub const SD_MODE_UHS_DDR50: u8 = 1 << 4; // include/linux/mmc/card.h:174

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchField {
    pub name: &'static str,
    pub byte: u8,
    pub mask: u16,
}

pub const SWITCH_STATUS_FIELDS: [SwitchField; 5] = [
    SwitchField {
        name: "CURRENT_LIMITS",
        byte: 6,
        mask: 0xffff,
    }, // drivers/mmc/core/sd.c:368
    SwitchField {
        name: "DRIVER_STRENGTHS",
        byte: 9,
        mask: 0x00ff,
    }, // drivers/mmc/core/sd.c:366-367
    SwitchField {
        name: "BUS_SPEED_MODES",
        byte: 13,
        mask: 0x00ff,
    }, // drivers/mmc/core/sd.c:361-365
    SwitchField {
        name: "DRIVE_SELECTION",
        byte: 15,
        mask: 0x000f,
    }, // drivers/mmc/core/sd.c:438
    SwitchField {
        name: "BUS_SPEED_SELECTION",
        byte: 16,
        mask: 0x000f,
    }, // drivers/mmc/core/sd.c:406
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwitchCaps {
    pub hs_max_dtr: u32,
    pub sd3_bus_mode: u8,
    pub sd3_drv_type: u8,
    pub sd3_curr_limit: u16,
}

/// Decode the support bytes Linux consumes from a 64-byte CMD6 status block.
pub fn decode_status(status: &[u8; 64], sda_spec3: bool, host_hs_max_hz: u32) -> SwitchCaps {
    let hs_max_dtr = if status[13] & SD_MODE_HIGH_SPEED != 0 {
        if host_hs_max_hz == 0 {
            HIGH_SPEED_MAX_DTR
        } else {
            host_hs_max_hz
        }
    } else {
        0
    }; // drivers/mmc/core/sd.c:361-362
    if sda_spec3 {
        SwitchCaps {
            hs_max_dtr,
            sd3_bus_mode: status[13],
            sd3_drv_type: status[9],
            sd3_curr_limit: u16::from(status[7]) | (u16::from(status[6]) << 8),
        } // drivers/mmc/core/sd.c:364-369
    } else {
        SwitchCaps {
            hs_max_dtr,
            sd3_bus_mode: 0,
            sd3_drv_type: 0,
            sd3_curr_limit: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchError {
    UnsupportedFunctionGroup {
        value: u8,
        supported_groups: [u8; 3],
    },
}

/// Check the function selected by a switch-mode CMD6 status response.
pub fn selected_function(status: &[u8; 64], group: u8) -> Result<u8, SwitchError> {
    match group {
        0 => Ok(status[16] & 0x0f),        // drivers/mmc/core/sd.c:406,519
        2 => Ok(status[15] & 0x0f),        // drivers/mmc/core/sd.c:438
        3 => Ok((status[15] >> 4) & 0x0f), // drivers/mmc/core/sd.c:608
        value => Err(SwitchError::UnsupportedFunctionGroup {
            value,
            supported_groups: [0, 2, 3],
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UhsMode {
    pub name: &'static str,
    pub host_cap: u32,
    pub card_mode: u8,
    pub bus_speed: u8,
}

pub const MMC_CAP_UHS_SDR12: u32 = 1 << 16; // include/linux/mmc/host.h:410
pub const MMC_CAP_UHS_SDR25: u32 = 1 << 17; // include/linux/mmc/host.h:411
pub const MMC_CAP_UHS_SDR50: u32 = 1 << 18; // include/linux/mmc/host.h:412
pub const MMC_CAP_UHS_SDR104: u32 = 1 << 19; // include/linux/mmc/host.h:413
pub const MMC_CAP_UHS_DDR50: u32 = 1 << 20; // include/linux/mmc/host.h:414

pub const UHS_MODES: [UhsMode; 5] = [
    UhsMode {
        name: "SDR104",
        host_cap: MMC_CAP_UHS_SDR104,
        card_mode: SD_MODE_UHS_SDR104,
        bus_speed: UHS_SDR104_BUS_SPEED,
    }, // sd.c:463-465
    UhsMode {
        name: "DDR50",
        host_cap: MMC_CAP_UHS_DDR50,
        card_mode: SD_MODE_UHS_DDR50,
        bus_speed: UHS_DDR50_BUS_SPEED,
    }, // sd.c:466-468
    UhsMode {
        name: "SDR50",
        host_cap: MMC_CAP_UHS_SDR104 | MMC_CAP_UHS_SDR50,
        card_mode: SD_MODE_UHS_SDR50,
        bus_speed: UHS_SDR50_BUS_SPEED,
    }, // sd.c:469-472
    UhsMode {
        name: "SDR25",
        host_cap: MMC_CAP_UHS_SDR104 | MMC_CAP_UHS_SDR50 | MMC_CAP_UHS_SDR25,
        card_mode: SD_MODE_UHS_SDR25,
        bus_speed: UHS_SDR25_BUS_SPEED,
    }, // sd.c:473-476
    UhsMode {
        name: "SDR12",
        host_cap: MMC_CAP_UHS_SDR104 | MMC_CAP_UHS_SDR50 | MMC_CAP_UHS_SDR25 | MMC_CAP_UHS_SDR12,
        card_mode: SD_MODE_UHS_SDR12,
        bus_speed: UHS_SDR12_BUS_SPEED,
    }, // sd.c:477-481
];

/// Apply Linux's priority order, including its fallback capability masks.
pub fn select_uhs_bus_speed(host_caps: u32, card_modes: u8) -> Option<u8> {
    UHS_MODES
        .iter()
        .find(|mode| host_caps & mode.host_cap != 0 && card_modes & mode.card_mode != 0)
        .map(|mode| mode.bus_speed) // drivers/mmc/core/sd.c:463-482
}
