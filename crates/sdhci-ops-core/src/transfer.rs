// SPDX-License-Identifier: GPL-2.0-only
//! TRANSFER_MODE register word composition.
//!
//! Ported from Linux `sdhci_auto_cmd_select()` and `sdhci_set_transfer_mode()` in
//! `drivers/mmc/host/sdhci.c:1419-:1498`, using fields from `drivers/mmc/host/sdhci.h:37-:44`.
//!
//! Original copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved, and the Linux SDHCI/MMC
//! authors.

use sdhci_core::regs::{
    SDHCI_TRNS_AUTO_CMD12, SDHCI_TRNS_AUTO_CMD23, SDHCI_TRNS_BLK_CNT_EN, SDHCI_TRNS_DMA,
    SDHCI_TRNS_MULTI, SDHCI_TRNS_READ,
};

/// Transfer direction consumed from `MMC_DATA_READ` (`include/linux/mmc/core.h:128-:129`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataDirection {
    Write,
    Read,
}

/// Automatic command selected by Linux's mutually exclusive Auto CMD logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCommand {
    None,
    Cmd12,
    Cmd23,
}

/// Frozen names of the three automatic-command outcomes in `sdhci_auto_cmd_select()`.
pub const AUTO_COMMANDS: [(&str, AutoCommand); 3] = [
    ("None", AutoCommand::None),
    ("SDHCI_AUTO_CMD12", AutoCommand::Cmd12), // drivers/mmc/host/sdhci.c:1454-:1455
    ("SDHCI_AUTO_CMD23", AutoCommand::Cmd23), // drivers/mmc/host/sdhci.c:1456-:1457
];

/// Inputs needed to reproduce the pure TRANSFER_MODE decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferConfig {
    pub opcode: u32,
    pub blocks: u32,
    pub direction: DataDirection,
    pub use_dma: bool,
    pub support_single: bool,
    pub auto_command: AutoCommand,
    pub version_410_or_later: bool,
    pub v4_mode: bool,
}

/// TRANSFER_MODE and the associated CMD23-selection bit for HOST_CONTROL2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransferMode {
    pub word: u16,
    pub cmd23_enable: Option<bool>,
}

/// Compose the data-bearing TRANSFER_MODE word from Linux's decisions.
///
/// `cmd23_enable` is `Some` only when Version 4.10 Auto Select is active; it tells the MMIO layer
/// whether Linux would set or clear `SDHCI_CMD23_ENABLE` (`drivers/mmc/host/sdhci.c:1438-:1445`).
pub fn transfer_mode(config: TransferConfig) -> TransferMode {
    let mut mode = if config.support_single {
        0
    } else {
        SDHCI_TRNS_BLK_CNT_EN
    };
    let mut cmd23_enable = None;

    if config.opcode == 18 || config.opcode == 25 || config.blocks > 1 {
        mode = SDHCI_TRNS_BLK_CNT_EN | SDHCI_TRNS_MULTI;
        if config.auto_command != AutoCommand::None {
            if config.version_410_or_later && config.v4_mode {
                // SDHCI_TRNS_AUTO_SEL is the combined 0x04 | 0x08 encoding (sdhci.h:40-:42).
                mode |= SDHCI_TRNS_AUTO_CMD12 | SDHCI_TRNS_AUTO_CMD23;
                cmd23_enable = Some(config.auto_command == AutoCommand::Cmd23);
            } else if config.auto_command == AutoCommand::Cmd12 && config.opcode != 53 {
                mode |= SDHCI_TRNS_AUTO_CMD12;
            } else if config.auto_command == AutoCommand::Cmd23 {
                mode |= SDHCI_TRNS_AUTO_CMD23;
            }
        }
    }

    if config.direction == DataDirection::Read {
        mode |= SDHCI_TRNS_READ;
    }
    if config.use_dma {
        mode |= SDHCI_TRNS_DMA;
    }

    TransferMode { word: mode, cmd23_enable }
}

/// Decode every field this crate emits from a TRANSFER_MODE word.
pub const fn decode_transfer_mode(word: u16) -> (bool, bool, bool, bool, u16) {
    (
        word & SDHCI_TRNS_DMA != 0,
        word & SDHCI_TRNS_BLK_CNT_EN != 0,
        word & SDHCI_TRNS_READ != 0,
        word & SDHCI_TRNS_MULTI != 0,
        word & (SDHCI_TRNS_AUTO_CMD12 | SDHCI_TRNS_AUTO_CMD23),
    )
}
