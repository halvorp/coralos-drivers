// SPDX-License-Identifier: GPL-2.0-only
//! The safe state: reachable from any state, verified by readback, ordered mute-first.
mod common;
use common::*;
use da7219_core::runner::*;
use da7219_core::safe::*;

const HP_L: u8 = 0x6b;
const HP_R: u8 = 0x6c;
const DAC_L: u8 = 0x69;
const DAC_R: u8 = 0x6a;
const MIXOUT_L: u8 = 0x6e;
const MIXOUT_R: u8 = 0x6f;
const CP: u8 = 0x47;

/// The loudest state the chip can be in: everything enabled, nothing muted, outputs driving the jack, all gains at full.
fn loud() -> Fake {
    let mut f = Fake::reset();
    f.regs[HP_L as usize] = 0x80 | 0x08 | 0x20;
    f.regs[HP_R as usize] = 0x80 | 0x08 | 0x20;
    f.regs[DAC_L as usize] = 0x80 | 0x20;
    f.regs[DAC_R as usize] = 0x80 | 0x20;
    f.regs[MIXOUT_L as usize] = 0x80;
    f.regs[MIXOUT_R as usize] = 0x80;
    f.regs[CP as usize] = 0x80 | 0x20;
    f.regs[0x45] = 0x7f;
    f.regs[0x48] = 0x3f;
    f
}

#[test]
fn the_safe_requirements_are_exactly_these_seven_registers() {
    // HP amp: muted, disabled, output-enable off; DAC: muted, disabled; mixout off; charge pump off
    // HP mask = MUTE 0x40 | EN 0x80 | OE 0x08 = 0xc8 (want MUTE only); DAC mask = MUTE | EN = 0xc0 (want MUTE only)
    let want = vec![
        ("hp-l-safe", 0x6b, 0xc8, 0x40),
        ("hp-r-safe", 0x6c, 0xc8, 0x40),
        ("dac-l-safe", 0x69, 0xc0, 0x40),
        ("dac-r-safe", 0x6a, 0xc0, 0x40),
        ("mixout-l-off", 0x6e, 0x80, 0),
        ("mixout-r-off", 0x6f, 0x80, 0),
        ("charge-pump-off", 0x47, 0x80, 0),
    ];
    let got: Vec<_> = SAFE_CHECKS.iter().map(|c| (c.label, c.reg, c.mask, c.want)).collect();
    assert_eq!(got, want);
}

#[test]
fn from_the_loudest_state_the_plan_reaches_a_verified_safe_state() {
    let mut f = loud();
    assert!(first_unsafe(|r| f.reg(r)).is_some());
    f.run(safe_state_plan().unwrap()).unwrap();
    assert_eq!(first_unsafe(|r| f.reg(r)), None);
    assert_eq!(f.reg(HP_L) & 0xc8, 0x40);
    assert_eq!(f.reg(HP_R) & 0xc8, 0x40);
    assert_eq!(f.reg(DAC_L) & 0xc0, 0x40);
    assert_eq!(f.reg(DAC_R) & 0xc0, 0x40);
    assert_eq!((f.reg(MIXOUT_L) & 0x80, f.reg(MIXOUT_R) & 0x80, f.reg(CP) & 0x80), (0, 0, 0));
    assert_eq!(f.reg(HP_L) & 0x04, 0x04, "pop guard left on");
    assert_only_codec(&f);
}

#[test]
fn it_is_idempotent_and_valid_from_the_reset_state() {
    let mut f = Fake::reset();
    f.run(safe_state_plan().unwrap()).unwrap();
    let snap = f.regs;
    f.run(safe_state_plan().unwrap()).unwrap();
    assert_eq!(f.regs, snap);
}

#[test]
fn the_order_stops_the_output_and_mutes_before_anything_is_powered_down() {
    let mut f = loud();
    f.run(safe_state_plan().unwrap()).unwrap();
    let pos = |l: &str| f.steps.iter().position(|s| *s == l).unwrap();
    // output enable off, amplifier mute, DAC mute, then the stages
    assert!(pos("hp-l-oe-off") < pos("hp-l-mute"));
    assert!(pos("hp-r-mute") < pos("dac-l-mute"));
    for later in ["mixout-l-off", "mixout-r-off", "dac-l-off", "dac-r-off", "hp-l-amp-off", "hp-r-amp-off"] {
        assert!(pos("dac-r-mute") < pos(later), "{later}");
    }
    // the pop guard (minimum gain) is on before the mixout stages go
    assert!(pos("min-gain-settle") < pos("mixout-l-off"));
    // the charge pump goes last of the power-downs, and the verification is last of all
    assert!(pos("hp-r-amp-off") < pos("charge-pump-off"));
    assert_eq!(*f.steps.last().unwrap(), "charge-pump-off");
}

#[test]
fn each_unsafe_register_is_named_when_it_cannot_be_made_safe() {
    // a stuck bit in each of the seven registers: the failure names exactly that check
    for c in SAFE_CHECKS {
        let mut f = loud();
        // choose a bit in the mask that the safe state wants different from the loud value, and freeze it
        let loud_v = f.reg(c.reg);
        let wrong = (loud_v ^ c.want) & c.mask;
        assert_ne!(wrong, 0, "{}", c.label);
        f.stuck.push((c.reg, wrong));
        let e = f.run(safe_state_plan().unwrap()).unwrap_err();
        assert!(matches!(e.cause, Cause::VerifyMismatch { reg, .. } if reg == c.reg), "{} -> {e:?}", c.label);
    }
}

#[test]
fn a_bus_that_does_not_answer_fails_at_the_first_step_so_the_state_is_unknown_not_assumed_safe() {
    let mut f = loud();
    f.nak_at = Some("hp-l-oe-off");
    let e = f.run(safe_state_plan().unwrap()).unwrap_err();
    assert_eq!(e, Failure { step: "hp-l-oe-off", cause: Cause::BusNak });
    assert_eq!(f.reg(HP_L) & 0x08, 0x08, "nothing was done and nothing is claimed");
}

#[test]
fn first_unsafe_reports_the_register_and_what_it_read() {
    let mut regs = [0u8; 256];
    regs[HP_L as usize] = 0x40;
    regs[HP_R as usize] = 0x40;
    regs[DAC_L as usize] = 0x40;
    regs[DAC_R as usize] = 0x40;
    assert_eq!(first_unsafe(|r| regs[r as usize]), None);
    regs[HP_R as usize] = 0x40 | 0x08;
    let (c, got) = first_unsafe(|r| regs[r as usize]).unwrap();
    assert_eq!((c.label, got), ("hp-r-safe", 0x48));
    regs[HP_R as usize] = 0x00; // unmuted
    assert_eq!(first_unsafe(|r| regs[r as usize]).unwrap().0.label, "hp-r-safe");
}

#[test]
fn the_safe_plan_fits_its_cap_and_its_wait_is_one_pop_guard_delay() {
    let p = safe_state_plan().unwrap();
    assert!(!p.overflowed());
    assert_eq!(p.max_wait_ms(), 30, "DA7219_MIN_GAIN_DELAY once");
}
