// SPDX-License-Identifier: GPL-2.0-only
//! COMMAND-register flag composition.
//!
//! Ported from Linux `sdhci_send_command()` in `drivers/mmc/host/sdhci.c:1700-:1725`, using the
//! response definitions in `include/linux/mmc/core.h:35-:66` and tuning opcodes in
//! `include/linux/mmc/mmc.h:55-:56,102-:106`.
//!
//! Original copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved, and the Linux MMC authors.

use sdhci_core::regs::{
    SDHCI_CMD_CRC, SDHCI_CMD_DATA, SDHCI_CMD_INDEX, SDHCI_CMD_RESP_LONG, SDHCI_CMD_RESP_NONE,
    SDHCI_CMD_RESP_SHORT, SDHCI_CMD_RESP_SHORT_BUSY,
};

pub const MMC_RSP_PRESENT: u32 = 1 << 0; // include/linux/mmc/core.h:35
pub const MMC_RSP_136: u32 = 1 << 1; // include/linux/mmc/core.h:36
pub const MMC_RSP_CRC: u32 = 1 << 2; // include/linux/mmc/core.h:37
pub const MMC_RSP_BUSY: u32 = 1 << 3; // include/linux/mmc/core.h:38
pub const MMC_RSP_OPCODE: u32 = 1 << 4; // include/linux/mmc/core.h:39

/// Linux's named native response types (`include/linux/mmc/core.h:57-:66`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseType {
    None,
    R1,
    R1b,
    R1bNoCrc,
    R2,
    R3,
    R4,
    R5,
    R6,
    R7,
}

/// Frozen order and names of all ten native response definitions in Linux.
pub const RESPONSE_TYPES: [(&str, ResponseType); 10] = [
    ("MMC_RSP_NONE", ResponseType::None), // include/linux/mmc/core.h:57
    ("MMC_RSP_R1", ResponseType::R1), // include/linux/mmc/core.h:58
    ("MMC_RSP_R1B", ResponseType::R1b), // include/linux/mmc/core.h:59
    ("MMC_RSP_R1B_NO_CRC", ResponseType::R1bNoCrc), // include/linux/mmc/core.h:60
    ("MMC_RSP_R2", ResponseType::R2), // include/linux/mmc/core.h:61
    ("MMC_RSP_R3", ResponseType::R3), // include/linux/mmc/core.h:62
    ("MMC_RSP_R4", ResponseType::R4), // include/linux/mmc/core.h:63
    ("MMC_RSP_R5", ResponseType::R5), // include/linux/mmc/core.h:64
    ("MMC_RSP_R6", ResponseType::R6), // include/linux/mmc/core.h:65
    ("MMC_RSP_R7", ResponseType::R7), // include/linux/mmc/core.h:66
];

/// Return Linux's MMC flag literal for a named native response type.
pub const fn response_flags(response: ResponseType) -> u32 {
    match response {
        ResponseType::None => 0,
        ResponseType::R1 | ResponseType::R5 | ResponseType::R6 | ResponseType::R7 => {
            MMC_RSP_PRESENT | MMC_RSP_CRC | MMC_RSP_OPCODE
        }
        ResponseType::R1b => MMC_RSP_PRESENT | MMC_RSP_CRC | MMC_RSP_OPCODE | MMC_RSP_BUSY,
        ResponseType::R1bNoCrc => MMC_RSP_PRESENT | MMC_RSP_OPCODE | MMC_RSP_BUSY,
        ResponseType::R2 => MMC_RSP_PRESENT | MMC_RSP_136 | MMC_RSP_CRC,
        ResponseType::R3 | ResponseType::R4 => MMC_RSP_PRESENT,
    }
}

/// Compose the low COMMAND-register flags exactly as `sdhci_send_command()` does.
///
/// Data Present Select is set for an attached data transfer and for CMD19/CMD21 tuning even when
/// no data object is attached (`drivers/mmc/host/sdhci.c:1723-:1725`).
pub const fn command_flags(response: ResponseType, has_data: bool, opcode: u32) -> u16 {
    let rsp = response_flags(response);
    let mut flags = if rsp & MMC_RSP_PRESENT == 0 {
        SDHCI_CMD_RESP_NONE
    } else if rsp & MMC_RSP_136 != 0 {
        SDHCI_CMD_RESP_LONG
    } else if rsp & MMC_RSP_BUSY != 0 {
        SDHCI_CMD_RESP_SHORT_BUSY
    } else {
        SDHCI_CMD_RESP_SHORT
    };

    if rsp & MMC_RSP_CRC != 0 {
        flags |= SDHCI_CMD_CRC;
    }
    if rsp & MMC_RSP_OPCODE != 0 {
        flags |= SDHCI_CMD_INDEX;
    }
    if has_data || opcode == 19 || opcode == 21 {
        flags |= SDHCI_CMD_DATA;
    }
    flags
}
