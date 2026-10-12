// SPDX-License-Identifier: GPL-2.0-only
//! The headphone playback path: power-up, power-down, mute, unmute and gain, in the order Linux's DAPM runs them.
//!
//! Where the order comes from. `da7219.c` declares widgets (da7219.c:957-1107) and routes (da7219.c:1109-1160); the ASoC core sorts the
//! widgets it powers by TYPE, then SUBSEQUENCE, then REGISTER (`dapm_seq_compare`, soc-dapm.c:1775-1805) using `dapm_up_seq`
//! (soc-dapm.c:74-113) going up and `dapm_down_seq` (soc-dapm.c:115-153, subsequence reversed) going down. For the playback path that gives:
//!
//! | up | type      | widget                | register   | event (da7219.c)                                  |
//! |----|-----------|-----------------------|------------|---------------------------------------------------|
//! | 1  | pre       | Pre Power Gain Ramp   | GAIN_RAMP  | PRE_PMU: ramp rate := NOMINAL (:926-955)         |
//! | 3  | supply 0  | DAI                   | DAI_CTRL   | PRE_PMU: PC_COUNT synchronised; SRM lock (:797-871)  |
//! | 3  | supply 0  | Charge Pump           | CP_CTRL    | POST_PMU: settle 40 ms (:873-886)                 |
//! | 3  | supply 1  | Headphone Left/Right  | HP_x_CTRL  | POST_PMU: settle 40 ms                            |
//! | 8  | dac       | DACL, DACR            | DAC_x_CTRL | POST_PMU: settle 40 ms                            |
//! | 10 | pga       | Mixout Left/Right     | MIXOUT_x   | POST_PMU: remove HP minimum gain (:888-924)       |
//! | 15 | post      | Post Power Gain Ramp  | GAIN_RAMP  | POST_PMU: restore the ramp rate                   |
//!
//! Going down the types run pga (6), dac (8), supply (14, subsequence 1 before 0), post (16), after the pre step (1). The board
//! widget "Platform Clock" (PLL) belongs to the machine driver and is the caller's: see [`crate::clock`].
//!
//! THE MUTE-BEFORE-ENABLE RULE. The DAPM events never touch a MUTE bit: the codec powers up with `DAC_x_CTRL` and `HP_x_CTRL` at
//! their reset value 0x40 (MUTE_EN set, da7219.c:2362-2365) and stays muted until the user-visible switches ("Playback Switch",
//! "Headphone Switch") clear MUTE_EN. This port makes that a CHECKED precondition of [`power_up_plan`] (it begins by verifying all four
//! mute bits are set and aborts if not), makes [`unmute_plan`] a separate plan that raises DAC before the headphone amplifier, and makes
//! [`power_down_plan`] begin by muting the amplifier then the DAC.
//!
//! THE OUTPUT ENABLE (`HP_x_AMP_OE`) is not a DAPM matter either: Linux sets it only after the accessory-detect has classified the
//! jack (`da7219_aad_btn_det_work` / `da7219_aad_hptest_work`, da7219-aad.c:62-68, :309-312) and clears it on removal
//! (da7219-aad.c:465-469). [`power_up_plan`] leaves it clear; [`crate::aad`] owns it.
use crate::bus::*;
use crate::fields::*;
use crate::regs::*;

/// `DA7219_DAC_x_MUTE_EN | DA7219_HP_x_AMP_MUTE_EN` live in the same bit (6) of four registers.
const MUTE: u8 = DA7219_HP_L_AMP_MUTE_EN_MASK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerUp {
    /// The `GAIN_RAMP_CTRL` value the "Pre Power Gain Ramp" widget saved and the "Post" one restores (da7219.c:926-950). The reset value is 0.
    pub gain_ramp_restore: u8,
    /// How many times to ask whether the SRM PLL locked before giving up (da7219.c:844, [`DA7219_SRM_CHECK_RETRIES`] = 8). Only meaningful
    /// when `PLL_CTRL` is in SRM mode; a failure is TOLERATED, as Linux does ("SRM failed to lock", da7219.c:846-847).
    pub srm_lock_polls: u8,
}

impl Default for PowerUp {
    fn default() -> Self {
        Self { gain_ramp_restore: 0, srm_lock_polls: DA7219_SRM_CHECK_RETRIES }
    }
}

/// The four mute bits the whole headphone path starts and ends under: (register, label).
const MUTES: [(u8, &str); 4] = [
    (DA7219_HP_L_CTRL, "hp-l-muted"),
    (DA7219_HP_R_CTRL, "hp-r-muted"),
    (DA7219_DAC_L_CTRL, "dac-l-muted"),
    (DA7219_DAC_R_CTRL, "dac-r-muted"),
];

/// Power the headphone path up with every output still muted. Precondition (verified): the four mute bits are set.
pub fn power_up_plan(o: PowerUp) -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    for (reg, label) in MUTES {
        p.push(Step::abort(label, Op::Verify { reg, mask: MUTE, want: MUTE }));
    }
    // pre (1)
    p.push(Step::abort("gain-ramp-nominal", Op::Write { reg: DA7219_GAIN_RAMP_CTRL, value: DA7219_GAIN_RAMP_RATE_NOMINAL }));
    // supply, subsequence 0, by register: DAI_CTRL (0x2C) then CP_CTRL (0x47)
    p.push(Step::abort("pc-count-synchronised", Op::Update { reg: DA7219_PC_COUNT, mask: DA7219_PC_FREERUN_MASK, value: 0 })); // da7219.c:826-828
    if o.srm_lock_polls > 0 {
        // da7219.c:830-847: Linux polls only when PLL_CTRL is in SRM mode; the caller passes srm_lock_polls = 0 otherwise
        p.push(Step::new(
            "srm-lock",
            Op::Poll { reg: DA7219_PLL_SRM_STS, mask: DA7219_PLL_SRM_STS_SRM_LOCK, want: DA7219_PLL_SRM_STS_SRM_LOCK, interval_ms: DA7219_SYS_STAT_CHECK_DELAY as u16, max_polls: o.srm_lock_polls },
            OnFail::Tolerate,
        ));
    }
    p.push(Step::abort("dai-enable", Op::Update { reg: DA7219_DAI_CTRL, mask: DA7219_DAI_EN_MASK, value: DA7219_DAI_EN_MASK }));
    p.push(Step::abort("charge-pump-enable", Op::Update { reg: DA7219_CP_CTRL, mask: DA7219_CP_EN_MASK, value: DA7219_CP_EN_MASK }));
    p.push(Step::abort("charge-pump-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    // supply, subsequence 1
    p.push(Step::abort("hp-l-amp-enable", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_EN_MASK, value: DA7219_HP_L_AMP_EN_MASK }));
    p.push(Step::abort("hp-l-amp-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    p.push(Step::abort("hp-r-amp-enable", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_EN_MASK, value: DA7219_HP_R_AMP_EN_MASK }));
    p.push(Step::abort("hp-r-amp-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    // dac (8)
    p.push(Step::abort("dac-l-enable", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_EN_MASK, value: DA7219_DAC_L_EN_MASK }));
    p.push(Step::abort("dac-l-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    p.push(Step::abort("dac-r-enable", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_EN_MASK, value: DA7219_DAC_R_EN_MASK }));
    p.push(Step::abort("dac-r-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    // pga (10): enable, then POST_PMU removes the HP minimum gain (still muted)
    p.push(Step::abort("mixout-l-enable", Op::Update { reg: DA7219_MIXOUT_L_CTRL, mask: DA7219_MIXOUT_L_AMP_EN_MASK, value: DA7219_MIXOUT_L_AMP_EN_MASK }));
    p.push(Step::abort("hp-l-min-gain-off", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MIN_GAIN_EN_MASK, value: 0 }));
    p.push(Step::abort("mixout-r-enable", Op::Update { reg: DA7219_MIXOUT_R_CTRL, mask: DA7219_MIXOUT_R_AMP_EN_MASK, value: DA7219_MIXOUT_R_AMP_EN_MASK }));
    p.push(Step::abort("hp-r-min-gain-off", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MIN_GAIN_EN_MASK, value: 0 }));
    // post (15)
    p.push(Step::abort("gain-ramp-restore", Op::Write { reg: DA7219_GAIN_RAMP_CTRL, value: o.gain_ramp_restore }));
    // the outputs are STILL muted: prove it
    for (reg, label) in MUTES {
        p.push(Step::abort(label, Op::Verify { reg, mask: MUTE, want: MUTE }));
    }
    p.checked()
}

/// Power the headphone path down: mute (amplifier, then DAC), then the DAPM down sequence.
pub fn power_down_plan(o: PowerUp) -> Result<Plan, crate::PlanError> {
    let mut p = mute_plan()?;
    p.push(Step::abort("gain-ramp-nominal", Op::Write { reg: DA7219_GAIN_RAMP_CTRL, value: DA7219_GAIN_RAMP_RATE_NOMINAL }));
    // pga (6): PRE_PMD enables the minimum gain and waits, then the mixout amp goes off (da7219.c:908-914)
    p.push(Step::abort("hp-l-min-gain-on", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MIN_GAIN_EN_MASK, value: DA7219_HP_L_AMP_MIN_GAIN_EN_MASK }));
    p.push(Step::abort("hp-l-min-gain-settle", Op::DelayMs(DA7219_MIN_GAIN_DELAY as u16)));
    p.push(Step::abort("mixout-l-disable", Op::Update { reg: DA7219_MIXOUT_L_CTRL, mask: DA7219_MIXOUT_L_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-r-min-gain-on", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MIN_GAIN_EN_MASK, value: DA7219_HP_R_AMP_MIN_GAIN_EN_MASK }));
    p.push(Step::abort("hp-r-min-gain-settle", Op::DelayMs(DA7219_MIN_GAIN_DELAY as u16)));
    p.push(Step::abort("mixout-r-disable", Op::Update { reg: DA7219_MIXOUT_R_CTRL, mask: DA7219_MIXOUT_R_AMP_EN_MASK, value: 0 }));
    // dac (8), POST_PMD settle
    p.push(Step::abort("dac-l-disable", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_EN_MASK, value: 0 }));
    p.push(Step::abort("dac-l-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    p.push(Step::abort("dac-r-disable", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_EN_MASK, value: 0 }));
    p.push(Step::abort("dac-r-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    // supply (14): subsequence 1 (headphone amps) first, then 0 by register (DAI, charge pump)
    p.push(Step::abort("hp-l-amp-disable", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-l-amp-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    p.push(Step::abort("hp-r-amp-disable", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-r-amp-settle", Op::DelayMs(DA7219_SETTLING_DELAY as u16)));
    p.push(Step::abort("dai-disable", Op::Update { reg: DA7219_DAI_CTRL, mask: DA7219_DAI_EN_MASK, value: 0 }));
    p.push(Step::abort("pc-count-freerun", Op::Update { reg: DA7219_PC_COUNT, mask: DA7219_PC_FREERUN_MASK, value: DA7219_PC_FREERUN_MASK })); // da7219.c:851-853
    p.push(Step::abort("charge-pump-disable", Op::Update { reg: DA7219_CP_CTRL, mask: DA7219_CP_EN_MASK, value: 0 }));
    // post (16)
    p.push(Step::abort("gain-ramp-restore", Op::Write { reg: DA7219_GAIN_RAMP_CTRL, value: o.gain_ramp_restore }));
    p.checked()
}

/// Mute the amplifier, then the DAC (the reverse of [`unmute_plan`]), and verify both.
pub fn mute_plan() -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    p.push(Step::abort("hp-l-mute", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MUTE_EN_MASK, value: DA7219_HP_L_AMP_MUTE_EN_MASK }));
    p.push(Step::abort("hp-r-mute", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MUTE_EN_MASK, value: DA7219_HP_R_AMP_MUTE_EN_MASK }));
    p.push(Step::abort("dac-l-mute", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_MUTE_EN_MASK, value: DA7219_DAC_L_MUTE_EN_MASK }));
    p.push(Step::abort("dac-r-mute", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_MUTE_EN_MASK, value: DA7219_DAC_R_MUTE_EN_MASK }));
    for (reg, label) in MUTES {
        p.push(Step::abort(label, Op::Verify { reg, mask: MUTE, want: MUTE }));
    }
    p.checked()
}

/// Unmute: the DAC first, then the headphone amplifier (the amplifier's gain ramp is enabled at probe, so its unmute ramps).
/// Refuses to run unless the path is POWERED: each register is verified enabled before its mute bit is cleared.
pub fn unmute_plan() -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    p.push(Step::abort("dac-l-powered", Op::Verify { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_EN_MASK, want: DA7219_DAC_L_EN_MASK }));
    p.push(Step::abort("dac-r-powered", Op::Verify { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_EN_MASK, want: DA7219_DAC_R_EN_MASK }));
    p.push(Step::abort("hp-l-powered", Op::Verify { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_EN_MASK | DA7219_HP_L_AMP_MIN_GAIN_EN_MASK, want: DA7219_HP_L_AMP_EN_MASK }));
    p.push(Step::abort("hp-r-powered", Op::Verify { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_EN_MASK | DA7219_HP_R_AMP_MIN_GAIN_EN_MASK, want: DA7219_HP_R_AMP_EN_MASK }));
    p.push(Step::abort("dac-l-unmute", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_MUTE_EN_MASK, value: 0 }));
    p.push(Step::abort("dac-r-unmute", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_MUTE_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-l-unmute", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MUTE_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-r-unmute", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MUTE_EN_MASK, value: 0 }));
    p.checked()
}

/// The two hardware gain codes the path is allowed to reach. `DAC` is `DAC_x_GAIN` (7 bits, 0.75 dB per code, 0x6F = 0 dB, da7219.c:53-57);
/// `HP` is `HP_x_GAIN` (6 bits, 1 dB per code, 0x39 = 0 dB, da7219.c:60).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GainPolicy {
    pub dac_max: u8,
    pub hp_max: u8,
    pub dac_initial: u8,
    pub hp_initial: u8,
}

impl GainPolicy {
    /// The CEILING is the point ChromeOS itself never exceeds: UCM `Playback Digital Volume 111` (= 0x6F, 0 dB) and CRAS `avs_da7219`
    /// `max_volume 0` (ATLAS-PERIPHERALS-FROM-RECOVERY.md section 3.4); the headphone amplifier stays at its reset 0 dB. The INITIAL values
    /// are conservative (-12 dB DAC, -6 dB amplifier) and PROVISIONAL: the owner or a silicon measurement sets the real figures.
    pub const ATLAS_HEADPHONES: GainPolicy = GainPolicy {
        dac_max: DA7219_DAC_DIGITAL_GAIN_0DB,
        hp_max: DA7219_HP_AMP_GAIN_0DB,
        dac_initial: DA7219_DAC_DIGITAL_GAIN_0DB - 16,
        hp_initial: DA7219_HP_AMP_GAIN_0DB - 6,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GainError {
    /// The requested code is above the ceiling; the ceiling is not exceeded and nothing is written.
    DacAboveCeiling { got: u8, ceiling: u8 },
    HpAboveCeiling { got: u8, ceiling: u8 },
    /// The ceiling itself is not a legal register value (above the field maximum).
    CeilingNotARegisterValue { what: &'static str, ceiling: u8 },
    Plan(crate::PlanError),
}

/// Set both channels' DAC digital gain and headphone amplifier gain. A code above the policy ceiling is REFUSED by name (never clamped
/// silently); the policy itself must be a legal register value.
pub fn gain_plan(dac: u8, hp: u8, policy: GainPolicy) -> Result<Plan, GainError> {
    if policy.dac_max > DA7219_DAC_DIGITAL_GAIN_MAX {
        return Err(GainError::CeilingNotARegisterValue { what: "dac", ceiling: policy.dac_max });
    }
    if policy.hp_max > DA7219_HP_AMP_GAIN_MAX {
        return Err(GainError::CeilingNotARegisterValue { what: "hp", ceiling: policy.hp_max });
    }
    if dac > policy.dac_max {
        return Err(GainError::DacAboveCeiling { got: dac, ceiling: policy.dac_max });
    }
    if hp > policy.hp_max {
        return Err(GainError::HpAboveCeiling { got: hp, ceiling: policy.hp_max });
    }
    let mut p = Plan::new();
    p.push(Step::abort("dac-l-gain", Op::Update { reg: DA7219_DAC_L_GAIN, mask: DA7219_DAC_L_DIGITAL_GAIN_MASK, value: dac }));
    p.push(Step::abort("dac-r-gain", Op::Update { reg: DA7219_DAC_R_GAIN, mask: DA7219_DAC_R_DIGITAL_GAIN_MASK, value: dac }));
    p.push(Step::abort("hp-l-gain", Op::Update { reg: DA7219_HP_L_GAIN, mask: DA7219_HP_L_AMP_GAIN_MASK, value: hp }));
    p.push(Step::abort("hp-r-gain", Op::Update { reg: DA7219_HP_R_GAIN, mask: DA7219_HP_R_AMP_GAIN_MASK, value: hp }));
    p.checked().map_err(GainError::Plan)
}

/// The initial, conservative gains of a policy.
pub fn initial_gain_plan(policy: GainPolicy) -> Result<Plan, GainError> {
    gain_plan(policy.dac_initial, policy.hp_initial, policy)
}
