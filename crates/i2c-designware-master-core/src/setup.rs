// SPDX-License-Identifier: GPL-2.0-only
//! Master configuration, target setup, message flags, validation, and STOP partitioning.
//!
//! Ported from Linux `drivers/i2c/busses/i2c-designware-master.c:188-233,819-950`, with I2C flag
//! literals from `include/uapi/linux/i2c.h:77-85`.
//!
//! Copyright (C) 2006 Texas Instruments.
//! Copyright (C) 2007 MontaVista Software Inc.
//! Copyright (C) 2009 Provigent Ltd.
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.

use i2c_designware_core::regs::bits;

/// Linux message flags used by this state machine (include/uapi/linux/i2c.h:77-85).
pub const FLAG_TABLE: [(&str, u16); 4] = [
    ("I2C_M_RD", 0x0001), // include/uapi/linux/i2c.h:77
    ("I2C_M_TEN", 0x0010), // include/uapi/linux/i2c.h:78
    ("I2C_M_RECV_LEN", 0x0400), // include/uapi/linux/i2c.h:80
    ("I2C_M_STOP", 0x8000), // include/uapi/linux/i2c.h:85
];

pub const I2C_M_RD: u16 = 0x0001; // include/uapi/linux/i2c.h:77
pub const I2C_M_TEN: u16 = 0x0010; // include/uapi/linux/i2c.h:78
pub const I2C_M_RECV_LEN: u16 = 0x0400; // include/uapi/linux/i2c.h:80
pub const I2C_M_STOP: u16 = 0x8000; // include/uapi/linux/i2c.h:85
/// Linux client PEC flag used by receive-length adjustment (include/linux/i2c.h:333).
pub const I2C_CLIENT_PEC: u16 = 0x0004;

/// Message metadata needed before FIFO bytes are supplied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Message {
    pub addr: u16,
    pub flags: u16,
    pub len: usize,
}

/// Values programmed by `i2c_dw_xfer_init` for one target (master.c:199-219).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSetup {
    pub con_10bit_value: u32,
    pub tar_value: u32,
}

/// Encode the target address and ten-bit addressing controls (i2c-designware-master.c:199-219).
pub fn target_setup(message: Message) -> TargetSetup {
    if message.flags & I2C_M_TEN != 0 {
        TargetSetup {
            con_10bit_value: bits::CON_10BITADDR_MASTER,
            tar_value: message.addr as u32 | bits::TAR_10BITADDR_MASTER,
        }
    } else {
        TargetSetup { con_10bit_value: 0, tar_value: message.addr as u32 }
    }
}

/// Initial master control word selected by `i2c_dw_configure_master` (master.c:934-948).
pub fn master_config(bus_freq_hz: u32) -> u32 {
    let speed = match bus_freq_hz {
        crate::timing::STANDARD_FREQ_HZ => bits::CON_SPEED_STD,
        crate::timing::HIGH_SPEED_FREQ_HZ => bits::CON_SPEED_HIGH,
        _ => bits::CON_SPEED_FAST,
    };
    bits::CON_MASTER | bits::CON_SLAVE_DISABLE | bits::CON_RESTART_EN | speed
}

/// Why a message cannot follow its predecessor in one controller transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRefusal {
    InvalidTargetAddress { previous: u16, current: u16 },
    CannotEmitRestart { previous_read: bool, current_read: bool },
}

/// Validate one message against its predecessor (i2c-designware-master.c:826-855).
pub fn validate_message(messages: &[Message], index: usize,
                        emptyfifo_hold_master: bool) -> Result<(), MessageRefusal> {
    if index == 0 { return Ok(()); }
    let previous = messages[index - 1];
    let current = messages[index];
    if previous.addr != current.addr {
        return Err(MessageRefusal::InvalidTargetAddress {
            previous: previous.addr,
            current: current.addr,
        });
    }
    let previous_read = previous.flags & I2C_M_RD != 0;
    let current_read = current.flags & I2C_M_RD != 0;
    if !emptyfifo_hold_master && previous_read == current_read {
        return Err(MessageRefusal::CannotEmitRestart { previous_read, current_read });
    }
    Ok(())
}

/// Number of messages in the next transaction part, through the first explicit STOP
/// (i2c-designware-master.c:875-895). Every included message is validated first.
pub fn next_part_len(messages: &[Message], emptyfifo_hold_master: bool)
    -> Result<usize, MessageRefusal> {
    for index in 0..messages.len() {
        validate_message(messages, index, emptyfifo_hold_master)?;
        if messages[index].flags & I2C_M_STOP != 0 || index + 1 == messages.len() {
            return Ok(index + 1);
        }
    }
    Ok(0)
}
