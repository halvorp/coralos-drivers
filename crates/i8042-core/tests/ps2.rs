// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the PS/2 command engine — drivers/input/serio/libps2.c.
//! Copyright (c) 1999-2002 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov.

mod sim;

use i8042_core::controller::Controller;
use i8042_core::ps2::{is_keyboard_id, Ps2Dev, Ps2Error};
use sim::Sim;

fn run(s: Sim, command: u16, param: &mut [u8]) -> (Result<(), Ps2Error>, Vec<u8>, Controller<Sim>) {
    let mut c = Controller::new(s);
    let mut dev = Ps2Dev::new();
    let mut strays = Vec::new();
    let r = dev.command(&mut c, param, command, &mut |b| strays.push(b));
    assert_eq!(dev.flags, 0, "flags are reset on every exit (libps2.c:372-373)");
    (r, strays, c)
}

#[test]
fn keyboard_ids_are_linux_s_six() {
    for id in [0xab, 0xac, 0x2b, 0x5d, 0x60, 0x47] {
        assert!(is_keyboard_id(id), "{id:#04x}");
    }
    for id in [0x00, 0x03, 0x04, 0xaa, 0xa5] {
        assert!(!is_keyboard_id(id), "{id:#04x}");
    }
}

#[test]
fn getid_collects_two_bytes_after_the_ack() {
    let mut p = [0xa5u8; 2];
    let (r, strays, c) = run(Sim::new(), 0x02f2, &mut p);
    assert_eq!(r, Ok(()));
    assert_eq!(p, [0xab, 0x83]);
    assert!(strays.is_empty());
    assert_eq!(c.io.kbd_log, [0xf2]);
}

#[test]
fn a_mouse_like_getid_answer_ends_the_wait_with_one_byte() {
    // libps2.c:225-241: first byte 0xaa clears the flags, and 0xaa is not a keyboard id, so cmdcnt drops
    // to 0 and the command SUCCEEDS with the second byte left as it was — atkbd_probe then refuses it.
    let mut s = Sim::new();
    s.getid_reply = vec![0xaa];
    let mut p = [0xa5u8; 2];
    let (r, _, c) = run(s, 0x02f2, &mut p);
    assert_eq!(r, Ok(()));
    assert_eq!(p, [0xaa, 0xa5]);
    assert_eq!(c.io.elapsed_us, 0, "no second-byte wait: the adjusted timeout is 0");
}

#[test]
fn a_mouse_that_does_not_ack_getid_is_taken_at_its_id() {
    // libps2.c:528-534: 0x00 while WAITID counts as the ACK and as the first reply byte.
    let mut s = Sim::new();
    s.no_keyboard = true;
    s.out.push_back((0x00, false));
    let mut p = [0xa5u8; 2];
    let (r, _, _) = run(s, 0x02f2, &mut p);
    assert_eq!(r, Ok(()));
    assert_eq!(p, [0x00, 0xa5]);
}

#[test]
fn one_nak_is_resent_and_then_acked() {
    let mut s = Sim::new();
    s.naks_before_ack = 1;
    let mut p = [0u8; 1];
    let (r, _, c) = run(s, 0x00f4, &mut p);
    assert_eq!(r, Ok(()));
    assert_eq!(c.io.kbd_log, [0xf4, 0xf4]);
}

#[test]
fn two_naks_exhaust_the_attempts() {
    let mut s = Sim::new();
    s.naks_before_ack = 2;
    let (r, _, c) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Err(Ps2Error::Nak { byte: 0xf4 })); // libps2.c:69,77-78
    assert_eq!(c.io.kbd_log, [0xf4, 0xf4]); // two attempts, libps2.c:328
    assert_eq!(Ps2Error::Nak { byte: 0 }.name(), "PS2-NAK");
}

#[test]
fn nak_then_0xfc_is_a_device_error() {
    // The NAK ends the first ACK wait (libps2.c:511-514 clears FLAG_ACK) and the byte is resent; the 0xfc
    // that follows a NAK is the device refusing it (libps2.c:516-520) -> -EPROTO (libps2.c:80-81).
    let mut s = Sim::new();
    s.naks_before_ack = 2;
    s.err_after_nak = true;
    let (r, _, c) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Err(Ps2Error::DeviceError { byte: 0xf4 }));
    assert_eq!(c.io.kbd_log, [0xf4, 0xf4]);
    assert_eq!(Ps2Error::DeviceError { byte: 0 }.name(), "PS2-DEVICE-ERROR");
}

#[test]
fn silence_is_no_ack_after_the_200_ms_ack_wait() {
    let mut s = Sim::new();
    s.no_keyboard = true;
    let (r, _, c) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Err(Ps2Error::NoAck { byte: 0xf4 })); // nak stays 1: -EIO, libps2.c:83-84
    assert_eq!(c.io.elapsed_us, 200_000);
    assert_eq!(Ps2Error::NoAck { byte: 0 }.name(), "PS2-NO-ACK");
}

#[test]
fn a_reset_wants_its_ack_within_a_second() {
    let mut s = Sim::new();
    s.no_keyboard = true;
    let (r, _, c) = run(s, 0x02ff, &mut [0u8; 2]);
    assert_eq!(r, Err(Ps2Error::NoAck { byte: 0xff }));
    assert_eq!(c.io.elapsed_us, 1_000_000); // libps2.c:326
}

#[test]
fn a_keyboard_reset_succeeds_with_the_bat_byte_alone() {
    let mut p = [0u8; 2];
    let (r, _, c) = run(Sim::new(), 0x02ff, &mut p);
    assert_eq!(r, Ok(())); // cmdcnt == 1 is allowed for RESET_BAT, libps2.c:365-366
    assert_eq!(p[0], 0xaa);
    assert_eq!(c.io.elapsed_us, 100_000, "the tail wait is cut to 100 ms (libps2.c:214-215)");
}

#[test]
fn a_reset_with_no_bat_is_a_short_reply_after_four_seconds() {
    let mut s = Sim::new();
    s.reset_reply = vec![];
    let (r, _, c) = run(s, 0x02ff, &mut [0u8; 2]);
    assert_eq!(r, Err(Ps2Error::ShortReply { command: 0x02ff, missing: 2 }));
    assert_eq!(c.io.elapsed_us, 4_000_000); // libps2.c:344
    assert_eq!(Ps2Error::ShortReply { command: 0, missing: 0 }.name(), "PS2-SHORT-REPLY");
}

#[test]
fn a_getid_with_no_reply_is_short_after_half_a_second() {
    let mut s = Sim::new();
    s.getid_reply = vec![];
    let (r, _, c) = run(s, 0x02f2, &mut [0xa5u8; 2]);
    assert_eq!(r, Err(Ps2Error::ShortReply { command: 0x02f2, missing: 2 }));
    assert_eq!(c.io.elapsed_us, 500_000);
}

#[test]
fn a_scancode_ahead_of_the_setleds_ack_goes_to_the_receive_handler() {
    // libps2.c:536-554: PASS_NOACK (SETLEDS/SETREP) hands the unexpected byte to the receive handler.
    let mut s = Sim::new();
    s.before_ack = vec![0x1e];
    let mut p = [0u8; 1];
    let (r, strays, c) = run(s, 0x10ed, &mut p);
    assert_eq!(r, Ok(()));
    assert_eq!(strays, [0x1e]);
    assert_eq!(c.io.kbd_log, [0xed, 0x00]);
    assert_eq!(c.io.leds, 0);
}

#[test]
fn a_scancode_ahead_of_another_command_s_ack_is_dropped() {
    let mut s = Sim::new();
    s.out.push_back((0x1e, false));
    let (r, strays, _) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Ok(()));
    assert!(strays.is_empty()); // no PASS_NOACK: dropped, libps2.c:553-554
}

#[test]
fn an_aux_byte_never_reaches_the_keyboard_state_machine() {
    let mut s = Sim::new();
    s.aux(0xfa);
    s.no_keyboard = true;
    let (r, _, _) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Err(Ps2Error::NoAck { byte: 0xf4 }));
}

#[test]
fn a_device_that_streams_forever_cannot_hold_the_wait_open() {
    let mut s = Sim::new();
    s.chatter = Some(0x1e);
    let (r, _, _) = run(s, 0x00f4, &mut []);
    assert_eq!(r, Err(Ps2Error::NoAck { byte: 0xf4 }));
}

#[test]
fn a_reply_longer_than_cmdbuf_or_a_short_buffer_is_refused() {
    let (r, _, _) = run(Sim::new(), 0x09f2, &mut [0u8; 9]);
    assert_eq!(r, Err(Ps2Error::BadCommand { command: 0x09f2 }));
    let (r, _, _) = run(Sim::new(), 0x10ed, &mut []);
    assert_eq!(r, Err(Ps2Error::BadCommand { command: 0x10ed }));
    assert_eq!(Ps2Error::BadCommand { command: 0 }.name(), "PS2-BAD-COMMAND");
}
