// SPDX-License-Identifier: GPL-2.0-only
//! CMD6 SWITCH argument encoding, target EXT_CSD indices, and busy response policy.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c`: `mmc_prepare_busy_cmd`
//! (mmc_ops.c:563-:586), `__mmc_switch` (mmc_ops.c:603-:665), and its local
//! EXT_CSD targets (mmc_ops.c:999-:1001, :1024-:1026, :1061-:1062), with
//! literals from `include/linux/mmc/mmc.h:256-:271, :310, :345-:347, :429-:432`.
//!
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.

use crate::ops::{Command, CMD_AC, RSP_SPI_R1, RSP_SPI_R1B, SWITCH};

/// An MMC SWITCH access mode in argument bits 25:24.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AccessMode {
    CommandSet = 0x00, // include/linux/mmc/mmc.h:429
    SetBits = 0x01,    // include/linux/mmc/mmc.h:430
    ClearBits = 0x02,  // include/linux/mmc/mmc.h:431
    WriteByte = 0x03,  // include/linux/mmc/mmc.h:432
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessModeDef {
    pub name: &'static str,
    pub value: u8,
}

pub const ACCESS_MODES: [AccessModeDef; 4] = [
    AccessModeDef {
        name: "CMD_SET",
        value: 0x00,
    }, // include/linux/mmc/mmc.h:429
    AccessModeDef {
        name: "SET_BITS",
        value: 0x01,
    }, // include/linux/mmc/mmc.h:430
    AccessModeDef {
        name: "CLEAR_BITS",
        value: 0x02,
    }, // include/linux/mmc/mmc.h:431
    AccessModeDef {
        name: "WRITE_BYTE",
        value: 0x03,
    }, // include/linux/mmc/mmc.h:432
];

/// Command-set selector in argument bits 7:0.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CommandSet {
    Normal = 1 << 0,                  // include/linux/mmc/mmc.h:345
    Secure = 1 << 1,                  // include/linux/mmc/mmc.h:346
    ContentProtectionSecure = 1 << 2, // include/linux/mmc/mmc.h:347
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandSetDef {
    pub name: &'static str,
    pub value: u8,
}

pub const COMMAND_SETS: [CommandSetDef; 3] = [
    CommandSetDef {
        name: "NORMAL",
        value: 1,
    }, // include/linux/mmc/mmc.h:345
    CommandSetDef {
        name: "SECURE",
        value: 2,
    }, // include/linux/mmc/mmc.h:346
    CommandSetDef {
        name: "CPSECURE",
        value: 4,
    }, // include/linux/mmc/mmc.h:347
];

/// EXT_CSD byte indices written by SWITCH paths in mmc_ops.c.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtCsdIndex {
    CmdqModeEn = 15,     // include/linux/mmc/mmc.h:256; mmc_ops.c:1024
    BkopsStart = 164,    // include/linux/mmc/mmc.h:269; mmc_ops.c:999-:1000
    SanitizeStart = 165, // include/linux/mmc/mmc.h:270; mmc_ops.c:1061
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtCsdTargetDef {
    pub name: &'static str,
    pub index: u8,
}

pub const EXT_CSD_TARGETS: [ExtCsdTargetDef; 3] = [
    ExtCsdTargetDef {
        name: "CMDQ_MODE_EN",
        index: 15,
    }, // include/linux/mmc/mmc.h:256
    ExtCsdTargetDef {
        name: "BKOPS_START",
        index: 164,
    }, // include/linux/mmc/mmc.h:269
    ExtCsdTargetDef {
        name: "SANITIZE_START",
        index: 165,
    }, // include/linux/mmc/mmc.h:270
];

/// EXT_CSD byte containing the card-supplied generic CMD6 timeout multiplier.
pub const GENERIC_CMD6_TIME_INDEX: usize = 248; // include/linux/mmc/mmc.h:310
/// One raw GENERIC_CMD6_TIME unit is 10 milliseconds.
pub const GENERIC_CMD6_TIME_UNIT_MS: u32 = 10; // mmc.c:601-:602

/// Encode `[access:2][index:8][value:8][cmd_set:3]` as Linux does for CMD6.
///
/// mmc_ops.c:621-:624 uses WRITE_BYTE; exposing access mode keeps all four
/// protocol-defined modes mechanically testable.
pub fn encode_argument(access: AccessMode, index: u8, value: u8, command_set: u8) -> u32 {
    (u32::from(access as u8) << 24)
        | (u32::from(index) << 16)
        | (u32::from(value) << 8)
        | u32::from(command_set)
}

/// Convert EXT_CSD[GENERIC_CMD6_TIME] into the timeout consumed by __mmc_switch.
///
/// Linux stores `10 * ext_csd[248]` in milliseconds (mmc.c:597-:602), then a
/// zero caller timeout selects that card-derived value (mmc_ops.c:614-:618).
pub fn generic_cmd6_timeout_ms(raw_generic_cmd6_time: u8) -> u32 {
    GENERIC_CMD6_TIME_UNIT_MS * u32::from(raw_generic_cmd6_time)
}

/// Select an explicit field timeout, or the card-derived generic timeout when
/// the caller passes zero (mmc_ops.c:614-:618).
pub fn effective_timeout_ms(requested_timeout_ms: u32, raw_generic_cmd6_time: u8) -> u32 {
    if requested_timeout_ms == 0 {
        generic_cmd6_timeout_ms(raw_generic_cmd6_time)
    } else {
        requested_timeout_ms
    }
}

/// Host properties that decide whether CMD6 may use its documented R1B.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyHost {
    pub needs_r1b: bool,
    pub max_busy_timeout_ms: u32,
}

/// Result of Linux's `mmc_prepare_busy_cmd` policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusyResponse {
    pub flags: u32,
    pub busy_timeout_ms: u32,
    pub uses_r1b: bool,
}

/// Select R1B unless a host with a finite hardware-busy limit cannot cover the
/// operation and does not require R1B (mmc_ops.c:569-:585).
pub fn prepare_busy_response(host: BusyHost, timeout_ms: u32) -> BusyResponse {
    if !host.needs_r1b && host.max_busy_timeout_ms != 0 && timeout_ms > host.max_busy_timeout_ms {
        BusyResponse {
            flags: CMD_AC | RSP_SPI_R1 | mmc_core_cmd::response::R1_FLAGS,
            busy_timeout_ms: 0,
            uses_r1b: false,
        }
    } else {
        BusyResponse {
            flags: CMD_AC
                | RSP_SPI_R1B
                | mmc_core_cmd::response::R1_FLAGS
                | mmc_core_cmd::response::RSP_BUSY,
            busy_timeout_ms: timeout_ms,
            uses_r1b: true,
        }
    }
}

/// Build the CMD6 request. CMD6 is documented as R1B (mmc.h:37); the host
/// policy can deliberately downgrade to R1 only so software can poll CMD13.
pub fn switch_command(
    access: AccessMode,
    index: ExtCsdIndex,
    value: u8,
    command_set: CommandSet,
    timeout_ms: u32,
    host: BusyHost,
) -> (Command, BusyResponse) {
    let response = prepare_busy_response(host, timeout_ms);
    (
        Command {
            opcode: SWITCH,
            argument: encode_argument(access, index as u8, value, command_set as u8),
            flags: response.flags,
        },
        response,
    )
}
