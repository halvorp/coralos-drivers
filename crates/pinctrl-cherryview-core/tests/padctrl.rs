// SPDX-License-Identifier: GPL-2.0-only
//! PADCTRL vectors from Linux `drivers/pinctrl/intel/pinctrl-cherryview.c`.
//! Copyright (C) 2014-2020 Intel Corporation; Mika Westerberg, Ning Li, Alan Cox.

use pinctrl_cherryview_core::padctrl::*;

#[test]
fn mux_gpio_direction_and_value_vectors_match_linux() {
    // PMODE mode 3 at bits 19:16, GPIOEN cleared, TXENABLE inversion bit 5; :691-703.
    assert_eq!(
        encode_mux(0xffff_ffff, 0xffff_ffff, 3, true),
        Ok((0xfff3_7fff, 0xffff_ff2f))
    );
    assert_eq!(
        encode_mux(0, 0, 16, false),
        Err(EncodeError::MuxModeOutOfRange {
            mode: 16,
            maximum: 15
        })
    );
    // Hi-Z (3 << 8) becomes GPI (2 << 8), then GPIOEN bit 15 is set; :770-778.
    assert_eq!(enable_gpio(0x300), 0x8200);
    assert_eq!(encode_direction(0xffff_ffff, Direction::Input), 0xffff_faff); // :807-811
    assert_eq!(
        encode_direction(0xffff_ffff, Direction::Output),
        0xffff_f9ff
    );
    assert_eq!(decode_direction(0x100), Direction::Output); // :1136-1142
    assert_eq!(decode_direction(0x300), Direction::Input);
    assert_eq!(encode_output_value(0, true), 0x2); // :1118-1121
    assert_eq!(encode_output_value(0xffff_ffff, false), 0xffff_fffd);
    assert!(decode_gpio_value(0x102)); // GPO reads TX bit, :1101-1106
    assert!(!decode_gpio_value(0x101));
    assert!(decode_gpio_value(0x201)); // GPI reads RX bit
}

#[test]
fn all_linux_pull_strengths_and_named_refusals_are_pinned() {
    // TERM_UP bit 23; TERM encoding in bits 22:20, pinctrl-cherryview.c:925-960.
    assert_eq!(encode_pull(0, Pull::Disabled), Ok(0x0000_0000));
    assert_eq!(encode_pull(0, Pull::Up(1_000)), Ok(0x00c0_0000));
    assert_eq!(encode_pull(0, Pull::Up(5_000)), Ok(0x00a0_0000));
    assert_eq!(encode_pull(0, Pull::Up(20_000)), Ok(0x0090_0000));
    assert_eq!(encode_pull(0, Pull::Down(5_000)), Ok(0x0020_0000));
    assert_eq!(encode_pull(0, Pull::Down(20_000)), Ok(0x0010_0000));
    assert_eq!(
        encode_pull(0, Pull::Up(10_000)),
        Err(EncodeError::UnsupportedPullUpOhms { ohms: 10_000 })
    );
    assert_eq!(
        encode_pull(0, Pull::Down(1_000)),
        Err(EncodeError::UnsupportedPullDownOhms { ohms: 1_000 })
    );
    assert_eq!(decode_pull(0x00c0_0000), DecodedPull::Up(1_000)); // :853-862
    assert_eq!(decode_pull(0x0010_0000), DecodedPull::Down(20_000)); // :871-877
    assert_eq!(
        decode_pull(0x0030_0000),
        DecodedPull::Unknown {
            up: false,
            encoding: 3
        }
    );
}

#[test]
fn lock_and_drive_have_vectors() {
    assert!(is_locked(0x8000_0000)); // CFGLOCK bit 31, :61 and :609-612
    assert!(!is_locked(0x7fff_ffff));
    assert_eq!(encode_drive(0, Drive::OpenDrain), 0x8); // ODEN bit 3, :981-984
    assert_eq!(encode_drive(0xffff_ffff, Drive::PushPull), 0xffff_fff7);
    assert_eq!(decode_drive(0x8), Drive::OpenDrain); // :893-900
    assert_eq!(decode_drive(0), Drive::PushPull);
}
