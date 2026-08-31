// SPDX-License-Identifier: GPL-2.0-only
//! FIFO and automatic-flow-control vectors from Linux `serial_reg.h:50-:87`, `:132`, selected by
//! `8250_lpss.c:343`.
//!
//! Copyright 2016 Intel Corporation; copyright 1992, 1994 Theodore Ts'o.

use serial_8250_lpss_core::fifo::{
    encode_fcr, encode_mcr_auto_flow, FifoControl, RxTrigger, RX_TRIGGER_STATES,
};

const BASE: FifoControl = FifoControl {
    enabled: false,
    clear_receiver: false,
    clear_transmitter: false,
    dma_mode: false,
    rx_trigger: RxTrigger::One,
};

/// The expected names and values are literal, not generated from the production table.
const LINUX_TRIGGER_NAMES: [&str; 4] = ["TRIGGER_1", "TRIGGER_4", "TRIGGER_8", "TRIGGER_14"];

#[test]
fn all_four_linux_trigger_states_are_present_by_name() {
    assert_eq!(RX_TRIGGER_STATES.len(), 4); // UART_FCR_R_TRIG_MAX_STATE=4, serial_reg.h:103
    let names: Vec<&str> = RX_TRIGGER_STATES.iter().map(|entry| entry.0).collect();
    assert_eq!(names, LINUX_TRIGGER_NAMES); // serial_reg.h:84-:87
}

#[test]
fn every_receive_trigger_encodes_to_its_linux_literal() {
    assert_eq!(
        encode_fcr(FifoControl {
            rx_trigger: RxTrigger::One,
            ..BASE
        }),
        0x00
    ); // serial_reg.h:84
    assert_eq!(
        encode_fcr(FifoControl {
            rx_trigger: RxTrigger::Four,
            ..BASE
        }),
        0x40
    ); // serial_reg.h:85
    assert_eq!(
        encode_fcr(FifoControl {
            rx_trigger: RxTrigger::Eight,
            ..BASE
        }),
        0x80
    ); // serial_reg.h:86
    assert_eq!(
        encode_fcr(FifoControl {
            rx_trigger: RxTrigger::Fourteen,
            ..BASE
        }),
        0xc0
    ); // serial_reg.h:87
}

#[test]
fn fifo_control_fields_compose_without_overlap() {
    assert_eq!(
        encode_fcr(FifoControl {
            enabled: true,
            clear_receiver: true,
            clear_transmitter: true,
            dma_mode: true,
            rx_trigger: RxTrigger::Fourteen,
        }),
        0xcf
    ); // serial_reg.h:51-:54 and :87: 0x01|0x02|0x04|0x08|0xc0
}

#[test]
fn automatic_flow_control_sets_and_clears_only_afe() {
    assert_eq!(encode_mcr_auto_flow(0x0b, true), 0x2b); // UART_MCR_AFE=0x20, serial_reg.h:132
    assert_eq!(encode_mcr_auto_flow(0x2b, false), 0x0b);
    assert_eq!(encode_mcr_auto_flow(0xff, false), 0xdf);
}
