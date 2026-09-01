// SPDX-License-Identifier: GPL-2.0-only
//! SEND_STATUS, SEND_EXT_CSD, SEND_OP_COND, and SET_RELATIVE_ADDR descriptors.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c`: `__mmc_send_status`
//! (mmc_ops.c:68-:91), `mmc_send_op_cond` and its callback (mmc_ops.c:191-:254),
//! `mmc_set_relative_addr` (mmc_ops.c:256-:266), and `mmc_get_ext_csd`
//! (mmc_ops.c:370-:396). Protocol command numbers are from
//! `include/linux/mmc/mmc.h:32-:44`.
//!
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.

/// MMC command type: addressed command (`MMC_CMD_AC`).
pub const CMD_AC: u32 = 0 << 5; // include/linux/mmc/core.h:42
/// MMC command type: addressed data-transfer command (`MMC_CMD_ADTC`).
pub const CMD_ADTC: u32 = 1 << 5; // include/linux/mmc/core.h:43
/// MMC command type: broadcast command with response (`MMC_CMD_BCR`).
pub const CMD_BCR: u32 = 3 << 5; // include/linux/mmc/core.h:45
/// SPI one-byte status response flag (`MMC_RSP_SPI_S1`).
pub const RSP_SPI_S1: u32 = 1 << 7; // include/linux/mmc/core.h:49
/// SPI second status byte (`MMC_RSP_SPI_S2`).
pub const RSP_SPI_S2: u32 = 1 << 8; // include/linux/mmc/core.h:50
/// SPI four-byte payload (`MMC_RSP_SPI_B4`).
pub const RSP_SPI_B4: u32 = 1 << 9; // include/linux/mmc/core.h:51
/// SPI busy indication (`MMC_RSP_SPI_BUSY`).
pub const RSP_SPI_BUSY: u32 = 1 << 10; // include/linux/mmc/core.h:52

pub const RSP_SPI_R1: u32 = RSP_SPI_S1; // include/linux/mmc/core.h:75
pub const RSP_SPI_R1B: u32 = RSP_SPI_S1 | RSP_SPI_BUSY; // include/linux/mmc/core.h:76
pub const RSP_SPI_R2: u32 = RSP_SPI_S1 | RSP_SPI_S2; // include/linux/mmc/core.h:77
pub const RSP_SPI_R3: u32 = RSP_SPI_S1 | RSP_SPI_B4; // include/linux/mmc/core.h:78

pub const SEND_OP_COND: u32 = 1; // include/linux/mmc/mmc.h:32
pub const SET_RELATIVE_ADDR: u32 = 3; // include/linux/mmc/mmc.h:34
pub const SWITCH: u32 = 6; // include/linux/mmc/mmc.h:37
pub const SEND_EXT_CSD: u32 = 8; // include/linux/mmc/mmc.h:39
pub const SEND_STATUS: u32 = 13; // include/linux/mmc/mmc.h:44

/// One pure command descriptor for the caller's request layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub opcode: u32,
    pub argument: u32,
    pub flags: u32,
}

/// The command family ported by this module, frozen for count/name reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationDef {
    pub name: &'static str,
    pub opcode: u32,
}

pub const OPERATIONS: [OperationDef; 5] = [
    OperationDef {
        name: "SEND_OP_COND",
        opcode: SEND_OP_COND,
    }, // include/linux/mmc/mmc.h:32
    OperationDef {
        name: "SET_RELATIVE_ADDR",
        opcode: SET_RELATIVE_ADDR,
    }, // include/linux/mmc/mmc.h:34
    OperationDef {
        name: "SWITCH",
        opcode: SWITCH,
    }, // include/linux/mmc/mmc.h:37
    OperationDef {
        name: "SEND_EXT_CSD",
        opcode: SEND_EXT_CSD,
    }, // include/linux/mmc/mmc.h:39
    OperationDef {
        name: "SEND_STATUS",
        opcode: SEND_STATUS,
    }, // include/linux/mmc/mmc.h:44
];

/// CMD13. Native mode supplies RCA in bits 31:16; SPI mode supplies zero.
///
/// mmc_ops.c:73-:77.
pub fn send_status(rca: u16, spi: bool) -> Command {
    Command {
        opcode: SEND_STATUS,
        argument: if spi { 0 } else { u32::from(rca) << 16 },
        flags: RSP_SPI_R2 | mmc_core_cmd::response::R1_FLAGS | CMD_AC,
    }
}

/// CMD8 SEND_EXT_CSD, a one-block ADTC read with a zero argument.
///
/// mmc_ops.c:388-:389 and mmc_ops.c:303-:317.
pub fn send_ext_csd() -> Command {
    Command {
        opcode: SEND_EXT_CSD,
        argument: 0,
        flags: RSP_SPI_R1 | mmc_core_cmd::response::R1_FLAGS | CMD_ADTC,
    }
}

/// CMD1 SEND_OP_COND. SPI uses argument zero; native mode supplies the OCR.
///
/// mmc_ops.c:240-:243.
pub fn send_op_cond(ocr: u32, spi: bool) -> Command {
    Command {
        opcode: SEND_OP_COND,
        argument: if spi { 0 } else { ocr },
        flags: RSP_SPI_R1 | mmc_core_cmd::response::R3_FLAGS | CMD_BCR,
    }
}

/// Whether another CMD1 is required from its response.
///
/// SPI leaves busy when `R1_SPI_IDLE` clears (mmc_ops.c:202-:206); native mode
/// leaves busy when `MMC_CARD_BUSY` is set (mmc_ops.c:207-:211).
pub fn op_cond_busy(response: u32, spi: bool) -> bool {
    if spi {
        response & 1 != 0 // R1_SPI_IDLE, include/linux/mmc/mmc.h:182
    } else {
        response & mmc_core_cmd::response::R3_CARD_BUSY == 0
    }
}

/// CMD1 argument for the next probe.
///
/// During a native zero-OCR inquiry Linux feeds the response back and sets bit
/// 30, keeping the card in idle while it is busy (mmc_ops.c:220-:222). All
/// other cases retain the previous command argument.
pub fn next_op_cond_argument(requested_ocr: u32, response: u32, spi: bool) -> u32 {
    if requested_ocr == 0 && !spi {
        response | (1 << 30) // mmc_ops.c:220-:222
    } else if spi {
        0 // mmc_ops.c:242
    } else {
        requested_ocr
    }
}

/// CMD3 SET_RELATIVE_ADDR; RCA occupies bits 31:16.
///
/// mmc_ops.c:261-:263.
pub fn set_relative_addr(rca: u16) -> Command {
    Command {
        opcode: SET_RELATIVE_ADDR,
        argument: u32::from(rca) << 16,
        flags: mmc_core_cmd::response::R1_FLAGS | CMD_AC,
    }
}
