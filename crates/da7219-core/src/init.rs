// SPDX-License-Identifier: GPL-2.0-only
//! Codec initialisation: `da7219_probe` (da7219.c:2454-2596) and `da7219_handle_pdata` (da7219.c:2269-2316) as a plan, ending in the safe
//! state. The accessory-detect block is initialised separately ([`crate::aad::init_plan`]), as Linux does last (da7219.c:2586).
//!
//! Beyond Linux: the chip identity is VERIFIED (`CHIP_ID1`/`CHIP_ID2` against the reset values the regmap defaults assume,
//! da7219.c:2368-2369) so a plan can never configure a different part that happens to answer at 0x1A.
use crate::bus::*;
use crate::aad_fields::DA7219_ACCDET_EN_MASK;
use crate::fields::*;
use crate::regs::*;
use crate::safe::SAFE_CHECKS;

/// `enum da7219_micbias_voltage` (include/sound/da7219.h:14-21).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicBias {
    V1_6 = 0,
    V1_8,
    V2_0,
    V2_2,
    V2_4,
    V2_6,
}

/// `enum da7219_mic_amp_in_sel` (include/sound/da7219.h:24-28).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicAmpIn {
    Diff = 0,
    SeP,
    SeN,
}

/// The firmware properties `da7219_fw_to_pdata` reads (da7219.c:1767-1803), with the Atlas values (recovery ACPI, section 3.2:
/// `micbias_lvl = 2600 mV`, `mic_amp_in_sel = diff`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pdata {
    pub micbias: MicBias,
    pub mic_amp_in: MicAmpIn,
    /// `IO_CTRL`: the voltage range of the I2C/I2S pads. Linux derives it from the VDDIO regulator and, without a valid one (an ACPI board), leaves
    /// the 2.5-3.6 V setting (da7219.c:1861-1900, default at :1880). Atlas' bus is 1.8 V, so the 1.2-2.8 V range is chosen here; the live unit's `IO_CTRL`
    /// readback is a recon fact (see the wave-2 report).
    pub io_voltage_level: u8,
}

pub const ATLAS_PDATA: Pdata = Pdata { micbias: MicBias::V2_6, mic_amp_in: MicAmpIn::Diff, io_voltage_level: DA7219_IO_VOLTAGE_LEVEL_1_2V_2_8V };

/// The chip identity registers' reset values (da7219.c:2368-2369).
pub const CHIP_ID1: u8 = 0x23; // da7219.c:2368
pub const CHIP_ID2: u8 = 0x93; // da7219.c:2369

/// `if system_active` of da7219_probe (da7219.c:2472-2489): audio paths still running from a previous start are stopped first.
pub fn quiesce_plan() -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    p.push(Step::abort("gain-ramp-nominal", Op::Write { reg: DA7219_GAIN_RAMP_CTRL, value: DA7219_GAIN_RAMP_RATE_NOMINAL }));
    p.push(Step::abort("system-modes-input", Op::Write { reg: DA7219_SYSTEM_MODES_INPUT, value: 0x00 }));
    p.push(Step::abort("system-modes-output", Op::Write { reg: DA7219_SYSTEM_MODES_OUTPUT, value: 0x01 }));
    // da7219.c:2480-2486: six reads 50 ms apart; Linux does not fail if the status never clears
    p.push(Step::new("system-status-clear", Op::Poll { reg: DA7219_SYSTEM_STATUS, mask: 0xFF, want: 0, interval_ms: DA7219_SYS_STAT_CHECK_DELAY as u16, max_polls: DA7219_SYS_STAT_CHECK_RETRIES }, OnFail::Tolerate));
    p.checked()
}

/// Read `SYSTEM_ACTIVE` (the plan's own single read decides whether [`quiesce_plan`] runs first).
pub fn read_system_active_plan() -> Plan {
    let mut p = Plan::new();
    p.push(Step::abort("system-active-read", Op::Read { reg: DA7219_SYSTEM_ACTIVE }));
    p
}

/// The rest of `da7219_probe`: soft reset, IO level, identity, pdata, default ramping, minimum gain, bias, then the safe-state readback.
pub fn init_plan(pdata: Pdata) -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    // soft reset component (da7219.c:2490-2499)
    p.push(Step::abort("accdet-off", Op::Update { reg: DA7219_ACCDET_CONFIG_1, mask: DA7219_ACCDET_EN_MASK, value: 0 }));
    p.push(Step::abort("soft-reset", Op::Update { reg: DA7219_CIF_CTRL, mask: DA7219_CIF_REG_SOFT_RESET_MASK, value: DA7219_CIF_REG_SOFT_RESET_MASK }));
    p.push(Step::abort("system-active-off", Op::Update { reg: DA7219_SYSTEM_ACTIVE, mask: DA7219_SYSTEM_ACTIVE_MASK, value: 0 }));
    p.push(Step::abort("system-active-on", Op::Update { reg: DA7219_SYSTEM_ACTIVE, mask: DA7219_SYSTEM_ACTIVE_MASK, value: DA7219_SYSTEM_ACTIVE_MASK }));
    // regcache_cache_bypass(false); regmap_reinit_cache(): the device discards any register shadow it keeps (da7219.c:2501-2502)
    p.push(Step::abort("register-shadow-reinit", Op::DelayMs(0)));
    p.push(Step::abort("io-voltage-level", Op::Write { reg: DA7219_IO_CTRL, value: pdata.io_voltage_level })); // da7219.c:2505
    // beyond Linux: this is the part we think it is
    p.push(Step::abort("chip-id-1", Op::Verify { reg: DA7219_CHIP_ID1, mask: DA7219_CHIP_ID1_MASK, want: CHIP_ID1 }));
    p.push(Step::abort("chip-id-2", Op::Verify { reg: DA7219_CHIP_ID2, mask: DA7219_CHIP_ID2_MASK, want: CHIP_ID2 }));
    // pdata (da7219.c:2269-2316)
    p.push(Step::abort("micbias-level", Op::Write { reg: DA7219_MICBIAS_CTRL, value: (pdata.micbias as u8) << DA7219_MICBIAS1_LEVEL_SHIFT }));
    p.push(Step::abort("mic-amp-input", Op::Write { reg: DA7219_MIC_1_SELECT, value: pdata.mic_amp_in as u8 }));
    // da7219.c:2546-2576
    p.push(Step::abort("pc-count-freerun", Op::Update { reg: DA7219_PC_COUNT, mask: DA7219_PC_FREERUN_MASK, value: DA7219_PC_FREERUN_MASK }));
    for (label, reg, mask) in [
        ("ramp-mixin-l", DA7219_MIXIN_L_CTRL, DA7219_MIXIN_L_AMP_RAMP_EN_MASK),
        ("ramp-adc-l", DA7219_ADC_L_CTRL, DA7219_ADC_L_RAMP_EN_MASK),
        ("ramp-dac-l", DA7219_DAC_L_CTRL, DA7219_DAC_L_RAMP_EN_MASK),
        ("ramp-dac-r", DA7219_DAC_R_CTRL, DA7219_DAC_R_RAMP_EN_MASK),
        ("ramp-hp-l", DA7219_HP_L_CTRL, DA7219_HP_L_AMP_RAMP_EN_MASK),
        ("ramp-hp-r", DA7219_HP_R_CTRL, DA7219_HP_R_AMP_RAMP_EN_MASK),
        ("min-gain-hp-l", DA7219_HP_L_CTRL, DA7219_HP_L_AMP_MIN_GAIN_EN_MASK),
        ("min-gain-hp-r", DA7219_HP_R_CTRL, DA7219_HP_R_AMP_MIN_GAIN_EN_MASK),
    ] {
        p.push(Step::abort(label, Op::Update { reg, mask, value: mask }));
    }
    p.push(Step::abort("tone-gen-cycles", Op::Write { reg: DA7219_TONE_GEN_CYCLES, value: DA7219_BEEP_CYCLES_MASK }));
    // set_bias_level(STANDBY) from OFF: master bias (da7219.c:1830-1834)
    p.push(Step::abort("master-bias", Op::Update { reg: DA7219_REFERENCES, mask: DA7219_BIAS_EN_MASK, value: DA7219_BIAS_EN_MASK }));
    // beyond Linux: a freshly initialised codec is PROVEN safe
    for c in SAFE_CHECKS {
        p.push(Step::abort(c.label, Op::Verify { reg: c.reg, mask: c.mask, want: c.want }));
    }
    p.checked()
}

/// `da7219_rev_aa_patch` (da7219.c:2450-2452), applied when `CHIP_REVISION`'s minor field is 0 (da7219.c:2507-2520).
pub fn rev_aa_patch_plan() -> Plan {
    let mut p = Plan::new();
    p.push(Step::abort("rev-aa-patch", Op::Write { reg: DA7219_REFERENCES, value: 0x08 }));
    p
}

/// Whether a `CHIP_REVISION` value needs [`rev_aa_patch_plan`] (da7219.c:2507-2520).
pub const fn needs_rev_aa_patch(chip_revision: u8) -> bool {
    chip_revision & DA7219_CHIP_MINOR_MASK == 0
}

/// `da7219_set_bias_level(OFF)`: master bias off (da7219.c:1843-1848; Atlas is not a wake-up source).
pub fn bias_off_plan() -> Plan {
    let mut p = Plan::new();
    p.push(Step::abort("master-bias-off", Op::Update { reg: DA7219_REFERENCES, mask: DA7219_BIAS_EN_MASK, value: 0 }));
    p
}
