// SPDX-License-Identifier: GPL-2.0-only
//! The bring-up, in Linux's order, every step NAMED.
//!
//! `i8042_init` → `i8042_controller_check` (i8042.c:1615) → `i8042_probe` (i8042.c:1537-1572): the
//! controller self-test when `i8042.reset` asks for it, `i8042_controller_init`, the AUX port (here
//! disabled: a keyboard-only host is `i8042.noaux`), `i8042_setup_kbd` → `i8042_enable_kbd_port`; then
//! `atkbd_connect`'s `atkbd_probe` + `atkbd_select_set` (atkbd.c:1318-1323) and the LED / typematic /
//! enable commands in `atkbd_reconnect`'s explicit order (atkbd.c:1389-1404).
//!
//! The host passes a `report` callback that is called once per step with its verdict, so its log has a
//! line per step; a refusal returns [`Failed`] naming the step.
//! Copyright (c) 1999-2004 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov.

use crate::atkbd::{self, KbdFail, Probe, INPUT_DEFAULT_DELAY_MS, INPUT_DEFAULT_PERIOD_MS};
use crate::controller::{Controller, CtrFacts, Fail, TestVerdict};
use crate::ps2::Ps2Dev;
use crate::Port;

/// Every step, in the order [`bring_up`] runs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Flush,
    SelfTest,
    Ctr,
    KbdTest,
    AuxOff,
    PortEnable,
    KbdReset,
    GetId,
    ScanSet,
    Leds,
    Typematic,
    ScanEnable,
}

pub const STEPS: [Step; 12] = [
    Step::Flush, Step::SelfTest, Step::Ctr, Step::KbdTest, Step::AuxOff, Step::PortEnable,
    Step::KbdReset, Step::GetId, Step::ScanSet, Step::Leds, Step::Typematic, Step::ScanEnable,
];

impl Step {
    pub const fn name(self) -> &'static str {
        match self {
            Step::Flush => "FLUSH",
            Step::SelfTest => "SELFTEST",
            Step::Ctr => "CTR",
            Step::KbdTest => "KBD-TEST",
            Step::AuxOff => "AUX-OFF",
            Step::PortEnable => "PORT-ENABLE",
            Step::KbdReset => "KBD-RESET",
            Step::GetId => "GETID",
            Step::ScanSet => "SCANSET",
            Step::Leds => "LEDS",
            Step::Typematic => "TYPEMATIC",
            Step::ScanEnable => "SCAN-ENABLE",
        }
    }
}

/// One step's verdict, as reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Done; `detail` is the step's one number (bytes flushed, the CTR, the id, the set, the typematic byte,
    /// the reset's BAT byte; 0 where there is none).
    Ok { detail: u16 },
    /// A wrong answer Linux carries on through, named (a test's answer, a failed reset, a failed deactivate).
    Tolerated { answer: u8, why: &'static str },
    /// Not asked for by [`Options`].
    Skipped,
    /// The refusal [`bring_up`] returns.
    Failed { why: &'static str },
}

/// Which optional steps run, and the typematic asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// `i8042.reset` = always (i8042.c:1541-1545). Linux x86's default is `I8042_RESET_ON_S2RAM`
    /// (i8042.c:57), which skips it at probe.
    pub selftest: bool,
    /// The keyboard interface test (KBD_TEST). Linux never runs it.
    pub kbd_test: bool,
    /// `atkbd.reset` (atkbd.c:819-823). FALSE by default on x86 (atkbd.c:40-41).
    pub kbd_reset: bool,
    pub repeat_delay_ms: u16,
    pub repeat_period_ms: u16,
}

impl Options {
    /// What Linux does on x86 with no module parameters.
    pub const LINUX_X86: Options = Options {
        selftest: false,
        kbd_test: false,
        kbd_reset: false,
        repeat_delay_ms: INPUT_DEFAULT_DELAY_MS,
        repeat_period_ms: INPUT_DEFAULT_PERIOD_MS,
    };
    /// Every test and the keyboard reset: `i8042.reset=1 atkbd.reset=1` plus the interface test.
    pub const FULL: Options = Options { selftest: true, kbd_test: true, kbd_reset: true, ..Options::LINUX_X86 };
}

/// Why a step refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Controller(Fail),
    Keyboard(KbdFail),
}

impl Why {
    pub const fn name(self) -> &'static str {
        match self {
            Why::Controller(f) => f.name(),
            Why::Keyboard(k) => k.name(),
        }
    }
}

/// A refusal: the step and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failed {
    pub step: Step,
    pub why: Why,
}

/// What a completed bring-up established.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BringUp {
    pub flushed: usize,
    pub selftest: Option<TestVerdict>,
    pub ctr: CtrFacts,
    pub kbd_test: Option<TestVerdict>,
    pub probe: Probe,
    /// The scancode set the decoder speaks; translated when `ctr.translated`.
    pub set: u8,
    pub extra: bool,
    pub repeat: u8,
}

fn fail(step: Step, why: Why, report: &mut dyn FnMut(Step, Verdict)) -> Failed {
    report(step, Verdict::Failed { why: why.name() });
    Failed { step, why }
}

fn test_verdict(v: TestVerdict, why: &'static str) -> Verdict {
    match v {
        TestVerdict::Ok => Verdict::Ok { detail: 0 },
        TestVerdict::Tolerated { answer } => Verdict::Tolerated { answer, why },
    }
}

/// Run the whole sequence. `rx` receives any scancode the keyboard sends while a command is in flight.
pub fn bring_up<P: Port>(
    c: &mut Controller<P>,
    dev: &mut Ps2Dev,
    opts: Options,
    report: &mut dyn FnMut(Step, Verdict),
    rx: &mut dyn FnMut(u8),
) -> Result<BringUp, Failed> {
    let flushed = c.controller_check().map_err(|e| fail(Step::Flush, Why::Controller(e), report))?;
    report(Step::Flush, Verdict::Ok { detail: flushed as u16 });

    let selftest = if opts.selftest {
        let v = c.selftest().map_err(|e| fail(Step::SelfTest, Why::Controller(e), report))?;
        report(Step::SelfTest, test_verdict(v, "SELFTEST-ANSWER-NOT-0x55"));
        Some(v)
    } else {
        report(Step::SelfTest, Verdict::Skipped);
        None
    };

    let ctr = c.init().map_err(|e| fail(Step::Ctr, Why::Controller(e), report))?;
    report(Step::Ctr, Verdict::Ok { detail: ctr.initial_ctr as u16 });

    let kbd_test = if opts.kbd_test {
        let v = c.kbd_test().map_err(|e| fail(Step::KbdTest, Why::Controller(e), report))?;
        report(Step::KbdTest, test_verdict(v, "KBD-INTERFACE-ERROR"));
        Some(v)
    } else {
        report(Step::KbdTest, Verdict::Skipped);
        None
    };

    c.disable_aux_port().map_err(|e| fail(Step::AuxOff, Why::Controller(e), report))?;
    report(Step::AuxOff, Verdict::Ok { detail: c.ctr as u16 });
    c.enable_kbd_port().map_err(|e| fail(Step::PortEnable, Why::Controller(e), report))?;
    report(Step::PortEnable, Verdict::Ok { detail: c.ctr as u16 });

    let probe = atkbd::probe(dev, c, opts.kbd_reset, ctr.translated, rx)
        .map_err(|e| fail(Step::GetId, Why::Keyboard(e), report))?;
    report(Step::KbdReset, match probe.reset {
        None => Verdict::Skipped,
        Some(Ok(bat)) => Verdict::Ok { detail: bat as u16 },
        Some(Err(e)) => Verdict::Tolerated { answer: 0, why: e.name() },
    });
    report(Step::GetId, match probe.deactivate {
        Ok(()) => Verdict::Ok { detail: probe.id },
        Err(e) => Verdict::Tolerated { answer: 0, why: e.name() },
    });

    let (set, extra) = atkbd::select_set(dev, c, probe.id, ctr.translated, 2, false, rx)
        .map_err(|e| fail(Step::ScanSet, Why::Keyboard(e), report))?;
    report(Step::ScanSet, Verdict::Ok { detail: set as u16 });

    atkbd::set_leds(dev, c, 0, rx).map_err(|e| fail(Step::Leds, Why::Keyboard(e), report))?;
    report(Step::Leds, Verdict::Ok { detail: 0 });
    let repeat = atkbd::set_repeat_rate(dev, c, opts.repeat_delay_ms, opts.repeat_period_ms, rx)
        .map_err(|e| fail(Step::Typematic, Why::Keyboard(e), report))?;
    report(Step::Typematic, Verdict::Ok { detail: repeat as u16 });
    atkbd::activate(dev, c, rx).map_err(|e| fail(Step::ScanEnable, Why::Keyboard(e), report))?;
    report(Step::ScanEnable, Verdict::Ok { detail: 0 });

    Ok(BringUp { flushed, selftest, ctr, kbd_test, probe, set, extra, repeat })
}
