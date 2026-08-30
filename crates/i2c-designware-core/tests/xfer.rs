// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the transfer decisions. Expected values are LINUX literals with file and line.

use i2c_designware_core::regs::bits;
use i2c_designware_core::xfer::{
    carries_stop, data_cmd_word, needs_restart, rx_fifo_depth, tx_fifo_depth, tx_room, MsgFlags,
};

const WRITE: MsgFlags = MsgFlags { read: false, recv_len: false };
const BLOCK_READ: MsgFlags = MsgFlags { read: true, recv_len: true };

/// i2c-designware-common.c:817-:818 — `FIELD_GET(field, param) + 1`, over the two DIFFERENT fields
/// at core.h:54-:55. Linux's comment bounds the answer: "the depth could be from 2 to 256".
#[test]
fn the_fifo_depths_come_from_two_different_fields_plus_one() {
    // TX in bits 23:16, RX in bits 15:8 — a word that distinguishes them.
    let param = (0x1f << 16) | (0x07 << 8);
    assert_eq!(tx_fifo_depth(param), 0x20, "0x1f + 1");
    assert_eq!(rx_fifo_depth(param), 0x08, "0x07 + 1");
    // The fields must not bleed into each other, nor pick up neighbouring bits.
    assert_eq!(tx_fifo_depth(0xffff_ffff), 256, "0xff + 1, the spec's maximum");
    assert_eq!(rx_fifo_depth(0xffff_ffff), 256);
    // A zero field is depth ONE, which is what "+ 1" means — not zero.
    assert_eq!(tx_fifo_depth(0), 1);
    assert_eq!(rx_fifo_depth(0), 1);
}

/// i2c-designware-master.c:401-:403 — a restart is needed before every message AFTER the first,
/// when RESTART_EN is configured. Not before the first: a restart before any data has moved is
/// simply a start.
#[test]
fn a_restart_precedes_every_message_but_the_first() {
    let cfg = bits::CON_RESTART_EN;
    assert!(!needs_restart(cfg, 0), "the first message starts, it does not restart");
    assert!(needs_restart(cfg, 1));
    assert!(needs_restart(cfg, 7));
    // And nothing restarts when the controller was not configured for it.
    assert!(!needs_restart(0, 1));
}

/// i2c-designware-master.c:427-:429 — STOP on the LAST byte of the LAST message.
#[test]
fn stop_rides_the_last_byte_of_the_last_message() {
    assert!(carries_stop(1, 2, 1, WRITE), "last message, last byte");
    assert!(!carries_stop(0, 2, 1, WRITE), "not the last message");
    assert!(!carries_stop(1, 2, 2, WRITE), "not the last byte");
    assert!(carries_stop(0, 1, 1, WRITE), "a single one-byte message stops");
}

/// THE EXCLUSION A PORT DROPS. For an SMBus block read, i2c-core sets the buffer length to 1
/// because the TRUE length is the first byte the device sends. Omitting `!(flags & I2C_M_RECV_LEN)`
/// issues STOP after that first byte and truncates EVERY block read to one byte — silently, with no
/// error anywhere, because from the controller's side the transfer completed as instructed.
#[test]
fn a_block_read_does_not_stop_on_its_first_byte() {
    assert!(!carries_stop(0, 1, 1, BLOCK_READ),
            "the length is not yet known; the transaction cannot end here");
    // The ONLY difference from the stopping case above is recv_len.
    assert!(carries_stop(0, 1, 1, MsgFlags { read: true, recv_len: false }));
}

/// master.c:442 (`cmd | 0x100`) and :448 (`cmd | *buf++`). A read request carries NO data byte —
/// the command bit is the request.
#[test]
fn the_command_word_carries_either_a_byte_or_a_read_request() {
    assert_eq!(data_cmd_word(Some(0xa5), false, false), 0xa5);
    assert_eq!(data_cmd_word(None, false, false), bits::DATA_CMD_READ);
    assert_eq!(data_cmd_word(None, false, false) & 0xff, 0, "a read carries no data byte");
    // The command bits sit above the data byte and compose.
    assert_eq!(
        data_cmd_word(Some(0xa5), true, true),
        bits::DATA_CMD_STOP | bits::DATA_CMD_RESTART | 0xa5
    );
    assert_eq!(
        data_cmd_word(None, true, false),
        bits::DATA_CMD_READ | bits::DATA_CMD_STOP
    );
}

/// master.c:406-:407 — `tx_limit = dev->tx_fifo_depth - flr`.
#[test]
fn the_tx_room_is_the_depth_less_the_current_level() {
    assert_eq!(tx_room(32, 0), 32, "an empty FIFO takes everything");
    assert_eq!(tx_room(32, 30), 2);
    assert_eq!(tx_room(32, 32), 0, "a full FIFO takes nothing");
    // A controller reporting a level ABOVE its own depth is reporting nonsense; the honest answer
    // is "no room". An unsaturated subtraction would wrap to about four billion and overrun.
    assert_eq!(tx_room(32, 33), 0);
    assert_eq!(tx_room(0, 1), 0);
}
