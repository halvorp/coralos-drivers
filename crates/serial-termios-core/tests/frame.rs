// SPDX-License-Identifier: GPL-2.0-only
//! Literal frame-format vectors from Linux termbits and tty frame decoding.
//!
//! Based on `drivers/char/serial.c`, by Linus Torvalds and Theodore Ts'o.
//! Copyright 1999 ARM Limited. Copyright (C) 2000-2001 Deep Blue Solutions Ltd.

use serial_termios_core::frame::{
    decode_cflag, encode_cflag, frame_size, DataBits, FlowControl, FrameFormat, Parity, StopBits,
    CMSPAR, CRTSCTS, CS5, CS6, CS7, CS8, CSIZE, CSTOPB, FRAME_CFLAG_MASK, PARENB, PARODD,
};

fn format(
    data_bits: DataBits,
    stop_bits: StopBits,
    parity: Parity,
    flow_control: FlowControl,
) -> FrameFormat {
    FrameFormat {
        data_bits,
        stop_bits,
        parity,
        flow_control,
    }
}

/// include/uapi/asm-generic/termbits.h:96-:104 and termbits-common.h:50-:51.
#[test]
fn cflag_literals_match_linux() {
    assert_eq!(CSIZE, 0x0000_0030);
    assert_eq!(
        (CS5, CS6, CS7, CS8),
        (0x0000_0000, 0x0000_0010, 0x0000_0020, 0x0000_0030)
    );
    assert_eq!(CSTOPB, 0x0000_0040);
    assert_eq!(PARENB, 0x0000_0100);
    assert_eq!(PARODD, 0x0000_0200);
    assert_eq!(CMSPAR, 0x4000_0000);
    assert_eq!(CRTSCTS, 0x8000_0000);
    assert_eq!(FRAME_CFLAG_MASK, 0xc000_0370);
}

/// tty_ioctl.c:270-:280: CS5/6/7/8 decode to 5/6/7/8.
#[test]
fn every_character_size_decodes() {
    assert_eq!(decode_cflag(0).data_bits, DataBits::Five);
    assert_eq!(decode_cflag(0x0000_0010).data_bits, DataBits::Six);
    assert_eq!(decode_cflag(0x0000_0020).data_bits, DataBits::Seven);
    assert_eq!(decode_cflag(0x0000_0030).data_bits, DataBits::Eight);
}

/// serial_core.c:2215-:2229 and termbits-common.h:50-:51.
#[test]
fn stop_parity_and_flow_combinations_decode() {
    assert_eq!(decode_cflag(0x0000_0300).parity, Parity::Odd);
    assert_eq!(decode_cflag(0x0000_0100).parity, Parity::Even);
    assert_eq!(decode_cflag(0x4000_0300).parity, Parity::Mark);
    assert_eq!(decode_cflag(0x4000_0100).parity, Parity::Space);
    assert_eq!(
        decode_cflag(0x4000_0200).parity,
        Parity::None,
        "PARENB gates PARODD and CMSPAR"
    );
    let decoded = decode_cflag(0x8000_0040);
    assert_eq!(decoded.stop_bits, StopBits::Two);
    assert_eq!(decoded.flow_control, FlowControl::RtsCts);
}

/// Linux cflag literals, termbits.h:97-:104 and termbits-common.h:50-:51.
#[test]
fn frame_formats_encode_to_literal_cflags() {
    assert_eq!(
        encode_cflag(format(
            DataBits::Eight,
            StopBits::One,
            Parity::None,
            FlowControl::None
        )),
        0x0000_0030
    );
    assert_eq!(
        encode_cflag(format(
            DataBits::Seven,
            StopBits::Two,
            Parity::Odd,
            FlowControl::RtsCts
        )),
        0x8000_0360
    );
    assert_eq!(
        encode_cflag(format(
            DataBits::Six,
            StopBits::One,
            Parity::Even,
            FlowControl::None
        )),
        0x0000_0110
    );
    assert_eq!(
        encode_cflag(format(
            DataBits::Five,
            StopBits::One,
            Parity::Mark,
            FlowControl::None
        )),
        0x4000_0300
    );
    assert_eq!(
        encode_cflag(format(
            DataBits::Eight,
            StopBits::One,
            Parity::Space,
            FlowControl::None
        )),
        0x4000_0130
    );
}

/// tty_ioctl.c:294-:305: `2 + char_size`, plus one each for CSTOPB and PARENB.
#[test]
fn frame_size_counts_start_data_parity_and_stop_bits() {
    assert_eq!(
        frame_size(format(
            DataBits::Eight,
            StopBits::One,
            Parity::None,
            FlowControl::None
        )),
        10
    );
    assert_eq!(
        frame_size(format(
            DataBits::Seven,
            StopBits::Two,
            Parity::Even,
            FlowControl::None
        )),
        11
    );
    assert_eq!(
        frame_size(format(
            DataBits::Five,
            StopBits::One,
            Parity::Mark,
            FlowControl::RtsCts
        )),
        8
    );
    assert_eq!(
        frame_size(format(
            DataBits::Six,
            StopBits::Two,
            Parity::None,
            FlowControl::None
        )),
        9
    );
}
