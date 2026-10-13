// SPDX-License-Identifier: GPL-2.0-only
//! The headphone path: DAPM order, the mute-before-enable rule, unmute/mute order, gains and their ceilings.
mod common;
use common::*;
use da7219_core::bus::*;
use da7219_core::hp::*;
use da7219_core::runner::*;

const HP_L: u8 = 0x6b;
const HP_R: u8 = 0x6c;
const DAC_L: u8 = 0x69;
const DAC_R: u8 = 0x6a;
const MIXOUT_L: u8 = 0x6e;
const MIXOUT_R: u8 = 0x6f;
const CP: u8 = 0x47;
const DAI: u8 = 0x2c;
const RAMP: u8 = 0x92;
const MUTE: u8 = 0x40;

fn initialised() -> Fake {
    // the state probe leaves: ramps on, HP minimum gain ON, bias on (da7219.c:2546-2576)
    let mut f = Fake::reset();
    f.run(da7219_core::init::init_plan(da7219_core::init::ATLAS_PDATA).unwrap()).unwrap();
    f.writes.clear();
    f.steps.clear();
    f.log.clear();
    f
}

fn powered_up() -> Fake {
    let mut f = initialised();
    f.run(power_up_plan(PowerUp { srm_lock_polls: 0, ..PowerUp::default() }).unwrap()).unwrap();
    f
}

#[test]
fn power_up_writes_registers_in_the_order_dapm_sorts_the_widgets() {
    let mut f = initialised();
    assert_eq!(f.reg(HP_L) & 0x04, 0x04, "probe leaves the minimum gain on");
    f.run(power_up_plan(PowerUp { srm_lock_polls: 0, gain_ramp_restore: 0 }).unwrap()).unwrap();
    // soc-dapm.c dapm_up_seq: pre, supply(subseq 0 by register: DAI 0x2c, charge pump 0x47; subseq 1: HP L, HP R), dac, pga, post
    let regs: Vec<u8> = f.writes.iter().map(|w| w.1).collect();
    assert_eq!(regs, vec![RAMP, 0x94, DAI, CP, HP_L, HP_R, DAC_L, DAC_R, MIXOUT_L, HP_L, MIXOUT_R, HP_R, RAMP]);
    // PC_COUNT is the "PC synchronised to DAI" PRE_PMU of the DAI widget (da7219.c:826-828)
    assert_eq!(f.writes[1].0, "pc-count-synchronised");
    assert_only_codec(&f);
}

#[test]
fn power_up_ends_with_everything_enabled_and_everything_still_muted() {
    let f = powered_up();
    assert_eq!(f.reg(DAI) & 0x80, 0x80);
    assert_eq!(f.reg(CP) & 0x80, 0x80);
    for r in [HP_L, HP_R] {
        assert_eq!(f.reg(r) & 0x80, 0x80, "amp enabled");
        assert_eq!(f.reg(r) & MUTE, MUTE, "amp still muted");
        assert_eq!(f.reg(r) & 0x04, 0, "minimum gain removed by the mixout POST_PMU");
        assert_eq!(f.reg(r) & 0x08, 0, "output enable is the accessory detect's, not DAPM's");
    }
    for r in [DAC_L, DAC_R] {
        assert_eq!(f.reg(r) & 0x80, 0x80);
        assert_eq!(f.reg(r) & MUTE, MUTE);
    }
    for r in [MIXOUT_L, MIXOUT_R] {
        assert_eq!(f.reg(r) & 0x80, 0x80);
    }
    assert_eq!(f.reg(RAMP), 0, "gain ramp restored");
}

#[test]
fn power_up_never_clears_a_mute_bit_or_sets_the_output_enable() {
    let f = powered_up();
    let mut cur = initialised();
    for (_, reg, val) in &f.writes {
        let before = cur.regs[*reg as usize];
        if [HP_L, HP_R, DAC_L, DAC_R].contains(reg) {
            assert_eq!(val & MUTE, MUTE, "reg {reg:#x}");
            assert_eq!(val & 0x08, before & 0x08, "OE untouched");
        }
        cur.regs[*reg as usize] = *val;
    }
}

#[test]
fn power_up_refuses_to_start_when_any_output_is_unmuted_and_touches_nothing() {
    for (reg, name) in [(HP_L, "hp-l-muted"), (HP_R, "hp-r-muted"), (DAC_L, "dac-l-muted"), (DAC_R, "dac-r-muted")] {
        let mut f = Fake::reset();
        f.regs[reg as usize] &= !MUTE;
        let e = f.run(power_up_plan(PowerUp::default()).unwrap()).unwrap_err();
        assert_eq!(e.step, name);
        assert!(matches!(e.cause, Cause::VerifyMismatch { reg: r, got: 0, .. } if r == reg));
        assert!(f.writes.is_empty(), "no register was written before the precondition failed");
    }
}

#[test]
fn the_srm_lock_wait_is_bounded_and_its_failure_is_tolerated_as_linux_does() {
    let mut f = initialised();
    let t = f.run(power_up_plan(PowerUp::default()).unwrap()).unwrap();
    let t = t.expect("SRM never locked in the fake");
    assert_eq!(t.step, "srm-lock");
    assert_eq!(t.cause, Cause::PollBound { reads: 8, last: 0 });
    assert_eq!(f.poll_reads, 8, "DA7219_SRM_CHECK_RETRIES");
    assert_eq!(f.reg(HP_L) & 0x80, 0x80, "the path still powered up");
    // zero polls: no wait at all
    let p = power_up_plan(PowerUp { srm_lock_polls: 0, ..PowerUp::default() }).unwrap();
    assert!(!p.as_slice().iter().any(|s| s.label == "srm-lock"));
}

#[test]
fn power_up_wait_budget_is_exactly_five_settles_and_the_lock_poll() {
    // 5 settling delays of 40 ms (charge pump, HP L, HP R, DAC L, DAC R) + 8 reads x 50 ms
    assert_eq!(power_up_plan(PowerUp::default()).unwrap().max_wait_ms(), 5 * 40 + 8 * 50);
    // power-down: 2 min-gain waits of 30 ms + 4 settles of 40 ms (DAC L, DAC R, HP L, HP R)
    assert_eq!(power_down_plan(PowerUp::default()).unwrap().max_wait_ms(), 2 * 30 + 4 * 40);
}

#[test]
fn unmute_raises_the_dac_first_and_the_amplifier_last_and_mute_is_the_reverse() {
    let mut f = powered_up();
    f.writes.clear();
    f.run(unmute_plan().unwrap()).unwrap();
    assert_eq!(f.writes.iter().map(|w| w.1).collect::<Vec<_>>(), vec![DAC_L, DAC_R, HP_L, HP_R]);
    for r in [HP_L, HP_R, DAC_L, DAC_R] {
        assert_eq!(f.reg(r) & MUTE, 0);
    }
    f.writes.clear();
    f.run(mute_plan().unwrap()).unwrap();
    assert_eq!(f.writes.iter().map(|w| w.1).collect::<Vec<_>>(), vec![HP_L, HP_R, DAC_L, DAC_R]);
    for r in [HP_L, HP_R, DAC_L, DAC_R] {
        assert_eq!(f.reg(r) & MUTE, MUTE);
    }
}

#[test]
fn unmute_refuses_an_unpowered_path_by_name() {
    let mut f = initialised();
    let e = f.run(unmute_plan().unwrap()).unwrap_err();
    assert_eq!(e.step, "dac-l-powered");
    assert!(f.writes.is_empty());
    // a powered path whose headphone amplifier still has its minimum gain on is not ready either, on either channel
    for (reg, step) in [(HP_L, "hp-l-powered"), (HP_R, "hp-r-powered")] {
        let mut g = powered_up();
        g.regs[reg as usize] |= 0x04;
        let e = g.run(unmute_plan().unwrap()).unwrap_err();
        assert_eq!(e.step, step);
        assert_eq!(g.reg(DAC_L) & MUTE, MUTE, "nothing was unmuted");
    }
    for (reg, step) in [(DAC_L, "dac-l-powered"), (DAC_R, "dac-r-powered"), (HP_L, "hp-l-powered"), (HP_R, "hp-r-powered")] {
        let mut g = powered_up();
        g.regs[reg as usize] &= !0x80; // the stage is not enabled
        assert_eq!(g.run(unmute_plan().unwrap()).unwrap_err().step, step);
    }
}

#[test]
fn a_stuck_mute_bit_is_caught_by_the_verification_not_assumed() {
    let mut f = powered_up();
    f.stuck.push((HP_L, MUTE));
    f.regs[HP_L as usize] &= !MUTE; // the chip believes it is unmuted, writes cannot fix it
    let e = f.run(mute_plan().unwrap()).unwrap_err();
    assert_eq!(e.step, "hp-l-muted");
}

#[test]
fn power_down_mutes_first_then_follows_the_dapm_down_sequence_and_leaves_the_chip_off() {
    let mut f = powered_up();
    f.run(unmute_plan().unwrap()).unwrap();
    f.writes.clear();
    f.run(power_down_plan(PowerUp::default()).unwrap()).unwrap();
    let regs: Vec<u8> = f.writes.iter().map(|w| w.1).collect();
    // mutes (HP L, HP R, DAC L, DAC R); ramp; pga: HP L min gain, MIXOUT L off, HP R min gain, MIXOUT R off; dac L, dac R;
    // supply subseq 1: HP L amp, HP R amp; subseq 0: DAI, PC_COUNT, charge pump; post
    assert_eq!(regs, vec![HP_L, HP_R, DAC_L, DAC_R, RAMP, HP_L, MIXOUT_L, HP_R, MIXOUT_R, DAC_L, DAC_R, HP_L, HP_R, DAI, 0x94, CP, RAMP]);
    for r in [HP_L, HP_R, DAC_L, DAC_R, MIXOUT_L, MIXOUT_R, CP, DAI] {
        assert_eq!(f.reg(r) & 0x80, 0, "reg {r:#x} off");
    }
    assert_eq!(f.reg(0x94) & 1, 1, "PC counter free-running again");
    // mute lands before any stage is powered off
    let first_off = f.writes.iter().position(|w| w.0 == "mixout-l-disable").unwrap();
    let last_mute = f.writes.iter().position(|w| w.0 == "dac-r-mute").unwrap();
    assert!(last_mute < first_off);
}

#[test]
fn the_down_sequence_raises_minimum_gain_before_each_mixout_amp_goes_off() {
    let mut f = powered_up();
    f.run(power_down_plan(PowerUp::default()).unwrap()).unwrap();
    let pos = |l: &str| f.steps.iter().position(|s| *s == l).unwrap();
    assert!(pos("hp-l-min-gain-on") < pos("hp-l-min-gain-settle"));
    assert!(pos("hp-l-min-gain-settle") < pos("mixout-l-disable"));
    assert!(pos("hp-r-min-gain-on") < pos("mixout-r-disable"));
    assert_eq!(f.reg(HP_L) & 0x04, 0x04, "minimum gain left on, as at probe");
}

#[test]
fn gains_are_set_within_the_ceiling_and_a_higher_code_is_refused_by_name() {
    let pol = GainPolicy::ATLAS_HEADPHONES;
    // ChromeOS: UCM 'Playback Digital Volume 111' = 0x6f, HP amp reset 0x39 (da7219.h:421, :442)
    assert_eq!((pol.dac_max, pol.hp_max), (0x6f, 0x39));
    assert_eq!((pol.dac_initial, pol.hp_initial), (0x5f, 0x33), "-12 dB DAC (16 x 0.75), -6 dB amp");
    let mut f = Fake::reset();
    f.run(initial_gain_plan(pol).unwrap()).unwrap();
    assert_eq!((f.reg(0x45), f.reg(0x46), f.reg(0x48), f.reg(0x49)), (0x5f, 0x5f, 0x33, 0x33));
    assert_eq!(gain_plan(0x70, 0x30, pol), Err(GainError::DacAboveCeiling { got: 0x70, ceiling: 0x6f }));
    assert_eq!(gain_plan(0x60, 0x3a, pol), Err(GainError::HpAboveCeiling { got: 0x3a, ceiling: 0x39 }));
    assert!(gain_plan(0x6f, 0x39, pol).is_ok(), "the ceiling itself is allowed");
    let bad = GainPolicy { dac_max: 0x80, ..pol };
    assert_eq!(gain_plan(0, 0, bad), Err(GainError::CeilingNotARegisterValue { what: "dac", ceiling: 0x80 }));
    let bad = GainPolicy { hp_max: 0x40, ..pol };
    assert_eq!(gain_plan(0, 0, bad), Err(GainError::CeilingNotARegisterValue { what: "hp", ceiling: 0x40 }));
}

#[test]
fn a_refused_gain_writes_nothing() {
    let pol = GainPolicy::ATLAS_HEADPHONES;
    let f = Fake::reset();
    assert!(gain_plan(0x7f, 0x3f, pol).is_err());
    assert!(f.writes.is_empty());
}

#[test]
fn gain_writes_preserve_the_reserved_bits_of_the_register() {
    let mut f = Fake::reset();
    f.regs[0x45] = 0x80;
    f.regs[0x48] = 0xc0;
    f.run(gain_plan(0x10, 0x20, GainPolicy::ATLAS_HEADPHONES).unwrap()).unwrap();
    assert_eq!(f.reg(0x45), 0x80 | 0x10);
    assert_eq!(f.reg(0x48), 0xc0 | 0x20);
}

#[test]
fn power_up_plans_fit_their_cap() {
    for p in [power_up_plan(PowerUp::default()), power_down_plan(PowerUp::default()), mute_plan(), unmute_plan()] {
        let p = p.unwrap();
        assert!(!p.overflowed());
        assert!(p.len() < PLAN_CAP);
    }
}

#[test]
fn the_route_sends_the_dai_through_the_dacs_and_mixers_to_the_amplifiers_and_nothing_else() {
    let mut f = initialised();
    // reset values (da7219.c:2336-2341): DIG_ROUTING_DAC 0x32, MIXOUT_x_SELECT 0 (OFF), DROUTING_ST_OUTFILT 0x01 / 0x02
    assert_eq!((f.reg(0x2e), f.reg(0x4b), f.reg(0x4c), f.reg(0x3c), f.reg(0x3d)), (0x32, 0, 0, 0x01, 0x02));
    f.regs[0x2e] = 0x00; // as if the DACs listened to the ADC
    f.regs[0x3c] = 0x04; // sidetone into the left mixer
    f.regs[0x4b] = 0x80; // an unrelated bit that must survive
    f.run(route_plan().unwrap()).unwrap();
    assert_eq!(f.reg(0x2e), 0x32, "DAC L from DAIL (2), DAC R from DAIR (3)");
    assert_eq!(f.reg(0x4b), 0x81, "left output mixer takes the DAC; the other bits are left alone");
    assert_eq!(f.reg(0x4c), 0x01);
    assert_eq!(f.reg(0x3c), 0x01, "left sidetone mixer takes only the left output filter");
    assert_eq!(f.reg(0x3d), 0x02, "right takes only the right");
    assert_only_codec(&f);
    // the route is applied while muted and touches no mute, enable or gain register
    for (_, reg, _) in &f.writes {
        assert!(![HP_L, HP_R, DAC_L, DAC_R, MIXOUT_L, MIXOUT_R, CP, 0x45, 0x46, 0x48, 0x49].contains(reg), "reg {reg:#x}");
    }
}

#[test]
fn a_route_that_does_not_stick_is_caught_by_name() {
    let mut f = initialised();
    f.stuck.push((0x4b, 0x01));
    let e = f.run(route_plan().unwrap()).unwrap_err();
    assert_eq!(e.step, "route-mixout-l-set");
}
