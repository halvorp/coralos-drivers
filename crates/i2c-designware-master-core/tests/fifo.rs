// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for the master read/write FIFO state machine.
//!
//! Copyright (C) 2006 Texas Instruments; Copyright (C) 2007 MontaVista Software Inc.;
//! Copyright (C) 2009 Provigent Ltd.

use i2c_designware_master_core::{fifo::*, setup::*};

fn msg(flags: u16, len: usize) -> Message { Message { addr: 0x50, flags, len } }

/// i2c-designware-core.h:149-151 defines exactly three state bits consumed here, with names frozen
/// independently of the production table.
#[test]
fn all_three_transfer_status_names_and_literals_are_pinned() {
    assert_eq!(STATUS_TABLE.len(), 3);
    assert_eq!(STATUS_TABLE, [
        ("STATUS_ACTIVE", 0x1),
        ("STATUS_WRITE_IN_PROGRESS", 0x2),
        ("STATUS_READ_IN_PROGRESS", 0x4),
    ]);
    assert_eq!(SMBUS_BLOCK_MAX, 32); // include/uapi/linux/i2c.h:141
}

/// master.c:400-450. First message writes bytes; second message gets RESTART on its first read and
/// STOP on its last. Linux DATA_CMD literals are READ 0x100, STOP 0x200, RESTART 0x400.
#[test]
fn tx_pump_encodes_write_read_restart_and_stop() {
    let messages = [msg(0, 2), msg(I2C_M_RD, 2)];
    let bytes: [&[u8]; 2] = [&[0xa5, 0x5a], &[]];
    let mut commands = [Command { word: 0, message_index: 0, byte_index: 0 }; 4];
    let got = pump_tx(TxState::new(), &messages, &bytes, 0x20, 8, 0, 8, 0, &mut commands).unwrap();
    assert_eq!(got.command_count, 4);
    assert_eq!(commands.map(|c| c.word), [0x0a5, 0x05a, 0x500, 0x300]);
    assert_eq!(got.state.message_index, 2);
    assert_eq!(got.state.rx_outstanding, 2);
    assert_eq!(got.interrupt_mask, 0x244); // core.h:117; MASTER_MASK 0x254 minus TX_EMPTY 0x10
}

/// master.c:405-470. A full FIFO persists WRITE_IN_PROGRESS; the next invocation resumes at the
/// saved byte rather than restarting the message.
#[test]
fn tx_state_resumes_a_message_longer_than_fifo_room() {
    let messages = [msg(0, 3)];
    let bytes: [&[u8]; 1] = [&[0x11, 0x22, 0x33]];
    let mut first_cmds = [Command { word: 0, message_index: 0, byte_index: 0 }; 2];
    let first = pump_tx(TxState::new(), &messages, &bytes, 0x20, 2, 0, 2, 0, &mut first_cmds).unwrap();
    assert_eq!(first_cmds.map(|c| c.word), [0x11, 0x22]);
    assert_eq!(first.state, TxState {
        message_index: 0, byte_index: 2, status: 0x2, rx_outstanding: 0,
    });
    let mut last = [Command { word: 0, message_index: 0, byte_index: 0 }; 1];
    let done = pump_tx(first.state, &messages, &bytes, 0x20, 2, 0, 2, 0, &mut last).unwrap();
    assert_eq!(last[0].word, 0x233, "last data byte 0x33 carries STOP 0x200");
    assert_eq!(done.state.status & 0x2, 0);
}

/// master.c:422-466. A block read's initial length request must NOT carry STOP, and TX_EMPTY is
/// masked while waiting for the length byte.
#[test]
fn unknown_block_length_pauses_transmit_without_stop() {
    let messages = [msg(I2C_M_RD | I2C_M_RECV_LEN, 1)];
    let bytes: [&[u8]; 1] = [&[]];
    let mut commands = [Command { word: 0, message_index: 0, byte_index: 0 }; 1];
    let got = pump_tx(TxState::new(), &messages, &bytes, 0x20, 8, 0, 8, 0, &mut commands).unwrap();
    assert_eq!(commands[0].word, 0x100, "READ only; no literal STOP 0x200");
    assert_eq!(got.state.status, 0x2);
    assert_eq!(got.interrupt_mask, 0x244);
}

/// Named TX refusals carry the precise missing message byte and available length.
#[test]
fn missing_write_data_is_a_named_refusal() {
    let messages = [msg(0, 2)];
    let bytes: [&[u8]; 1] = [&[0x11]];
    let mut commands = [Command { word: 0, message_index: 0, byte_index: 0 }; 2];
    assert_eq!(pump_tx(TxState::new(), &messages, &bytes, 0, 2, 0, 2, 0, &mut commands),
               Err(TxRefusal::WriteByteMissing {
                   message_index: 0, byte_index: 1, available: 1,
               }));
    let bad = TxState { message_index: 2, ..TxState::new() };
    assert_eq!(pump_tx(bad, &messages, &bytes, 0, 2, 0, 2, 0, &mut commands),
               Err(TxRefusal::MessageIndexOutOfRange { index: 2, count: 1 }));
}

/// master.c:499-510,554-557. PEC adds two, no PEC adds one, invalid 0 or >32 is forced to one,
/// RECV_LEN is removed, and literal TX_EMPTY 0x10 is restored.
#[test]
fn receive_length_matches_linux_for_valid_invalid_and_pec_lengths() {
    assert_eq!(receive_length(5, I2C_M_RECV_LEN, 1, 0x244), ReceiveLength {
        length: 6, tx_bytes_left: 5, flags: 0, interrupt_mask: 0x254,
    });
    assert_eq!(receive_length(5, I2C_M_RECV_LEN | I2C_CLIENT_PEC, 2, 0x244), ReceiveLength {
        length: 7, tx_bytes_left: 5, flags: I2C_CLIENT_PEC, interrupt_mask: 0x254,
    });
    assert_eq!(receive_length(0, I2C_M_RECV_LEN, 1, 0).length, 2);
    assert_eq!(receive_length(32, I2C_M_RECV_LEN, 1, 0).length, 33,
               "the Linux maximum is valid, not an invalid-length sentinel");
    assert_eq!(receive_length(33, I2C_M_RECV_LEN, 1, 0).length, 2);
}

/// master.c:521-569. Writes are skipped; DATA_CMD is masked to 0xff; a partial read persists and
/// resumes without overwriting the first bytes.
#[test]
fn rx_drain_skips_writes_masks_data_and_resumes() {
    let messages = [msg(0, 1), msg(I2C_M_RD, 3)];
    let mut output = [0u8; 3];
    let state = RxState { rx_outstanding: 3, ..RxState::new() };
    let first = drain_rx(state, &messages, &[0x1aa, 0x2bb], &mut output[..2], 0).unwrap();
    assert_eq!(&output[..2], &[0xaa, 0xbb]);
    assert_eq!(first.state.status & 0x4, 0x4);
    assert_eq!(first.state.remaining, 1);
    let second = drain_rx(first.state, &messages, &[0x3cc], &mut output[2..], 0).unwrap();
    assert_eq!(output, [0xaa, 0xbb, 0xcc]);
    assert_eq!(second.state.message_index, 2);
    assert_eq!(second.state.status & 0x4, 0);
}

/// RX refusals name the required and available output sizes, or the exhausted outstanding count.
#[test]
fn rx_capacity_failures_are_named() {
    let messages = [msg(I2C_M_RD, 1)];
    let state = RxState { rx_outstanding: 1, ..RxState::new() };
    assert_eq!(drain_rx(state, &messages, &[0xaa], &mut [], 0),
               Err(RxRefusal::OutputTooSmall { needed: 1, available: 0 }));
    assert_eq!(drain_rx(RxState::new(), &messages, &[0xaa], &mut [0], 0),
               Err(RxRefusal::OutstandingUnderflow { outstanding: 0 }));
}

/// master.c:636-672. TX_ABRT clears all three status bits and outstanding requests; a STOP during
/// a partial transfer is named spurious and completion waits for outstanding reads to reach zero.
#[test]
fn interrupt_state_handles_abort_spurious_stop_and_completion() {
    let abort = process_interrupt(0x40, 0x7, 3, false); // core.h:108 TX_ABRT
    assert_eq!(abort.status, 0);
    assert_eq!(abort.rx_outstanding, 0);
    assert_eq!(abort.interrupt_mask, Some(0));
    assert!(abort.command_error_tx_abort && abort.complete);

    let stop = process_interrupt(0x200, 0x2, 2, false); // core.h:111 STOP_DET
    assert!(stop.spurious_stop);
    assert_eq!(stop.rx_outstanding, 0);
    assert!(stop.complete);
    let pending = process_interrupt(0x200, 0, 1, false);
    assert!(!pending.complete, "STOP does not complete with a read request outstanding");
}

