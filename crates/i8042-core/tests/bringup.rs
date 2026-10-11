// SPDX-License-Identifier: GPL-2.0-only
//! The whole bring-up as a scripted byte exchange: Linux's order, every step reported, every refusal
//! naming its step.
//! Copyright (c) 1999-2004 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov.

mod sim;

use i8042_core::bringup::{bring_up, BringUp, Failed, Options, Step, Verdict, Why, STEPS};
use i8042_core::controller::{Controller, Fail};
use i8042_core::ps2::Ps2Dev;
use sim::Sim;

fn go(s: Sim, opts: Options) -> (Result<BringUp, Failed>, Vec<(Step, Verdict)>, Controller<Sim>) {
    let mut c = Controller::new(s);
    let mut dev = Ps2Dev::new();
    let mut log = Vec::new();
    let r = bring_up(&mut c, &mut dev, opts, &mut |s, v| log.push((s, v)), &mut |_| {});
    (r, log, c)
}

#[test]
fn step_names() {
    let names: Vec<&str> = STEPS.iter().map(|s| s.name()).collect();
    assert_eq!(
        names,
        ["FLUSH", "SELFTEST", "CTR", "KBD-TEST", "AUX-OFF", "PORT-ENABLE", "KBD-RESET", "GETID", "SCANSET", "LEDS",
         "TYPEMATIC", "SCAN-ENABLE"]
    );
}

#[test]
fn options_are_linux_x86_defaults_and_the_full_set() {
    assert_eq!(
        Options::LINUX_X86,
        Options { selftest: false, kbd_test: false, kbd_reset: false, repeat_delay_ms: 250, repeat_period_ms: 33 }
    );
    assert_eq!(
        Options::FULL,
        Options { selftest: true, kbd_test: true, kbd_reset: true, repeat_delay_ms: 250, repeat_period_ms: 33 }
    );
}

#[test]
fn the_full_sequence_on_a_healthy_controller() {
    let (r, log, c) = go(Sim::new(), Options::FULL);
    let b = r.unwrap();
    assert_eq!(c.io.cmd_log, [0xaa, 0x20, 0x20, 0x60, 0xab, 0x60, 0x60]);
    assert_eq!(c.io.kbd_log, [0xff, 0xf2, 0xf5, 0xed, 0x00, 0xf3, 0x00, 0xf4]);
    assert_eq!(c.io.ctr, 0x65); // translation kept, KBD on + its interrupt, AUX off
    assert!(c.io.scanning);
    assert_eq!((b.set, b.extra, b.repeat, b.probe.id), (2, false, 0x00, 0xab83));
    assert!(b.ctr.translated);
    assert_eq!(
        log,
        [
            (Step::Flush, Verdict::Ok { detail: 0 }),
            (Step::SelfTest, Verdict::Ok { detail: 0 }),
            (Step::Ctr, Verdict::Ok { detail: 0x45 }),
            (Step::KbdTest, Verdict::Ok { detail: 0 }),
            (Step::AuxOff, Verdict::Ok { detail: 0x74 }),
            (Step::PortEnable, Verdict::Ok { detail: 0x65 }),
            (Step::KbdReset, Verdict::Ok { detail: 0xaa }),
            (Step::GetId, Verdict::Ok { detail: 0xab83 }),
            (Step::ScanSet, Verdict::Ok { detail: 2 }),
            (Step::Leds, Verdict::Ok { detail: 0 }),
            (Step::Typematic, Verdict::Ok { detail: 0 }),
            (Step::ScanEnable, Verdict::Ok { detail: 0 }),
        ]
    );
}

#[test]
fn linux_x86_defaults_skip_the_tests_and_the_reset() {
    let (r, log, c) = go(Sim::new(), Options::LINUX_X86);
    r.unwrap();
    assert_eq!(c.io.cmd_log, [0x20, 0x20, 0x60, 0x60, 0x60]);
    assert_eq!(c.io.kbd_log, [0xf2, 0xf5, 0xed, 0x00, 0xf3, 0x00, 0xf4]);
    let skipped: Vec<Step> = log.iter().filter(|(_, v)| *v == Verdict::Skipped).map(|(s, _)| *s).collect();
    assert_eq!(skipped, [Step::SelfTest, Step::KbdTest, Step::KbdReset]);
}

#[test]
fn tolerated_answers_are_named_and_the_bring_up_continues() {
    let mut s = Sim::new();
    s.selftest_answer = Some(0x00);
    s.kbd_test_answer = 0x02;
    s.reset_reply = vec![];
    let (r, log, _) = go(s, Options::FULL);
    r.unwrap();
    assert_eq!(log[1], (Step::SelfTest, Verdict::Tolerated { answer: 0x00, why: "SELFTEST-ANSWER-NOT-0x55" }));
    assert_eq!(log[3], (Step::KbdTest, Verdict::Tolerated { answer: 0x02, why: "KBD-INTERFACE-ERROR" }));
    assert_eq!(log[6], (Step::KbdReset, Verdict::Tolerated { answer: 0, why: "PS2-SHORT-REPLY" }));
}

fn fails_at(s: Sim, step: Step, why: Why) {
    let (r, log, _) = go(s, Options::FULL);
    assert_eq!(r, Err(Failed { step, why }));
    assert_eq!(log.last(), Some(&(step, Verdict::Failed { why: why.name() })));
}

#[test]
fn every_refusal_names_its_step() {
    let mut s = Sim::new();
    s.absent = true;
    fails_at(s, Step::Flush, Why::Controller(Fail::NoController { flushed: 16 }));

    let mut s = Sim::new();
    s.selftest_answer = None;
    fails_at(s, Step::SelfTest, Why::Controller(Fail::WaitReadTimeout));

    let mut s = Sim::new();
    s.ctr_unstable = true;
    fails_at(s, Step::Ctr, Why::Controller(Fail::CtrUnstable { last: [0x45, 0x44] }));

    let mut s = Sim::new();
    s.no_keyboard = true;
    s.reset_reply = vec![];
    use i8042_core::{atkbd::KbdFail, ps2::Ps2Error};
    fails_at(s, Step::GetId, Why::Keyboard(KbdFail::Command(Ps2Error::NoAck { byte: 0xed })));

    let mut s = Sim::new();
    s.getid_reply = vec![0x03];
    fails_at(s, Step::GetId, Why::Keyboard(KbdFail::NotAKeyboard { id0: 0x03 }));
}
