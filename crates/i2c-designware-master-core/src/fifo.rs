// SPDX-License-Identifier: GPL-2.0-only
//! Pure read/write FIFO transfer state machine.
//!
//! Ported from Linux `drivers/i2c/busses/i2c-designware-master.c:368-571,634-679`. Register bits
//! are consumed from the already-landed `i2c-designware-core`; this module performs no MMIO.
//!
//! Copyright (C) 2006 Texas Instruments.
//! Copyright (C) 2007 MontaVista Software Inc.
//! Copyright (C) 2009 Provigent Ltd.
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.

use i2c_designware_core::regs::bits;
use crate::setup::{I2C_CLIENT_PEC, I2C_M_RD, I2C_M_RECV_LEN};

/// `STATUS_*` state bits consumed by the master state machine (i2c-designware-core.h:149-152).
pub const STATUS_TABLE: [(&str, u32); 3] = [
    ("STATUS_ACTIVE", 0x1), // i2c-designware-core.h:149
    ("STATUS_WRITE_IN_PROGRESS", 0x2), // i2c-designware-core.h:150
    ("STATUS_READ_IN_PROGRESS", 0x4), // i2c-designware-core.h:151
];
pub const STATUS_ACTIVE: u32 = 0x1; // i2c-designware-core.h:149
pub const STATUS_WRITE_IN_PROGRESS: u32 = 0x2; // i2c-designware-core.h:150
pub const STATUS_READ_IN_PROGRESS: u32 = 0x4; // i2c-designware-core.h:151
/// `I2C_SMBUS_BLOCK_MAX` (include/uapi/linux/i2c.h:141).
pub const SMBUS_BLOCK_MAX: u8 = 32;

/// One DATA_CMD write produced while pumping the TX FIFO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub word: u32,
    pub message_index: usize,
    pub byte_index: usize,
}

/// Named refusal from the TX FIFO state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxRefusal {
    MessageIndexOutOfRange { index: usize, count: usize },
    WriteByteMissing { message_index: usize, byte_index: usize, available: usize },
}

/// Persisted transmit-side state corresponding to Linux's message index, buffer cursor, status,
/// and `rx_outstanding` fields (master.c:380-382,387,438-450).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxState {
    pub message_index: usize,
    pub byte_index: usize,
    pub status: u32,
    pub rx_outstanding: u32,
}

impl TxState {
    /// Start a transfer at its first message and byte (`__i2c_dw_xfer_one_part`, master.c:756-762).
    pub const fn new() -> Self {
        Self { message_index: 0, byte_index: 0, status: 0, rx_outstanding: 0 }
    }
}

impl Default for TxState { fn default() -> Self { Self::new() } }

/// Result of one FIFO-pump invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxPump {
    pub state: TxState,
    pub command_count: usize,
    pub interrupt_mask: u32,
}

/// Pump as many commands as current FIFO limits permit, mirroring `i2c_dw_xfer_msg`
/// (i2c-designware-master.c:375-485). The caller supplies message metadata and concatenated write
/// buffers; generated register values are returned in `commands`.
pub fn pump_tx(state: TxState, messages: &[crate::setup::Message], write_bytes: &[&[u8]],
               master_cfg: u32, tx_fifo_depth: u32, tx_level: u32,
               rx_fifo_depth: u32, rx_level: u32, commands: &mut [Command])
               -> Result<TxPump, TxRefusal> {
    if state.message_index > messages.len() {
        return Err(TxRefusal::MessageIndexOutOfRange {
            index: state.message_index, count: messages.len(),
        });
    }
    let mut out = state;
    let mut count = 0usize;
    let mut intr_mask = bits::INTR_MASTER_MASK;
    let mut tx_limit = tx_fifo_depth.saturating_sub(tx_level);
    let mut rx_limit = rx_fifo_depth.saturating_sub(rx_level);

    while out.message_index < messages.len() {
        let message = messages[out.message_index];
        let continuing = out.status & STATUS_WRITE_IN_PROGRESS != 0;
        if !continuing { out.byte_index = 0; }
        let mut need_restart = !continuing
            && master_cfg & bits::CON_RESTART_EN != 0 && out.message_index > 0; // master.c:400-402

        while out.byte_index < message.len && tx_limit > 0 && rx_limit > 0
              && count < commands.len() {
            if message.flags & I2C_M_RD != 0 && out.rx_outstanding >= rx_fifo_depth {
                break; // master.c:438-440
            }
            let bytes_left = message.len - out.byte_index;
            let mut word = 0;
            if out.message_index + 1 == messages.len() && bytes_left == 1
                && message.flags & I2C_M_RECV_LEN == 0 {
                word |= bits::DATA_CMD_STOP; // master.c:427-429
            }
            if need_restart {
                word |= bits::DATA_CMD_RESTART; // master.c:431-433
                need_restart = false;
            }
            if message.flags & I2C_M_RD != 0 {
                word |= bits::DATA_CMD_READ; // master.c:442-445
                rx_limit -= 1;
                out.rx_outstanding += 1;
            } else {
                let available = write_bytes.get(out.message_index).map_or(0, |b| b.len());
                let byte = write_bytes.get(out.message_index)
                    .and_then(|b| b.get(out.byte_index)).copied()
                    .ok_or(TxRefusal::WriteByteMissing {
                        message_index: out.message_index,
                        byte_index: out.byte_index,
                        available,
                    })?;
                word |= byte as u32; // master.c:447-448
            }
            commands[count] = Command {
                word, message_index: out.message_index, byte_index: out.byte_index,
            };
            count += 1;
            out.byte_index += 1;
            tx_limit -= 1;
        }

        if message.flags & I2C_M_RECV_LEN != 0 {
            out.status |= STATUS_WRITE_IN_PROGRESS;
            intr_mask &= !bits::INTR_TX_EMPTY; // master.c:463-466
            break;
        } else if out.byte_index < message.len {
            out.status |= STATUS_WRITE_IN_PROGRESS; // master.c:467-470
            break;
        } else {
            out.status &= !STATUS_WRITE_IN_PROGRESS;
            out.message_index += 1;
            out.byte_index = 0;
        }
    }
    if out.message_index == messages.len() {
        intr_mask &= !bits::INTR_TX_EMPTY; // master.c:475-480
    }
    Ok(TxPump { state: out, command_count: count, interrupt_mask: intr_mask })
}

/// Result of Linux's receive-length adjustment (master.c:488-512).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReceiveLength {
    pub length: u8,
    pub tx_bytes_left: u32,
    pub flags: u16,
    pub interrupt_mask: u32,
}

/// Validate and apply an SMBus block length byte. Invalid lengths become one exactly as Linux does
/// so one final byte with STOP can complete the transaction (master.c:543-557).
pub fn receive_length(raw: u8, flags: u16, rx_outstanding: u32,
                      interrupt_mask: u32) -> ReceiveLength {
    let valid = if raw == 0 || raw > SMBUS_BLOCK_MAX { 1 } else { raw }; // master.c:554-555
    let length = valid + if flags & I2C_CLIENT_PEC != 0 { 2 } else { 1 }; // master.c:499
    ReceiveLength {
        length,
        tx_bytes_left: (length as u32).saturating_sub(rx_outstanding), // master.c:500
        flags: flags & !I2C_M_RECV_LEN, // master.c:502
        interrupt_mask: interrupt_mask | bits::INTR_TX_EMPTY, // master.c:508-510
    }
}

/// Persisted receive-side state corresponding to Linux's read index, cursor, status, and
/// `rx_outstanding` fields (master.c:521-569).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxState {
    pub message_index: usize,
    pub byte_index: usize,
    pub remaining: usize,
    pub status: u32,
    pub rx_outstanding: u32,
}

impl RxState {
    /// Start receive processing at the first message (master.c:757-762).
    pub const fn new() -> Self {
        Self { message_index: 0, byte_index: 0, remaining: 0, status: 0, rx_outstanding: 0 }
    }
}
impl Default for RxState { fn default() -> Self { Self::new() } }

/// Named refusal from the RX FIFO state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxRefusal {
    OutputTooSmall { needed: usize, available: usize },
    OutstandingUnderflow { outstanding: u32 },
}

/// Result of one receive FIFO drain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxDrain {
    pub state: RxState,
    pub consumed: usize,
    pub receive_length: Option<ReceiveLength>,
}

/// Drain caller-supplied DATA_CMD values as `i2c_dw_read` does (master.c:515-571). The low byte of
/// each value is copied to `output`; the returned state resumes a partial message on the next call.
pub fn drain_rx(mut state: RxState, messages: &[crate::setup::Message], fifo_words: &[u32],
                output: &mut [u8], interrupt_mask: u32) -> Result<RxDrain, RxRefusal> {
    let mut consumed = 0usize;
    let mut adjusted = None;
    while state.message_index < messages.len() && consumed < fifo_words.len() {
        let message = messages[state.message_index];
        if message.flags & I2C_M_RD == 0 {
            state.message_index += 1;
            state.byte_index = 0;
            state.remaining = 0;
            continue;
        }
        if state.status & STATUS_READ_IN_PROGRESS == 0 {
            state.remaining = message.len;
            state.byte_index = 0;
        }
        while state.remaining > 0 && consumed < fifo_words.len() {
            if consumed >= output.len() {
                return Err(RxRefusal::OutputTooSmall { needed: consumed + 1, available: output.len() });
            }
            if state.rx_outstanding == 0 {
                return Err(RxRefusal::OutstandingUnderflow { outstanding: 0 });
            }
            let mut byte = (fifo_words[consumed] & bits::DATA_CMD_DAT) as u8; // master.c:541-542
            if message.flags & I2C_M_RECV_LEN != 0 && state.byte_index == 0 {
                let length = receive_length(byte, message.flags, state.rx_outstanding, interrupt_mask);
                byte = if byte == 0 || byte > SMBUS_BLOCK_MAX { 1 } else { byte };
                state.remaining = length.length as usize;
                adjusted = Some(length);
            }
            output[consumed] = byte;
            consumed += 1;
            state.byte_index += 1;
            state.remaining -= 1;
            state.rx_outstanding -= 1;
        }
        if state.remaining > 0 {
            state.status |= STATUS_READ_IN_PROGRESS; // master.c:563-567
            break;
        }
        state.status &= !STATUS_READ_IN_PROGRESS; // master.c:568-569
        state.message_index += 1;
        state.byte_index = 0;
    }
    Ok(RxDrain { state, consumed, receive_length: adjusted })
}

/// Effects of one interrupt-status sample after transfer processing (master.c:634-679).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptDecision {
    pub command_error_tx_abort: bool,
    pub status: u32,
    pub rx_outstanding: u32,
    pub interrupt_mask: Option<u32>,
    pub spurious_stop: bool,
    pub complete: bool,
}

/// Apply abort/STOP state transitions around the FIFO pumps (master.c:634-679). RX_FULL and
/// TX_EMPTY data movement remains explicit through [`drain_rx`] and [`pump_tx`].
pub fn process_interrupt(stat: u32, status: u32, rx_outstanding: u32,
                         message_error: bool) -> InterruptDecision {
    let mut next_status = status;
    let mut outstanding = rx_outstanding;
    let aborted = stat & bits::INTR_TX_ABRT != 0;
    let mut mask = None;
    if aborted {
        next_status &= !(STATUS_ACTIVE | STATUS_WRITE_IN_PROGRESS | STATUS_READ_IN_PROGRESS);
        outstanding = 0;
        mask = Some(0); // master.c:636-646
    }
    let spurious = !aborted && stat & bits::INTR_STOP_DET != 0
        && next_status & (STATUS_READ_IN_PROGRESS | STATUS_WRITE_IN_PROGRESS) != 0;
    if spurious { outstanding = 0; } // master.c:655-660
    let error = message_error || spurious;
    let complete = ((stat & (bits::INTR_TX_ABRT | bits::INTR_STOP_DET) != 0) || error)
        && outstanding == 0; // master.c:669-672
    InterruptDecision {
        command_error_tx_abort: aborted, status: next_status, rx_outstanding: outstanding,
        interrupt_mask: mask, spurious_stop: spurious, complete,
    }
}
