// SPDX-License-Identifier: GPL-2.0-only
//! SPEAKERS ARE NEVER TOUCHED. Every plan and the interrupt machine, run end to end against the fake, produce bus messages that all
//! target the DA7219 at 0x1A - and none target a MAX98373 amplifier (0x31 right, 0x32 left).
mod common;
use common::*;
use da7219_core::aad::*;
use da7219_core::bus::*;
use da7219_core::clock::*;
use da7219_core::dai::*;
use da7219_core::hp::*;
use da7219_core::init::*;
use da7219_core::safe::*;

fn all_plans() -> Vec<(&'static str, Plan)> {
    vec![
        ("quiesce", quiesce_plan().unwrap()),
        ("read-system-active", read_system_active_plan()),
        ("init", da7219_core::init::init_plan(ATLAS_PDATA).unwrap()),
        ("rev-aa-patch", rev_aa_patch_plan()),
        ("bias-off", bias_off_plan()),
        ("clock-up", atlas_clock_up().unwrap()),
        ("clock-down", atlas_clock_down().unwrap()),
        ("dai-fmt", fmt_plan(ATLAS_DAI_FMT).unwrap()),
        ("dai-hw-params", hw_params_plan(48000, 32, 2).unwrap()),
        ("route", route_plan().unwrap()),
        ("power-up", power_up_plan(PowerUp::default()).unwrap()),
        ("power-down", power_down_plan(PowerUp::default()).unwrap()),
        ("mute", mute_plan().unwrap()),
        ("unmute", unmute_plan().unwrap()),
        ("gain", initial_gain_plan(GainPolicy::ATLAS_HEADPHONES).unwrap()),
        ("safe", safe_state_plan().unwrap()),
        ("aad-init", da7219_core::aad::init_plan(ATLAS_AAD_PDATA).unwrap()),
        ("aad-exit", exit_plan()),
        ("aad-detect-on", detection_plan(true)),
        ("aad-detect-off", detection_plan(false)),
        ("aad-ground-on", ground_switch_plan(true)),
        ("aad-ground-off", ground_switch_plan(false)),
        ("aad-drive", output_drive_plan(true)),
        ("aad-undrive", output_drive_plan(false)),
        ("aad-buttons", button_detect_plan(ATLAS_AAD_PDATA).unwrap()),
        ("aad-removal", removal_plan().unwrap()),
    ]
}

#[test]
fn every_plan_the_crate_can_build_talks_only_to_the_codec() {
    let plans = all_plans();
    assert_eq!(plans.len(), 26, "a new plan must be added to this list");
    for (name, plan) in plans {
        let mut f = Fake::reset();
        // run against a reset chip; a verification failure is fine here - what matters is where the messages went
        let _ = f.run(plan);
        assert!(!f.log.is_empty(), "{name}");
        for t in &f.log {
            assert_eq!(t.addr, CODEC_ADDR, "{name}: {t:?}");
            assert!(!AMP_ADDRS.contains(&t.addr), "{name} reached an amplifier");
        }
    }
}

#[test]
fn the_interrupt_machine_never_leaves_the_codec_either() {
    for (a, b, s) in [(0x01u8, 0u8, 0x01u8), (0x04, 0, 0x03), (0x04, 0, 0x01), (0, 0xff, 0x03), (0x02, 0, 0x00)] {
        let mut f = Fake::reset();
        f.regs[0xc2] = a;
        f.regs[0xc3] = b;
        f.regs[0xc0] = s;
        let mut m = Irq::new(ATLAS_AAD_PDATA, AadState::new(ATLAS_AAD_PDATA.jack_det_rate));
        let mut input = da7219_core::runner::Input::Begin;
        loop {
            match m.next(input) {
                IrqOutput::Do(step) => input = f.exec(step),
                _ => break,
            }
        }
        assert_only_codec(&f);
    }
}
