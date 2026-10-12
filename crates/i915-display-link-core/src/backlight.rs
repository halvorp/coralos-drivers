// SPDX-License-Identifier: GPL-2.0-only
//! The PCH PWM backlight of an SPT-LP platform (`spt_pwm_funcs` over the LPT-style `BLC_PWM_PCH_CTL1/2`): setup from the
//! registers + the VBT, frequency to PWM period, enable, set and disable as plans.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_backlight.c (v7.0-rc6): `scale` (:42-62), `clamp_user_to_hw`
//! (:68-78), `scale_hw_to_user` (:80-88), `intel_backlight_set_pwm_level` (:108-117), `lpt_get_backlight` (:150-155),
//! `lpt_set_backlight` (:212-220), `intel_backlight_disable` (:444-473), `lpt_disable_backlight` (:342-366),
//! `lpt_enable_backlight` (:475-518), `spt_hz_to_pwm` (:1050-1061), `get_vbt_pwm_freq` (:1162-1179), `get_backlight_max_vbt`
//! (:1181-1202), `get_backlight_min_vbt` (:1206-1231), `lpt_setup_backlight` (:1233-1288), `intel_pwm_*` wrappers
//! (:1598-1649), `intel_backlight_init_funcs` (:1814-1857, the `HAS_PCH_SPLIT` + `PCH_LPT_H <= type < PCH_CNP` + not LPT arm).
//! Copyright (c) 2008-2012 Intel Corporation.
//!
//! The VBT inputs are `i915-vbt-core`'s [`Backlight`] (`pwm_freq_hz`, `min_brightness`), never re-parsed here.
//!
//! NOT PORTED (named): the backlight class device and user-brightness properties beyond the pure scaling, the eDP AUX
//! (DPCD) backlight (`intel_dp_aux_init_backlight_funcs`, tried first on eDP, :1843-1846 — a panel whose DPCD advertises AUX
//! brightness control would use it; the Pixelbook's VBT selects PWM, `control_type` 2), the `invert_brightness` module
//! parameter and quirk (`intel_backlight_invert_pwm_level` is the identity here), the LPT-only CPU-PWM override migration, and
//! the PPS `EDP_BLC_ENABLE` gate (that is `pps::Kind::BacklightOn/Off`, called around these plans by the orchestrator).
use crate::plan::{Effect, Plan};
use crate::regs::*;
use alloc::vec;
use i915_vbt_core::backlight::Backlight;

/// "pwm_freq_hz = 200" when the VBT says nothing — intel_backlight.c:1172.
pub const DEFAULT_PWM_FREQ_HZ: u32 = 200;
/// `MHz(24)`: the SPT PWM clock — intel_backlight.c:1043-1049.
pub const SPT_PWM_CLOCK_HZ: u32 = 24_000_000;
/// The VBT minimum is clamped to at most 64/255 (25 %) — `clamp_t(int, min_brightness, 0, 64)`, intel_backlight.c:1222.
pub const VBT_MIN_BRIGHTNESS_CLAMP: u32 = 64;

/// Backlight refusals, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BacklightError {
    /// The VBT says no PWM backlight (`vbt.backlight.present` false) — `intel_backlight_setup()` returns 0 with nothing set
    /// up (:1676-1687).
    NotPresentInVbt,
    /// `hz_to_pwm()` returned 0 ("backlight frequency conversion failed", :1196-1199) and the register max was 0:
    /// `lpt_setup_backlight()` returns `-ENODEV` (:1257-1258).
    NoPwmPeriod { pwm_freq_hz: u32 },
}

impl BacklightError {
    pub const fn name(&self) -> &'static str {
        match self {
            BacklightError::NotPresentInVbt => "BACKLIGHT-NOT-IN-VBT",
            BacklightError::NoPwmPeriod { .. } => "BACKLIGHT-NO-PWM-PERIOD",
        }
    }
}

/// `scale()` — intel_backlight.c:42-62: `source_val` clamped into `[source_min, source_max]` and mapped linearly onto
/// `[target_min, target_max]`, rounding to nearest. A degenerate range returns `target_min` (Linux `WARN_ON`s).
pub fn scale(source_val: u32, source_min: u32, source_max: u32, target_min: u32, target_max: u32) -> u32 {
    if source_min >= source_max || target_min > target_max {
        return target_min;
    }
    let v = source_val.clamp(source_min, source_max);
    let num = (v - source_min) as u64 * (target_max - target_min) as u64;
    let den = (source_max - source_min) as u64;
    ((num + den / 2) / den) as u32 + target_min
}

/// `spt_hz_to_pwm()` — intel_backlight.c:1050-1061: the PWM period in units of `mul` clocks of the 24 MHz PWM clock, `mul`
/// 16 by default and 128 with the alternate increment (SOUTH_CHICKEN1 bit 0). `DIV_ROUND_CLOSEST(MHz(24), hz * mul)`.
pub fn spt_hz_to_pwm(pwm_freq_hz: u32, alternate_pwm_increment: bool) -> u32 {
    let mul: u64 = if alternate_pwm_increment { 128 } else { 16 };
    let d = pwm_freq_hz as u64 * mul;
    if d == 0 {
        return 0;
    }
    ((SPT_PWM_CLOCK_HZ as u64 + d / 2) / d) as u32
}

/// `get_vbt_pwm_freq()` — intel_backlight.c:1162-1179.
pub fn vbt_pwm_freq(vbt: &Backlight) -> u32 {
    if vbt.pwm_freq_hz != 0 {
        vbt.pwm_freq_hz as u32
    } else {
        DEFAULT_PWM_FREQ_HZ
    }
}

/// The panel backlight state (`panel->backlight`) for the PCH PWM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BacklightState {
    pub pwm_level_min: u32,
    pub pwm_level_max: u32,
    pub active_low_pwm: bool,
    pub alternate_pwm_increment: bool,
    pub pwm_enabled: bool,
    /// `panel->backlight.min` / `.max` / `.level` (`intel_pwm_setup_backlight`, :1599-1607: min/max = the PWM levels).
    pub min: u32,
    pub max: u32,
    pub level: u32,
}

/// `get_backlight_min_vbt()` — intel_backlight.c:1206-1231.
pub fn backlight_min_vbt(vbt: &Backlight, pwm_level_max: u32) -> u32 {
    let min = (vbt.min_brightness as u32).min(VBT_MIN_BRIGHTNESS_CLAMP);
    scale(min, 0, 255, 0, pwm_level_max)
}

impl BacklightState {
    /// `lpt_setup_backlight()` + `intel_pwm_setup_backlight()` — intel_backlight.c:1233-1288, :1634-1649, for an SPT platform.
    /// Inputs are the three registers Linux reads: `SOUTH_CHICKEN1`, `BLC_PWM_PCH_CTL1`, `BLC_PWM_PCH_CTL2`.
    pub fn setup(vbt: &Backlight, south_chicken1: u32, pch_ctl1: u32, pch_ctl2: u32) -> Result<BacklightState, BacklightError> {
        if !vbt.present {
            return Err(BacklightError::NotPresentInVbt);
        }
        let alt = south_chicken1 & SPT_PWM_GRANULARITY != 0;
        let active_low_pwm = pch_ctl1 & BLM_PCH_POLARITY != 0;
        let mut pwm_level_max = pch_ctl2 >> 16;
        if pwm_level_max == 0 {
            // get_backlight_max_vbt(): the VBT frequency through hz_to_pwm()
            pwm_level_max = spt_hz_to_pwm(vbt_pwm_freq(vbt), alt);
        }
        if pwm_level_max == 0 {
            return Err(BacklightError::NoPwmPeriod { pwm_freq_hz: vbt_pwm_freq(vbt) });
        }
        let pwm_level_min = backlight_min_vbt(vbt, pwm_level_max);
        let level = pch_ctl2 & BACKLIGHT_DUTY_CYCLE_MASK; // lpt_get_backlight, :150-155
        Ok(BacklightState {
            pwm_level_min,
            pwm_level_max,
            active_low_pwm,
            alternate_pwm_increment: alt,
            pwm_enabled: pch_ctl1 & BLM_PCH_PWM_ENABLE != 0,
            min: pwm_level_min,
            max: pwm_level_max,
            level: scale(level, pwm_level_min, pwm_level_max, pwm_level_min, pwm_level_max),
        })
    }

    /// `clamp_user_to_hw()` — intel_backlight.c:68-78: user level in `[0, user_max]` to a hardware level in `[min, max]`.
    pub fn user_to_hw(&self, user_level: u32, user_max: u32) -> u32 {
        scale(user_level, 0, user_max, 0, self.max).clamp(self.min, self.max)
    }

    /// `scale_hw_to_user()` — intel_backlight.c:80-88.
    pub fn hw_to_user(&self, hw_level: u32, user_max: u32) -> u32 {
        scale(hw_level, self.min, self.max, 0, user_max)
    }

    /// `lpt_set_backlight()` — intel_backlight.c:212-220: only the duty-cycle field of `BLC_PWM_PCH_CTL2` changes.
    pub fn set_plan(&self, pwm_level: u32) -> Plan {
        vec![Effect::Rmw { reg: BLC_PWM_PCH_CTL2, clear: BACKLIGHT_DUTY_CYCLE_MASK, set: pwm_level & BACKLIGHT_DUTY_CYCLE_MASK }]
    }

    /// `__intel_backlight_enable()` + `lpt_enable_backlight()` for non-LPT PCH — intel_backlight.c:774-795, :475-518.
    /// A level below the minimum is raised to it first (:782). Any PWM left enabled is disabled first (the `ClearIfSet`),
    /// then granularity, period (`pwm_level_max << 16`) and polarity are programmed, the PWM enabled, and the level set
    /// ("This won't stick until the above enable").
    pub fn enable_plan(&mut self, level: u32) -> Plan {
        let level = if level < self.min { self.min } else { level };
        self.level = level;
        self.pwm_enabled = true;
        let mut ctl1 = 0u32;
        if self.active_low_pwm {
            ctl1 |= BLM_PCH_POLARITY;
        }
        let mut p = vec![
            Effect::ClearIfSet { reg: BLC_PWM_PCH_CTL1, bits: BLM_PCH_PWM_ENABLE },
            Effect::Rmw { reg: SOUTH_CHICKEN1, clear: SPT_PWM_GRANULARITY, set: if self.alternate_pwm_increment { SPT_PWM_GRANULARITY } else { 0 } },
            Effect::Write { reg: BLC_PWM_PCH_CTL2, value: self.pwm_level_max << 16 },
            Effect::Write { reg: BLC_PWM_PCH_CTL1, value: ctl1 },
            Effect::PostingRead { reg: BLC_PWM_PCH_CTL1 },
            Effect::Write { reg: BLC_PWM_PCH_CTL1, value: ctl1 | BLM_PCH_PWM_ENABLE },
        ];
        p.extend(self.set_plan(level));
        p
    }

    /// `intel_backlight_disable()` -> `lpt_disable_backlight(level 0)` — intel_backlight.c:444-473, :342-366: duty to 0, drop a
    /// CPU PWM the firmware may have left enabled, then clear the PCH PWM enable.
    pub fn disable_plan(&mut self) -> Plan {
        self.pwm_enabled = false;
        let mut p = self.set_plan(0);
        p.push(Effect::ClearIfSet { reg: BLC_PWM_CPU_CTL2, bits: BLM_PWM_ENABLE });
        p.push(Effect::Rmw { reg: BLC_PWM_PCH_CTL1, clear: BLM_PCH_PWM_ENABLE, set: 0 });
        p
    }
}
