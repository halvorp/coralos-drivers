// SPDX-License-Identifier: GPL-2.0-only
//! The SAFE STATE: every output muted and powered down, proven by reading the registers back.
//!
//! This is the minimum the owner's ruling requires of audio at all times (Astra audio ruling: "the minimum safe state is verified
//! amplifier shutdown or output disable"; PCM silence is not a hardware safety boundary). The order is the reverse of bringing the path
//! up and puts the cheapest-to-hear failure first: stop the output drivers and mute (amplifier, then DAC), enable the amplifier's minimum
//! gain (the pop guard, da7219.c:908-914), then power the stages off, then read every control register back.
//!
//! A safe-state plan that FAILS leaves the amplifier state UNKNOWN. The `.device` must treat that as a fault: keep the I2S clocks stopped
//! (the DSP path not running), report `safe-state-unverified` upward, and not admit a stream.
use crate::bus::*;
use crate::fields::*;
use crate::regs::*;

/// One readback requirement: `(value & mask) == want`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Check {
    pub label: &'static str,
    pub reg: u8,
    pub mask: u8,
    pub want: u8,
}

const HP_BITS_L: u8 = DA7219_HP_L_AMP_MUTE_EN_MASK | DA7219_HP_L_AMP_EN_MASK | DA7219_HP_L_AMP_OE_MASK;
const HP_BITS_R: u8 = DA7219_HP_R_AMP_MUTE_EN_MASK | DA7219_HP_R_AMP_EN_MASK | DA7219_HP_R_AMP_OE_MASK;
const DAC_BITS_L: u8 = DA7219_DAC_L_MUTE_EN_MASK | DA7219_DAC_L_EN_MASK;
const DAC_BITS_R: u8 = DA7219_DAC_R_MUTE_EN_MASK | DA7219_DAC_R_EN_MASK;

/// What "safe" means, register by register. Number of entries is fixed by the HEADPHONE PATH'S hardware: two amplifiers, two DACs, two
/// mixout stages, one charge pump.
pub const SAFE_CHECKS: [Check; 7] = [
    Check { label: "hp-l-safe", reg: DA7219_HP_L_CTRL, mask: HP_BITS_L, want: DA7219_HP_L_AMP_MUTE_EN_MASK },
    Check { label: "hp-r-safe", reg: DA7219_HP_R_CTRL, mask: HP_BITS_R, want: DA7219_HP_R_AMP_MUTE_EN_MASK },
    Check { label: "dac-l-safe", reg: DA7219_DAC_L_CTRL, mask: DAC_BITS_L, want: DA7219_DAC_L_MUTE_EN_MASK },
    Check { label: "dac-r-safe", reg: DA7219_DAC_R_CTRL, mask: DAC_BITS_R, want: DA7219_DAC_R_MUTE_EN_MASK },
    Check { label: "mixout-l-off", reg: DA7219_MIXOUT_L_CTRL, mask: DA7219_MIXOUT_L_AMP_EN_MASK, want: 0 },
    Check { label: "mixout-r-off", reg: DA7219_MIXOUT_R_CTRL, mask: DA7219_MIXOUT_R_AMP_EN_MASK, want: 0 },
    Check { label: "charge-pump-off", reg: DA7219_CP_CTRL, mask: DA7219_CP_EN_MASK, want: 0 },
];

/// The plan that reaches the safe state and verifies it.
pub fn safe_state_plan() -> Result<Plan, crate::PlanError> {
    let mut p = Plan::new();
    // 1. stop driving the jack and mute: amplifier first, then the DAC
    p.push(Step::abort("hp-l-oe-off", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_OE_MASK, value: 0 })); // da7219-aad.c:465-469
    p.push(Step::abort("hp-r-oe-off", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_OE_MASK, value: 0 }));
    p.push(Step::abort("hp-l-mute", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MUTE_EN_MASK, value: DA7219_HP_L_AMP_MUTE_EN_MASK }));
    p.push(Step::abort("hp-r-mute", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MUTE_EN_MASK, value: DA7219_HP_R_AMP_MUTE_EN_MASK }));
    p.push(Step::abort("dac-l-mute", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_MUTE_EN_MASK, value: DA7219_DAC_L_MUTE_EN_MASK }));
    p.push(Step::abort("dac-r-mute", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_MUTE_EN_MASK, value: DA7219_DAC_R_MUTE_EN_MASK }));
    // 2. minimum gain on the amplifiers (pop guard), then the stages off
    p.push(Step::abort("hp-l-min-gain-on", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_MIN_GAIN_EN_MASK, value: DA7219_HP_L_AMP_MIN_GAIN_EN_MASK }));
    p.push(Step::abort("hp-r-min-gain-on", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_MIN_GAIN_EN_MASK, value: DA7219_HP_R_AMP_MIN_GAIN_EN_MASK }));
    p.push(Step::abort("min-gain-settle", Op::DelayMs(DA7219_MIN_GAIN_DELAY as u16)));
    p.push(Step::abort("mixout-l-off", Op::Update { reg: DA7219_MIXOUT_L_CTRL, mask: DA7219_MIXOUT_L_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("mixout-r-off", Op::Update { reg: DA7219_MIXOUT_R_CTRL, mask: DA7219_MIXOUT_R_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("dac-l-off", Op::Update { reg: DA7219_DAC_L_CTRL, mask: DA7219_DAC_L_EN_MASK, value: 0 }));
    p.push(Step::abort("dac-r-off", Op::Update { reg: DA7219_DAC_R_CTRL, mask: DA7219_DAC_R_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-l-amp-off", Op::Update { reg: DA7219_HP_L_CTRL, mask: DA7219_HP_L_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("hp-r-amp-off", Op::Update { reg: DA7219_HP_R_CTRL, mask: DA7219_HP_R_AMP_EN_MASK, value: 0 }));
    p.push(Step::abort("charge-pump-off", Op::Update { reg: DA7219_CP_CTRL, mask: DA7219_CP_EN_MASK, value: 0 }));
    // 3. prove it
    for c in SAFE_CHECKS {
        p.push(Step::abort(c.label, Op::Verify { reg: c.reg, mask: c.mask, want: c.want }));
    }
    p.checked()
}

/// The first register of `read` (a register file, e.g. the result of a readback) that is not in the safe state.
pub fn first_unsafe(read: impl Fn(u8) -> u8) -> Option<(Check, u8)> {
    SAFE_CHECKS.iter().find_map(|c| {
        let got = read(c.reg) & c.mask;
        (got != c.want).then_some((*c, got))
    })
}
