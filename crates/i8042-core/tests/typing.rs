// SPDX-License-Identifier: GPL-2.0-only
//! Typing, end to end through the ported pieces: the keyboard's set-2 bytes, translated by the controller
//! to set 1 (CTR_XLATE, kept from firmware as Linux keeps it), read out of the output buffer by
//! `Controller::read_byte`, decoded by `atkbd-scancode-core` (atkbd_receive_byte, translated mode), and
//! mapped to HID keyboard usages by `hid-input-keyboard-core` (the inverse of hid-input.c hid_keyboard[]).
//! Every expected usage is a literal from the HID Usage Tables keyboard page as hid-input.c:28-42 lays it
//! out. Copyright (c) 1999-2004 Vojtech Pavlik.

mod sim;

use atkbd_scancode_core::decode::{Config, DecodeResult, Decoder, KeyState};
use hid_input_keyboard_core::keycode_to_usage;
use i8042_core::bringup::{bring_up, Options};
use i8042_core::controller::{Controller, Source};
use i8042_core::ps2::Ps2Dev;
use sim::Sim;

/// (usage, pressed) for each key event the bytes produce.
fn typed(set2: &[u8]) -> Vec<(u8, bool)> {
    let mut c = Controller::new(Sim::new());
    let mut dev = Ps2Dev::new();
    let b = bring_up(&mut c, &mut dev, Options::FULL, &mut |_, _| {}, &mut |_| {}).unwrap();
    assert!(b.ctr.translated);
    let mut dec = Decoder::new(Config { set: b.set, translated: b.ctr.translated, scroll: false }).unwrap();
    c.io.type_set2(set2);
    let mut out = Vec::new();
    while let Some((byte, src)) = c.read_byte() {
        assert_eq!(src, Source::Kbd);
        if let DecodeResult::Key(k) = dec.feed(byte).unwrap() {
            out.push((keycode_to_usage(k.keycode).expect("a keyboard-page usage"), k.state == KeyState::Make));
        }
    }
    out
}

#[test]
fn letters_press_and_release() {
    assert_eq!(typed(&[0x33, 0xf0, 0x33, 0x43, 0xf0, 0x43]), [(0x0b, true), (0x0b, false), (0x0c, true), (0x0c, false)]);
}

#[test]
fn shift_release_is_not_mistaken_for_a_keyboard_reset() {
    // Left Shift's set-1 break is 0xaa, the BAT byte: atkbd's xl_bit rule (atkbd.c:340-370) reads it as a
    // release because its make was seen.
    assert_eq!(
        typed(&[0x12, 0x1c, 0xf0, 0x1c, 0xf0, 0x12]),
        [(0xe1, true), (0x04, true), (0x04, false), (0xe1, false)]
    );
}

#[test]
fn every_modifier_left_and_right() {
    assert_eq!(
        typed(&[0x14, 0x11, 0x12, 0x59, 0xe0, 0x14, 0xe0, 0x11, 0xe0, 0x1f]),
        [(0xe0, true), (0xe2, true), (0xe1, true), (0xe5, true), (0xe4, true), (0xe6, true), (0xe3, true)]
    );
}

#[test]
fn e0_arrows_and_the_editing_keys() {
    assert_eq!(
        typed(&[0xe0, 0x75, 0xe0, 0x72, 0xe0, 0x6b, 0xe0, 0x74, 0xe0, 0x6c, 0xe0, 0x69, 0xe0, 0xf0, 0x75]),
        [(0x52, true), (0x51, true), (0x50, true), (0x4f, true), (0x4a, true), (0x4d, true), (0x52, false)]
    );
}

#[test]
fn esc_enter_backspace_space_and_digits() {
    assert_eq!(
        typed(&[0x76, 0x5a, 0x66, 0x29, 0x16]),
        [(0x29, true), (0x28, true), (0x2a, true), (0x2c, true), (0x1e, true)]
    );
}

#[test]
fn the_function_row_including_f7_s_code_past_0x7f() {
    assert_eq!(typed(&[0x05, 0x06, 0x83, 0x09]), [(0x3a, true), (0x3b, true), (0x40, true), (0x43, true)]);
}

#[test]
fn pause_is_one_press_and_one_release() {
    assert_eq!(typed(&[0xe1, 0x14, 0x77, 0xe1, 0xf0, 0x14, 0xf0, 0x77]), [(0x48, true), (0x48, false)]);
}

#[test]
fn ctrl_alt_shift_r() {
    assert_eq!(
        typed(&[0x14, 0x11, 0x12, 0x2d, 0xf0, 0x2d]),
        [(0xe0, true), (0xe2, true), (0xe1, true), (0x15, true), (0x15, false)]
    );
}
