// SPDX-License-Identifier: GPL-2.0-only
//! Card-state machine encoded in the native R1 STATUS response.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c` and
//! `include/linux/mmc/mmc.h`. Copyright 2006-2007 Pierre Ossman and the Linux
//! MMC authors.

pub const CURRENT_STATE_MASK: u32 = 0x0000_1e00; // include/linux/mmc/mmc.h:154
pub const CURRENT_STATE_SHIFT: u8 = 9; // include/linux/mmc/mmc.h:154
pub const READY_FOR_DATA: u32 = 1 << 8; // include/linux/mmc/mmc.h:155

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CardState { Idle = 0, Ready = 1, Ident = 2, Standby = 3, Transfer = 4, Data = 5, Receive = 6, Programming = 7, Disconnect = 8 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateDef { pub name: &'static str, pub value: u8 }
pub const CARD_STATES: [StateDef; 9] = [
    StateDef { name: "IDLE", value: 0 }, // include/linux/mmc/mmc.h:160
    StateDef { name: "READY", value: 1 }, // include/linux/mmc/mmc.h:161
    StateDef { name: "IDENT", value: 2 }, // include/linux/mmc/mmc.h:162
    StateDef { name: "STBY", value: 3 }, // include/linux/mmc/mmc.h:163
    StateDef { name: "TRAN", value: 4 }, // include/linux/mmc/mmc.h:164
    StateDef { name: "DATA", value: 5 }, // include/linux/mmc/mmc.h:165
    StateDef { name: "RCV", value: 6 }, // include/linux/mmc/mmc.h:166
    StateDef { name: "PRG", value: 7 }, // include/linux/mmc/mmc.h:167
    StateDef { name: "DIS", value: 8 }, // include/linux/mmc/mmc.h:168
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusError { ReservedCardState { value: u8, maximum_defined: u8 } }

pub fn current_state(status: u32) -> Result<CardState, StatusError> {
    let value = ((status & CURRENT_STATE_MASK) >> CURRENT_STATE_SHIFT) as u8;
    match value {
        0 => Ok(CardState::Idle), 1 => Ok(CardState::Ready), 2 => Ok(CardState::Ident),
        3 => Ok(CardState::Standby), 4 => Ok(CardState::Transfer), 5 => Ok(CardState::Data),
        6 => Ok(CardState::Receive), 7 => Ok(CardState::Programming), 8 => Ok(CardState::Disconnect),
        _ => Err(StatusError::ReservedCardState { value, maximum_defined: 8 }),
    }
}

pub fn ready_for_data(status: u32) -> bool {
    status & READY_FOR_DATA != 0 && current_state(status) == Ok(CardState::Transfer)
}
