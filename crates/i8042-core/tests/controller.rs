// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the controller half — drivers/input/serio/i8042.c.
//! Copyright (c) 1999-2004 Vojtech Pavlik.

mod sim;

use i8042_core::controller::{Controller, CtrFacts, Fail, Source, TestVerdict};
use sim::Sim;

fn ctl(s: Sim) -> Controller<Sim> {
    Controller::new(s)
}

#[test]
fn an_absent_controller_is_refused_after_sixteen_flushed_bytes() {
    let mut s = Sim::new();
    s.absent = true;
    let mut c = ctl(s);
    assert_eq!(c.controller_check(), Err(Fail::NoController { flushed: 16 })); // i8042.c:264-265
    assert_eq!(Fail::NoController { flushed: 16 }.name(), "NO-CONTROLLER");
}

#[test]
fn the_flush_counts_what_it_discards() {
    let mut s = Sim::new();
    s.out.extend([(0x1e, false), (0x9e, false), (0x08, true)]);
    let mut c = ctl(s);
    assert_eq!(c.flush(), Ok(3));
    assert_eq!(c.io.elapsed_us, 150); // udelay(50) per byte, i8042.c:267
}

#[test]
fn a_stuck_input_buffer_times_out_after_ten_thousand_polls() {
    let mut s = Sim::new();
    s.ibf_stuck = true;
    let mut c = ctl(s);
    assert_eq!(c.wait_write(), Err(Fail::WaitWriteTimeout));
    assert_eq!(c.io.elapsed_us, 10000 * 50); // I8042_CTL_TIMEOUT x udelay(50) = 500 ms
    assert_eq!(Fail::WaitWriteTimeout.name(), "WAIT-WRITE-TIMEOUT");
}

#[test]
fn an_empty_output_buffer_times_out_after_ten_thousand_polls() {
    let mut c = ctl(Sim::new());
    assert_eq!(c.wait_read(), Err(Fail::WaitReadTimeout));
    assert_eq!(c.io.elapsed_us, 500_000);
    assert_eq!(Fail::WaitReadTimeout.name(), "WAIT-READ-TIMEOUT");
}

#[test]
fn a_command_writes_its_byte_then_its_parameters_then_reads_its_replies() {
    let mut c = ctl(Sim::new());
    let mut p = [0u8; 1];
    c.command(&mut p, 0x0120).unwrap(); // RCTR: one reply
    assert_eq!(p[0], 0x45);
    let mut p = [0x47u8];
    c.command(&mut p, 0x1060).unwrap(); // WCTR: one parameter
    assert_eq!(c.io.ctr, 0x47);
    assert_eq!(c.io.cmd_log, [0x20, 0x60]);
}

#[test]
fn the_self_test_passes_on_0x55() {
    let mut c = ctl(Sim::new());
    assert_eq!(c.selftest(), Ok(TestVerdict::Ok));
    assert_eq!(c.io.cmd_log, [0xaa]);
}

#[test]
fn a_wrong_self_test_answer_is_tried_six_times_then_tolerated_by_name() {
    let mut s = Sim::new();
    s.selftest_answer = Some(0x01);
    let mut c = ctl(s);
    assert_eq!(c.selftest(), Ok(TestVerdict::Tolerated { answer: 0x01 }));
    assert_eq!(c.io.cmd_log, [0xaa; 6]); // do { } while (i++ < 5), i8042.c:963
    assert_eq!(c.io.elapsed_us, 6 * 50_000); // msleep(50) after each, i8042.c:962
}

#[test]
fn a_silent_self_test_is_a_refusal() {
    let mut s = Sim::new();
    s.selftest_answer = None;
    let mut c = ctl(s);
    assert_eq!(c.selftest(), Err(Fail::WaitReadTimeout)); // i8042.c:952-955
}

#[test]
fn the_interface_test_reads_the_documented_answers() {
    for (answer, want) in [
        (0x00, TestVerdict::Ok),
        (0xfa, TestVerdict::Ok),
        (0x01, TestVerdict::Tolerated { answer: 0x01 }),
        (0x03, TestVerdict::Tolerated { answer: 0x03 }),
        (0xff, TestVerdict::Tolerated { answer: 0xff }),
    ] {
        let mut s = Sim::new();
        s.kbd_test_answer = answer;
        let mut c = ctl(s);
        assert_eq!(c.kbd_test(), Ok(want), "answer {answer:#04x}");
        assert_eq!(c.io.cmd_log, [0xab]);
    }
}

#[test]
fn init_parks_the_keyboard_port_and_keeps_the_firmware_translation() {
    let mut c = ctl(Sim::new());
    let facts = c.init().unwrap();
    assert_eq!(facts, CtrFacts { initial_ctr: 0x45, translated: true, keylock: false });
    assert_eq!(c.ctr, 0x54); // 0x45 | KBDDIS(0x10), & !KBDINT(0x01)
    assert_eq!(c.io.ctr, 0x54);
    assert_eq!(c.io.cmd_log, [0x20, 0x20, 0x60]);
}

#[test]
fn init_on_an_untranslated_controller_says_so_and_sees_the_keylock() {
    let mut s = Sim::new();
    s.ctr = 0x05;
    s.keylock_active = true;
    let mut c = ctl(s);
    assert_eq!(c.init(), Ok(CtrFacts { initial_ctr: 0x05, translated: false, keylock: true }));
    assert_eq!(c.io.ctr, 0x14);
}

#[test]
fn an_unstable_ctr_is_refused_after_ten_reads() {
    let mut s = Sim::new();
    s.ctr_unstable = true;
    let mut c = ctl(s);
    assert_eq!(c.init(), Err(Fail::CtrUnstable { last: [0x45, 0x44] }));
    assert_eq!(c.io.cmd_log, [0x20; 10]); // i8042.c:996
    assert_eq!(c.io.elapsed_us, 9 * 50); // udelay(50) before every read but the first, i8042.c:1001-1002
    assert_eq!(Fail::CtrUnstable { last: [0, 0] }.name(), "CTR-UNSTABLE");
}

#[test]
fn aux_off_then_keyboard_port_on() {
    let mut c = ctl(Sim::new());
    c.init().unwrap();
    c.disable_aux_port().unwrap();
    assert_eq!(c.io.ctr, 0x74); // AUXDIS(0x20) set, AUXINT(0x02) clear
    c.enable_kbd_port().unwrap();
    assert_eq!(c.io.ctr, 0x65); // KBDDIS(0x10) clear, KBDINT(0x01) set
}

#[test]
fn a_failed_port_enable_restores_the_image() {
    let mut c = ctl(Sim::new());
    c.init().unwrap();
    c.io.ibf_stuck = true;
    assert_eq!(c.enable_kbd_port(), Err(Fail::WaitWriteTimeout));
    assert_eq!(c.ctr, 0x54); // i8042.c:626-627
}

#[test]
fn a_byte_says_which_port_it_came_from() {
    let mut s = Sim::new();
    s.aux(0x08);
    s.type_set2(&[0x1c]);
    let mut c = ctl(s);
    assert_eq!(c.read_byte(), Some((0x08, Source::Aux)));
    assert_eq!(c.read_byte(), Some((0x1e, Source::Kbd)));
    assert_eq!(c.read_byte(), None);
}
