// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the AT keyboard's commands — drivers/input/keyboard/atkbd.c.
//! Copyright (c) 1999-2002 Vojtech Pavlik.

mod sim;

use i8042_core::atkbd::{self, KbdFail, Probe};
use i8042_core::controller::Controller;
use i8042_core::ps2::{Ps2Dev, Ps2Error};
use sim::Sim;

fn parts(s: Sim) -> (Controller<Sim>, Ps2Dev) {
    (Controller::new(s), Ps2Dev::new())
}

#[test]
fn the_typematic_byte_walks_linux_s_tables() {
    assert_eq!(atkbd::repeat_param(250, 33), (0x00, 250, 33));
    assert_eq!(atkbd::repeat_param(500, 100), (0x2c, 500, 100)); // i=12, j=1
    assert_eq!(atkbd::repeat_param(1000, 500), (0x7f, 1000, 500)); // the last entries
    assert_eq!(atkbd::repeat_param(9999, 9999), (0x7f, 1000, 500)); // the walk stops at the end
    assert_eq!(atkbd::repeat_param(260, 34), (0x21, 500, 37)); // rounds UP to the next entry
}

#[test]
fn probe_without_reset_is_getid_then_deactivate() {
    let (mut c, mut d) = parts(Sim::new());
    let p = atkbd::probe(&mut d, &mut c, false, true, &mut |_| {}).unwrap();
    assert_eq!(p, Probe { id: 0xab83, reset: None, deactivate: Ok(()) });
    assert_eq!(c.io.kbd_log, [0xf2, 0xf5]);
    assert!(!c.io.scanning);
}

#[test]
fn probe_with_reset_names_the_bat_byte() {
    let (mut c, mut d) = parts(Sim::new());
    let p = atkbd::probe(&mut d, &mut c, true, true, &mut |_| {}).unwrap();
    assert_eq!(p.reset, Some(Ok(0xaa)));
    assert_eq!(c.io.kbd_log, [0xff, 0xf2, 0xf5]);
}

#[test]
fn a_failed_reset_is_carried_through_like_linux_warns() {
    let mut s = Sim::new();
    s.reset_reply = vec![];
    let (mut c, mut d) = parts(s);
    let p = atkbd::probe(&mut d, &mut c, true, true, &mut |_| {}).unwrap();
    assert_eq!(p.reset, Some(Err(Ps2Error::ShortReply { command: 0x02ff, missing: 2 })));
    assert_eq!(p.id, 0xab83);
}

#[test]
fn a_keyboard_without_getid_falls_back_to_setleds() {
    let mut s = Sim::new();
    s.getid_reply = vec![];
    let (mut c, mut d) = parts(s);
    let p = atkbd::probe(&mut d, &mut c, false, true, &mut |_| {}).unwrap();
    assert_eq!(p.id, 0xabba); // atkbd.c:848
    assert_eq!(c.io.kbd_log, [0xf2, 0xed, 0x00]);
}

#[test]
fn a_mouse_is_not_a_keyboard() {
    let mut s = Sim::new();
    s.getid_reply = vec![0x00];
    let (mut c, mut d) = parts(s);
    assert_eq!(
        atkbd::probe(&mut d, &mut c, false, true, &mut |_| {}),
        Err(KbdFail::NotAKeyboard { id0: 0x00 })
    );
    assert_eq!(KbdFail::NotAKeyboard { id0: 0 }.name(), "NOT-A-KEYBOARD");
}

#[test]
fn no_keyboard_at_all_is_named_by_the_setleds_fallback() {
    let mut s = Sim::new();
    s.no_keyboard = true;
    let (mut c, mut d) = parts(s);
    assert_eq!(
        atkbd::probe(&mut d, &mut c, false, true, &mut |_| {}),
        Err(KbdFail::Command(Ps2Error::NoAck { byte: 0xed }))
    );
}

#[test]
fn an_ncd_keyboard_is_refused_behind_translation() {
    let mut s = Sim::new();
    s.getid_reply = vec![0xac, 0xa1];
    let (mut c, mut d) = parts(s);
    assert_eq!(
        atkbd::probe(&mut d, &mut c, false, true, &mut |_| {}),
        Err(KbdFail::NcdOnTranslatingController)
    );
    assert_eq!(KbdFail::NcdOnTranslatingController.name(), "NCD-ON-TRANSLATING-CONTROLLER");
}

#[test]
fn select_set_sends_nothing_for_set_2() {
    let (mut c, mut d) = parts(Sim::new());
    assert_eq!(atkbd::select_set(&mut d, &mut c, 0xab83, true, 2, false, &mut |_| {}), Ok((2, false)));
    assert_eq!(atkbd::select_set(&mut d, &mut c, 0xab83, false, 2, false, &mut |_| {}), Ok((2, false)));
    assert!(c.io.kbd_log.is_empty());
}

#[test]
fn select_set_branches() {
    let (mut c, mut d) = parts(Sim::new());
    assert_eq!(atkbd::select_set(&mut d, &mut c, 0xaca1, false, 2, false, &mut |_| {}), Ok((3, false)));
    assert_eq!(c.io.kbd_log, [0xf0, 0x03]);
    assert_eq!(c.io.set, 3);
    let (mut c, mut d) = parts(Sim::new());
    assert_eq!(atkbd::select_set(&mut d, &mut c, 0xab83, false, 2, true, &mut |_| {}), Ok((2, true)));
    assert_eq!(c.io.kbd_log, [0xea, 0x71]);
    let (mut c, mut d) = parts(Sim::new());
    assert_eq!(
        atkbd::select_set(&mut d, &mut c, 0xab83, false, 3, false, &mut |_| {}),
        Err(KbdFail::Set3NotPorted)
    );
    assert_eq!(KbdFail::Set3NotPorted.name(), "SET3-NOT-PORTED");
}

#[test]
fn leds_typematic_and_enable_reach_the_keyboard() {
    let (mut c, mut d) = parts(Sim::new());
    c.io.scanning = false;
    atkbd::set_leds(&mut d, &mut c, atkbd::LED_CAPS | 0x80, &mut |_| {}).unwrap();
    assert_eq!(c.io.leds, 0x04, "only the three plain LED bits");
    assert_eq!(atkbd::set_repeat_rate(&mut d, &mut c, 500, 100, &mut |_| {}), Ok(0x2c));
    assert_eq!(c.io.repeat, 0x2c);
    atkbd::activate(&mut d, &mut c, &mut |_| {}).unwrap();
    assert!(c.io.scanning);
    assert_eq!(c.io.kbd_log, [0xed, 0x04, 0xf3, 0x2c, 0xf4]);
}
