// SPDX-License-Identifier: GPL-2.0-only
//! The transfer decisions — how deep the FIFO is, and which byte carries STOP or RESTART.
//!
//! Ported from Linux `drivers/i2c/busses/`:
//!   * `i2c_dw_set_fifo_size` (i2c-designware-common.c:788-:826)
//!   * `i2c_dw_xfer_msg` (i2c-designware-master.c:378-:460) — the restart and stop conditions
//!   * `DW_IC_FIFO_TX_FIELD` / `DW_IC_FIFO_RX_FIELD` (i2c-designware-core.h:54-:55)
//!
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.
//!
//! These are the decisions a transfer makes per byte. They are pure functions of the message list
//! and the controller's own reported geometry, which is why they can be pinned exactly here while
//! the loop that drives them cannot.

use crate::regs::bits;

/// FIFO depth from `DW_IC_COMP_PARAM_1` (i2c-designware-common.c:817-:818).
///
/// `FIELD_GET(field, param) + 1`. THE PLUS ONE IS LOAD-BEARING: the register encodes depth MINUS
/// one. Dropping it under-reports the FIFO by a byte — which is merely slow if the value is used to
/// fill, and a silent overrun if anything ever compares the other way. Linux's comment gives the
/// range the answer must fall in: "the depth could be from 2 to 256 from HW spec".
pub fn tx_fifo_depth(comp_param_1: u32) -> u32 {
    ((comp_param_1 & bits::FIFO_TX_FIELD) >> 16) + 1
}

/// RX FIFO depth — a DIFFERENT field of the same word (GENMASK(15, 8), core.h:55).
pub fn rx_fifo_depth(comp_param_1: u32) -> u32 {
    ((comp_param_1 & bits::FIFO_RX_FIELD) >> 8) + 1
}

/// One message's flags, as far as these decisions are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MsgFlags {
    /// `I2C_M_RD` — this message reads.
    pub read: bool,
    /// `I2C_M_RECV_LEN` — an SMBus block read, whose LENGTH IS NOT YET KNOWN. i2c-core sets the
    /// buffer length to 1 and the real length arrives in the first received byte.
    pub recv_len: bool,
}

/// Whether a RESTART must be issued before this message's first byte.
///
/// i2c-designware-master.c:401-:403. Linux's comment: "If both IC_EMPTYFIFO_HOLD_MASTER_EN and
/// IC_RESTART_EN are set, we must manually set restart bit between messages." So: every message
/// after the first, when RESTART_EN is configured. Not the first — a restart before any data has
/// moved is a start.
pub fn needs_restart(master_cfg: u32, msg_index: usize) -> bool {
    master_cfg & bits::CON_RESTART_EN != 0 && msg_index > 0
}

/// Whether this byte carries STOP.
///
/// i2c-designware-master.c:427-:429 — the LAST byte of the LAST message, and NOT an SMBus block
/// read.
///
/// THE `recv_len` EXCLUSION IS THE ONE A PORT DROPS. For a block read, i2c-core sets the buffer
/// length to 1 because the true length is the first byte the device sends; the transaction cannot
/// end there. Omitting the check issues STOP after that first byte and TRUNCATES EVERY BLOCK READ
/// TO ONE BYTE — silently, with no error anywhere, because from the controller's side the transfer
/// completed exactly as instructed.
pub fn carries_stop(msg_index: usize, msg_count: usize, bytes_left: usize, flags: MsgFlags) -> bool {
    msg_index + 1 == msg_count && bytes_left == 1 && !flags.recv_len
}

/// The DATA_CMD word for one byte, given the decisions above.
///
/// A read request carries no data byte — the command bit IS the request (master.c:442, written as
/// `cmd | 0x100`). A write carries the byte in bits 7:0 (:448).
pub fn data_cmd_word(byte: Option<u8>, stop: bool, restart: bool) -> u32 {
    let mut w = 0;
    if stop {
        w |= bits::DATA_CMD_STOP;
    }
    if restart {
        w |= bits::DATA_CMD_RESTART;
    }
    match byte {
        None => w | bits::DATA_CMD_READ,
        Some(b) => w | b as u32,
    }
}

/// How many bytes may be pushed now: `tx_fifo_depth - TXFLR` (i2c-designware-master.c:406-:407).
///
/// Saturating, because a controller reporting a level ABOVE its own depth is reporting nonsense and
/// the honest response is "no room", not a wrapped enormous limit.
pub fn tx_room(tx_fifo_depth: u32, txflr: u32) -> u32 {
    tx_fifo_depth.saturating_sub(txflr)
}
