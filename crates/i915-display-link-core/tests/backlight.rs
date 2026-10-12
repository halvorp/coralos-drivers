// SPDX-License-Identifier: GPL-2.0-only
//! SPT PCH PWM backlight vs intel_backlight.c / intel_backlight_regs.h (v7.0-rc6), with the Pixelbook Go's real VBT.
#![allow(unused_imports)]
use i915_common_core::Reg;
use i915_display_link_core::backlight::*;
use i915_display_link_core::plan::*;
use i915_display_link_core::regs::*;
use i915_vbt_core::backlight::{parse_lfp_backlight, Backlight};
use i915_vbt_core::Vbt;

#[test]
fn backlight_registers_are_linuxs() {
    // intel_backlight_regs.h:74,81-85,25; i915_reg.h:1035,1055; intel_display_regs.h:1807,1812
    assert_eq!(BLC_PWM_CPU_CTL2.offset(), 0x48250);
    assert_eq!(BLC_PWM_PCH_CTL1.offset(), 0xc8250);
    assert_eq!(BLC_PWM_PCH_CTL2.offset(), 0xc8254);
    assert_eq!(SOUTH_CHICKEN1.offset(), 0xc2000);
    assert_eq!(PCH_RAWCLK_FREQ.offset(), 0xc6204);
    assert_eq!(BLM_PWM_ENABLE, 1 << 31);
    assert_eq!(BLM_PCH_PWM_ENABLE, 1 << 31);
    assert_eq!(BLM_PCH_OVERRIDE_ENABLE, 1 << 30);
    assert_eq!(BLM_PCH_POLARITY, 1 << 29);
    assert_eq!(BACKLIGHT_DUTY_CYCLE_MASK, 0xffff);
    assert_eq!(SPT_PWM_GRANULARITY, 1);
    assert_eq!(RAWCLK_FREQ_MASK, 0x3ff);
}

#[test]
fn hz_to_pwm_is_the_spt_formula() {
    // DIV_ROUND_CLOSEST(MHz(24), hz * mul): mul = 16 by default, 128 with SOUTH_CHICKEN1 bit 0
    assert_eq!(spt_hz_to_pwm(200, false), 7500); // 24_000_000 / 3_200
    assert_eq!(spt_hz_to_pwm(200, true), 938); // 24_000_000 / 25_600 = 937.5 -> 938
    assert_eq!(spt_hz_to_pwm(1000, false), 1500);
    assert_eq!(spt_hz_to_pwm(25_000, false), 60); // 24e6 / 400_000
    assert_eq!(spt_hz_to_pwm(1_000_000, true), 0); // period rounds to 0: "conversion failed"
    assert_eq!(spt_hz_to_pwm(0, false), 0);
    assert_eq!(SPT_PWM_CLOCK_HZ, 24_000_000);
}

#[test]
fn scale_maps_linearly_rounding_to_nearest() {
    // intel_backlight.c:36-62
    assert_eq!(scale(0, 0, 255, 0, 7500), 0);
    assert_eq!(scale(255, 0, 255, 0, 7500), 7500);
    assert_eq!(scale(64, 0, 255, 0, 7500), 1882); // 1882.35
    assert_eq!(scale(128, 0, 255, 0, 7500), 3765); // 3764.7 -> 3765
    assert_eq!(scale(300, 0, 255, 0, 7500), 7500); // clamped to the source range first
    assert_eq!(scale(5, 10, 20, 100, 200), 100); // below the source range
    // degenerate ranges return target_min (the WARN_ON paths)
    assert_eq!(scale(5, 10, 10, 7, 9), 7);
    assert_eq!(scale(5, 0, 10, 9, 7), 9);
}

fn vbt_bl(freq: u16, min: u16) -> Backlight {
    Backlight { present: true, entry_type: 2, control_type: Some(2), controller: 0, pwm_freq_hz: freq, active_low_pwm: false, min_brightness: min, brightness_precision_bits: 0, hdr_dpcd_refresh_timeout: 30 }
}

#[test]
fn the_vbt_frequency_defaults_to_200_hz() {
    assert_eq!(vbt_pwm_freq(&vbt_bl(0, 0)), 200);
    assert_eq!(vbt_pwm_freq(&vbt_bl(1000, 0)), 1000);
    assert_eq!(DEFAULT_PWM_FREQ_HZ, 200);
}

#[test]
fn the_vbt_minimum_is_clamped_to_a_quarter_of_the_range() {
    assert_eq!(backlight_min_vbt(&vbt_bl(200, 0), 7500), 0);
    assert_eq!(backlight_min_vbt(&vbt_bl(200, 64), 7500), 1882);
    assert_eq!(backlight_min_vbt(&vbt_bl(200, 255), 7500), 1882); // "XXX: If the vbt value is 255 ..." clamp to 64
    assert_eq!(backlight_min_vbt(&vbt_bl(200, 100), 7500), 1882);
    assert_eq!(backlight_min_vbt(&vbt_bl(200, 10), 7500), 294); // 10/255 * 7500 = 294.1
    assert_eq!(VBT_MIN_BRIGHTNESS_CLAMP, 64);
}

#[test]
fn setup_with_blank_registers_takes_everything_from_the_vbt() {
    let s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 0).unwrap();
    assert_eq!(s.pwm_level_max, 7500);
    assert_eq!(s.pwm_level_min, 0);
    assert!(!s.active_low_pwm && !s.alternate_pwm_increment && !s.pwm_enabled);
    assert_eq!((s.min, s.max, s.level), (0, 7500, 0));
}

#[test]
fn setup_prefers_what_the_firmware_programmed() {
    // CTL2 = 4000 << 16 | duty 1000, CTL1 = enabled + active low, SOUTH_CHICKEN1 alt increment
    let s = BacklightState::setup(&vbt_bl(200, 0), 1, BLM_PCH_PWM_ENABLE | BLM_PCH_POLARITY, (4000 << 16) | 1000).unwrap();
    assert_eq!(s.pwm_level_max, 4000); // not the VBT's 938
    assert!(s.active_low_pwm && s.alternate_pwm_increment && s.pwm_enabled);
    assert_eq!(s.level, 1000);
    // with the alternate increment and nothing programmed the VBT frequency uses the x128 divisor
    let s = BacklightState::setup(&vbt_bl(200, 0), 1, 0, 0).unwrap();
    assert_eq!(s.pwm_level_max, 938);
}

#[test]
fn setup_refuses_by_name_when_there_is_no_backlight_or_no_period() {
    let mut v = vbt_bl(200, 0);
    v.present = false;
    let e = BacklightState::setup(&v, 0, 0, 0).unwrap_err();
    assert_eq!(e, BacklightError::NotPresentInVbt);
    assert_eq!(e.name(), "BACKLIGHT-NOT-IN-VBT");
    // NoPwmPeriod (`hz_to_pwm()` returning 0 with nothing in the registers) cannot be reached from a u16 VBT frequency on SPT:
    // 65535 Hz * 128 still divides MHz(24) to 3. It is kept because Linux guards it (:1257-1258); its name is pinned here.
    assert_eq!(BacklightError::NoPwmPeriod { pwm_freq_hz: 0 }.name(), "BACKLIGHT-NO-PWM-PERIOD");
    assert!(spt_hz_to_pwm(65_535, true) >= 3);
}

#[test]
fn atlas_backlight_from_the_real_vbt() {
    let p = std::env::var("I915_ATLAS_VBT").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/sources/atlas-vbt.bin".into());
    let Ok(buf) = std::fs::read(p) else {
        eprintln!("SKIP: no Atlas VBT on this host");
        return;
    };
    let vbt = Vbt::parse(&buf).unwrap();
    let bl = parse_lfp_backlight(&vbt, 2).unwrap().unwrap();
    // i915-vbt-core's tests pin: PWM, 200 Hz, min 0
    let s = BacklightState::setup(&bl, 0, 0, 0).unwrap();
    assert_eq!(s.pwm_level_max, 7500);
    assert_eq!(s.pwm_level_min, 0);
}

#[test]
fn user_and_hardware_levels_scale_into_each_others_ranges() {
    let mut s = BacklightState::setup(&vbt_bl(200, 64), 0, 0, 0).unwrap();
    assert_eq!((s.min, s.max), (1882, 7500));
    assert_eq!(s.user_to_hw(0, 255), 1882); // clamped up to the minimum
    assert_eq!(s.user_to_hw(255, 255), 7500);
    assert_eq!(s.user_to_hw(128, 255), 3765);
    assert_eq!(s.user_to_hw(1000, 255), 7500);
    assert_eq!(s.hw_to_user(7500, 255), 255);
    assert_eq!(s.hw_to_user(1882, 255), 0);
    s.min = 0;
    assert_eq!(s.hw_to_user(3750, 100), 50);
}

#[test]
fn set_only_touches_the_duty_cycle_field() {
    let s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 0).unwrap();
    assert_eq!(s.set_plan(1234), vec![Effect::Rmw { reg: BLC_PWM_PCH_CTL2, clear: 0xffff, set: 1234 }]);
    assert_eq!(s.set_plan(0x1_2345), vec![Effect::Rmw { reg: BLC_PWM_PCH_CTL2, clear: 0xffff, set: 0x2345 }]);
}

#[test]
fn enable_follows_lpt_enable_backlight_for_spt() {
    let mut s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 0).unwrap();
    let p = s.enable_plan(3000);
    assert_eq!(
        p,
        vec![
            // "PCH backlight already enabled": drop the enable bit first, only if it is set
            Effect::ClearIfSet { reg: Reg::new(0xc8250), bits: 1 << 31 },
            // SPT: SOUTH_CHICKEN1 granularity bit follows alternate_pwm_increment
            Effect::Rmw { reg: Reg::new(0xc2000), clear: 1, set: 0 },
            // pch_ctl2 = pwm_level_max << 16
            Effect::Write { reg: Reg::new(0xc8254), value: 7500 << 16 },
            // pch_ctl1 = polarity only (no LPT override bit on SPT), written, read back, then written with ENABLE
            Effect::Write { reg: Reg::new(0xc8250), value: 0 },
            Effect::PostingRead { reg: Reg::new(0xc8250) },
            Effect::Write { reg: Reg::new(0xc8250), value: 1 << 31 },
            // "This won't stick until the above enable"
            Effect::Rmw { reg: Reg::new(0xc8254), clear: 0xffff, set: 3000 },
        ]
    );
    assert_eq!(7500u32 << 16, 0x1D4C_0000);
    assert!(s.pwm_enabled);
    assert_eq!(s.level, 3000);
}

#[test]
fn enable_with_active_low_and_alternate_increment() {
    let mut s = BacklightState::setup(&vbt_bl(200, 0), 1, BLM_PCH_POLARITY, 0).unwrap();
    let p = s.enable_plan(500);
    assert!(p.contains(&Effect::Rmw { reg: Reg::new(0xc2000), clear: 1, set: 1 }));
    assert!(p.contains(&Effect::Write { reg: Reg::new(0xc8250), value: 1 << 29 }));
    assert!(p.contains(&Effect::Write { reg: Reg::new(0xc8250), value: (1 << 29) | (1 << 31) }));
    assert!(p.contains(&Effect::Write { reg: Reg::new(0xc8254), value: 938 << 16 }));
}

#[test]
fn enable_raises_a_level_below_the_minimum() {
    let mut s = BacklightState::setup(&vbt_bl(200, 64), 0, 0, 0).unwrap();
    let p = s.enable_plan(10);
    assert_eq!(p.last(), Some(&Effect::Rmw { reg: Reg::new(0xc8254), clear: 0xffff, set: 1882 })); // intel_backlight.c:782
    assert_eq!(s.level, 1882);
}

#[test]
fn disable_zeroes_the_duty_drops_cpu_pwm_and_clears_the_pch_enable() {
    let mut s = BacklightState::setup(&vbt_bl(200, 0), 0, BLM_PCH_PWM_ENABLE, 0).unwrap();
    assert!(s.pwm_enabled);
    let p = s.disable_plan();
    assert_eq!(
        p,
        vec![
            Effect::Rmw { reg: Reg::new(0xc8254), clear: 0xffff, set: 0 },
            Effect::ClearIfSet { reg: Reg::new(0x48250), bits: 1 << 31 },
            Effect::Rmw { reg: Reg::new(0xc8250), clear: 1 << 31, set: 0 },
        ]
    );
    assert!(!s.pwm_enabled);
}

#[test]
fn backlight_plans_write_only_backlight_registers() {
    let mut s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 0).unwrap();
    let mut all = s.enable_plan(100);
    all.extend(s.disable_plan());
    for r in written_regs(&all) {
        assert!([0xc8250, 0xc8254, 0xc2000, 0x48250].contains(&r.offset()), "{:#x}", r.offset());
    }
}

// ---- mutation-sweep additions ---------------------------------------------------------------------------------------
#[test]
fn scale_rounds_halves_up_and_thirds_down() {
    assert_eq!(scale(1, 0, 2, 0, 1), 1); // 0.5 -> 1
    assert_eq!(scale(1, 0, 3, 0, 1), 0); // 0.33 -> 0
    assert_eq!(scale(2, 0, 3, 0, 1), 1); // 0.67 -> 1
    assert_eq!(scale(1, 0, 4, 0, 1), 0); // 0.25 -> 0
    assert_eq!(scale(3, 0, 4, 0, 1), 1);
    assert_eq!(scale(7, 5, 5, 3, 4), 3);
}

#[test]
fn a_register_max_of_one_is_a_real_max() {
    let s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 1 << 16).unwrap();
    assert_eq!(s.pwm_level_max, 1); // only 0 means "unset"
}

#[test]
fn with_no_vbt_minimum_user_zero_is_hardware_zero() {
    let s = BacklightState::setup(&vbt_bl(200, 0), 0, 0, 0).unwrap();
    assert_eq!(s.user_to_hw(0, 255), 0);
    assert_eq!(s.user_to_hw(255, 255), 7500);
    assert_eq!(s.user_to_hw(1, 255), 29); // 29.4
    assert_eq!(s.hw_to_user(0, 255), 0);
}

#[test]
fn enable_leaves_a_level_at_the_minimum_alone() {
    let mut s = BacklightState::setup(&vbt_bl(200, 64), 0, 0, 0).unwrap();
    s.enable_plan(1882);
    assert_eq!(s.level, 1882);
    s.enable_plan(1883);
    assert_eq!(s.level, 1883);
    s.enable_plan(1881);
    assert_eq!(s.level, 1882);
}
