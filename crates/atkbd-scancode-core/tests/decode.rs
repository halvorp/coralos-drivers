// SPDX-License-Identifier: GPL-2.0-only
//! Literal make/break, E0, E1 pause, and print-screen vectors from Linux atkbd tables/state.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux atkbd authors.

use atkbd_scancode_core::decode::*;

fn raw_set2() -> Decoder {
    Decoder::new(Config { set: 2, translated: false, scroll: false }).unwrap()
}

#[test]
fn constructor_names_unsupported_set_refusal() {
    assert_eq!(
        Decoder::new(Config { set: 1, translated: false, scroll: false }),
        Err(DecodeError::UnsupportedScancodeSet { set: 1, supported: [2, 3] })
    );
}

#[test]
fn set_two_make_and_f0_break_decode() {
    let mut decoder = raw_set2();
    assert_eq!(decoder.feed(0x1c), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0x1c, keycode: 30, state: KeyState::Make,
    }))); // atkbd.c:88
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix)); // atkbd.c:157,484-486
    assert_eq!(decoder.feed(0x1c), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0x1c, keycode: 30, state: KeyState::Break,
    })));
}

#[test]
fn e0_extended_make_and_break_decode() {
    let mut decoder = raw_set2();
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.emulation_depth(), 1);
    assert_eq!(decoder.feed(0x75), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xf5, keycode: 103, state: KeyState::Make,
    }))); // set2[0xf5]=103, atkbd.c:103
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x75), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xf5, keycode: 103, state: KeyState::Break,
    })));
}

#[test]
fn e1_pause_sequence_is_consumed_then_maps_pause() {
    let mut decoder = raw_set2();
    // Set-2 Pause: E1 14 77 E1 F0 14 F0 77. Linux's emul=2 consumes the first byte after E1.
    assert_eq!(decoder.feed(0xe1), Ok(DecodeResult::Prefix)); // atkbd.c:481-483
    assert_eq!(decoder.feed(0x14), Ok(DecodeResult::Prefix)); // atkbd.c:504-505
    assert_eq!(decoder.feed(0x77), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xf7, keycode: 119, state: KeyState::Make,
    }))); // E1 compatibility index 0xf7; set2[0xf7]=119, atkbd.c:103
    assert_eq!(decoder.feed(0xe1), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x14), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x77), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xf7, keycode: 119, state: KeyState::Break,
    })));
}

#[test]
fn print_screen_multibyte_sequence_preserves_linux_compatibility_behavior() {
    let mut decoder = raw_set2();
    // Set-2 Print Screen make: E0 12 E0 7C. The fake-shift E0 12 is KEY_NULL (255).
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x12), Ok(DecodeResult::Null { scancode: 0x92, state: KeyState::Make })); // atkbd.c:97
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x7c), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xfc, keycode: 99, state: KeyState::Make,
    }))); // atkbd.c:103

    // Set-2 Print Screen break: E0 F0 7C E0 F0 12.
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x7c), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0xfc, keycode: 99, state: KeyState::Break,
    })));
    assert_eq!(decoder.feed(0xe0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0xf0), Ok(DecodeResult::Prefix));
    assert_eq!(decoder.feed(0x12), Ok(DecodeResult::Null { scancode: 0x92, state: KeyState::Break }));
}

#[test]
fn protocol_results_unknown_and_reset_are_explicit() {
    let mut decoder = raw_set2();
    assert_eq!(decoder.feed(0xfa), Ok(DecodeResult::ProtocolResponse { name: "ACK", byte: 0xfa }));
    assert_eq!(decoder.feed(0xfe), Ok(DecodeResult::ProtocolResponse { name: "NAK", byte: 0xfe }));
    assert_eq!(decoder.feed(0xff), Ok(DecodeResult::TooManyKeys));
    assert_eq!(decoder.feed(0xaa), Ok(DecodeResult::BasicAssuranceTest));
    assert_eq!(decoder.feed(0x00), Ok(DecodeResult::Unknown { scancode: 0, state: KeyState::Make }));
    decoder.feed(0xe1).unwrap();
    decoder.feed(0xf0).unwrap();
    assert_eq!((decoder.emulation_depth(), decoder.release_pending()), (2, true));
    decoder.reset();
    assert_eq!((decoder.emulation_depth(), decoder.release_pending(), decoder.xlate_bits()), (0, false, 0));

    let mut translated = Decoder::new(Config { set: 2, translated: true, scroll: false }).unwrap();
    translated.feed(0x7a).unwrap(); // low-seven-bit make partner of ACK, atkbd.c:372-378
    assert_eq!(translated.xlate_bits(), 0x04); // ACK is xl_table index 2, atkbd.c:341
    translated.reset();
    assert_eq!(translated.xlate_bits(), 0, "reset must clear translated response-pair state");
}

#[test]
fn translated_and_set_three_paths_have_vectors() {
    let mut translated = Decoder::new(Config { set: 2, translated: true, scroll: false }).unwrap();
    assert_eq!(translated.feed(0x1e), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0x1e, keycode: 30, state: KeyState::Make,
    }))); // UNXLATE[0x1e]=0x1c, atkbd.c:127; set2[0x1c]=30, atkbd.c:88
    assert_eq!(translated.feed(0x9e), Ok(DecodeResult::Key(KeyEvent {
        scancode: 0x1e, keycode: 30, state: KeyState::Break,
    })));

    let mut set3 = Decoder::new(Config { set: 3, translated: false, scroll: false }).unwrap();
    assert_eq!(set3.feed(0x08), Ok(DecodeResult::Key(KeyEvent {
        scancode: 8, keycode: 1, state: KeyState::Make,
    }))); // atkbd.c:111
}

#[test]
fn forced_release_protocol_key_predicate_has_both_linux_literals() {
    assert!(is_forced_release_protocol_key(0xf2)); // HANGEUL, atkbd.c:1159-1164
    assert!(is_forced_release_protocol_key(0xf1)); // HANJA, atkbd.c:1166-1170
    assert!(!is_forced_release_protocol_key(0xfa));
}
