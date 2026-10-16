// SPDX-License-Identifier: GPL-2.0-only
//! The MAX98373's register plans, as data. A [`Step`] is one register operation or a bounded wait; a plan is a straight list of them. The
//! crate performs no I/O: the `.device` performs each step over I2C (the wire form is [`wire`]) at the amplifier's address, and a failed
//! step ends the plan.
//!
//! Ported from Linux `sound/soc/codecs/max98373.c` (probe :353-426, reset :325-350, the `Amp Enable` DAPM widget :64-66 with its event
//! :20-44) and `max98373-i2c.c` (`set_fmt` :121-171, `hw_params` :213-315, `set_tdm_slot` :317-401), driven the way the Intel AVS machine
//! driver `sound/soc/intel/avs/boards/max98373.c` drives them (boards/max98373.c:132 DSP_B + NB_NF; :76,:83 TDM 8 slots of 16 bits; :58-63 the back end is fixed to 48 kHz, stereo, S16).
//!
//! THE ORDER OF A SPEAKER SESSION, and why it differs from Linux in exactly two places (both stricter, both named):
//!
//! 1. [`configure`]: reset, probe defaults, the DAI format, then ChromeOS's UCM PROTECTION SET ([`crate::ctl::VERB`], then
//!    [`crate::ctl::DEVICE`]) and a READBACK of every register it set. No audio clock is needed for any of it.
//! 2. [`power_up`]: only after the readback held AND the DSP's pipelines are running (so BCLK and the frame clock exist): `SPK_EN` (Linux sets
//!    it in probe, max98373.c:424, long before anything is protected; here it waits for the protection), the DAPM `Amp Enable` widget's
//!    register `PCM_RX_EN`, then its POST_PMU event, `GLOBAL_EN` and the 30 ms settle (:26-32). Linux enables before the clocks; the part
//!    has a clock monitor with auto-restart (`R20FE` = 0xF, :407) so both orders work, and this one never runs the part without a clock.
//! 3. [`power_down`]: the PRE_PMD event first, `GLOBAL_EN` off and the 30 ms settle (:33-39), then `PCM_RX_EN` off, then a readback. Linux's
//!    UCM DisableSequence would also switch DHT and BDE OFF here; this does not: a protection is never turned off while the part could be
//!    enabled, and the amplifier goes back into its hardware reset right after.
extern crate alloc;

use crate::ctl::{self, Cset, CtlError, Kind, Side};
use crate::regs::*;
use alloc::vec::Vec;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `regmap_write`.
    Write { reg: u16, value: u8 },
    /// `regmap_update_bits`: `reg = (reg & !mask) | (value & mask)`.
    Update { reg: u16, mask: u8, value: u8 },
    /// Read back and require `(value & mask) == want`.
    Verify { reg: u16, mask: u8, want: u8 },
    /// A read that must be answered (the identity read of `max98373_reset`, :341-347); the value is reported, not judged.
    Read { reg: u16 },
    /// `usleep_range` / `msleep`, rounded up to whole milliseconds.
    DelayMs(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// The control or event this step is, for the failure it names.
    pub label: &'static str,
    pub op: Op,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    OutOfMemory,
    /// A UCM value does not fit its control: a table bug, found on the host.
    Ctl { ctl: &'static str, why: CtlError },
}

impl PlanError {
    pub const fn name(&self) -> &'static str {
        match self {
            PlanError::OutOfMemory => "amp-plan-out-of-memory",
            PlanError::Ctl { .. } => "amp-ucm-value",
        }
    }
}

/// A plan under construction: every push can fail (D10: no fixed capacity, a named refusal).
pub struct Plan(Vec<Step>);

impl Plan {
    pub fn new() -> Self {
        Plan(Vec::new())
    }
    pub fn push(&mut self, label: &'static str, op: Op) -> Result<(), PlanError> {
        self.0.try_reserve(1).map_err(|_| PlanError::OutOfMemory)?;
        self.0.push(Step { label, op });
        Ok(())
    }
    pub fn steps(&self) -> &[Step] {
        &self.0
    }
    pub fn into_steps(self) -> Vec<Step> {
        self.0
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

/// Which amplifier, and its board wiring. The Atlas values are the coreboot devicetree's (`variants/atlas/devicetree.cb:402-417`: MAXR uid 0
/// at 0x31, vmon slot 4, imon slot 5; MAXL uid 1 at 0x32, slots 6 and 7) and the machine driver's (`boards/max98373.c:23-28`: device 0 is the
/// `Right` prefix, device 1 the `Left`; `:76`, `:83`: the TDM transmit masks 0x30 and 0xC0 are exactly those slot pairs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Amp {
    pub side: Side,
    pub addr: u16,
    pub vmon_slot: u8,
    pub imon_slot: u8,
}

/// The speaker-feedback slot when the firmware names none (`max98373_slot_config`, max98373.c:501-504).
pub const SPKFB_SLOT_DEFAULT: u8 = 2; // max98373.c:504
/// The receive slots both amplifiers listen to (`snd_soc_dai_set_tdm_slot(dai, tx, 3, 8, 16)`, boards/max98373.c:76,83): slots 0 and 1.
pub const TDM_RX_MASK: u16 = 0x3;
pub const TDM_SLOTS: u32 = 8;
pub const TDM_SLOT_WIDTH: u32 = 16;

impl Amp {
    /// The transmit (voltage and current feedback) slots: one bit each for `vmon_slot` and `imon_slot`.
    pub const fn tx_mask(&self) -> u16 {
        (1u16 << self.vmon_slot) | (1u16 << self.imon_slot)
    }
}

/// The Pixelbook Go: the right amplifier first (device 0), then the left.
pub const ATLAS_AMPS: [Amp; 2] = [
    Amp { side: Side::Right, addr: 0x31, vmon_slot: 4, imon_slot: 5 },
    Amp { side: Side::Left, addr: 0x32, vmon_slot: 6, imon_slot: 7 },
];

/// The two amplifiers of `board_amps` if (and only if) the board record lists exactly the Pixelbook Go's two addresses.
pub fn atlas_amps(board_amps: &[u16]) -> Option<[Amp; 2]> {
    if board_amps.len() == 2 && ATLAS_AMPS.iter().all(|a| board_amps.contains(&a.addr)) {
        Some(ATLAS_AMPS)
    } else {
        None
    }
}

/// `max98373_get_bclk_sel` (max98373-i2c.c:177-186): BCLKs per LRCLK -> the field value (index + 2), 0 when unsupported.
pub const BCLK_SEL_TABLE: [u16; 10] = [32, 48, 64, 96, 128, 192, 256, 384, 512, 320]; // max98373-i2c.c:173-175
pub fn bclk_sel(bclk_per_lrclk: u32) -> Option<u8> {
    BCLK_SEL_TABLE.iter().position(|b| *b as u32 == bclk_per_lrclk).map(|i| i as u8 + 2)
}

fn ucm_step(plan: &mut Plan, c: Cset) -> Result<(), PlanError> {
    let ctl = ctl::find(c.ctl).ok_or(PlanError::Ctl { ctl: c.ctl, why: CtlError::Unknown })?;
    if ctl.kind == Kind::Pin {
        // `Spk Switch`: a DAPM pin. Its register effect is the power sequence (power_up), which only runs for a pin that is ON.
        return Ok(());
    }
    let (mask, value) = ctl.encode(c.value).map_err(|why| PlanError::Ctl { ctl: c.ctl, why })?;
    plan.push(ctl.name, Op::Update { reg: ctl.reg, mask, value })
}

/// `max98373_reset` (max98373.c:325-350): the software reset, the 10 ms the driver sleeps before it reads, and the revision read that tells
/// the part answers.
fn reset(plan: &mut Plan) -> Result<(), PlanError> {
    plan.push("soft-reset", Op::Write { reg: R2000_SW_RESET, value: SOFT_RESET })?;
    plan.push("reset-settle", Op::DelayMs(10))?; // max98373.c:339 usleep_range(10000, 11000)
    plan.push("rev-id", Op::Read { reg: R21FF_REV_ID })
}

/// `max98373_probe` (max98373.c:353-426) for one amplifier, less the final `SPK_EN` (see the module comment) and the interleave branch
/// (the AVS card does not set `maxim,interleave_mode`).
fn probe(plan: &mut Plan, amp: &Amp, spkfb_slot: u8) -> Result<(), PlanError> {
    plan.push("pcm-tx-hiz-1", Op::Write { reg: R2020_PCM_TX_HIZ_EN_1, value: 0xFF })?; // max98373.c:362
    plan.push("pcm-tx-hiz-2", Op::Write { reg: R2021_PCM_TX_HIZ_EN_2, value: 0xFF })?; // max98373.c:365
    plan.push("mono-mix-1", Op::Write { reg: R2029_PCM_TO_SPK_MONO_MIX_1, value: 0x80 })?; // max98373.c:369
    plan.push("mono-mix-2", Op::Write { reg: R202A_PCM_TO_SPK_MONO_MIX_2, value: 0x01 })?; // max98373.c:372
    plan.push("dc-blocker", Op::Write { reg: R203F_AMP_DSP_CFG, value: 0x03 })?; // max98373.c:376
    plan.push("iv-dc-blocker", Op::Write { reg: R2046_IV_SENSE_ADC_DSP_CFG, value: 0x07 })?; // max98373.c:380
    let slots = (amp.imon_slot << PCM_TX_CH_SRC_A_I_SHIFT) | amp.vmon_slot; // max98373.c:384-385 (the `& 0xFF`)
    plan.push("tx-src-1", Op::Write { reg: R2022_PCM_TX_SRC_1, value: slots })?;
    for (label, slot) in [("vmon-slot-enable", amp.vmon_slot), ("imon-slot-enable", amp.imon_slot)] {
        // max98373.c:388-403 - the slot's Hi-Z bit is cleared in the register half it belongs to
        let (reg, bit) = if slot < 8 { (R2020_PCM_TX_HIZ_EN_1, slot) } else { (R2021_PCM_TX_HIZ_EN_2, slot - 8) };
        plan.push(label, Op::Update { reg, mask: 1 << bit, value: 0 })?;
    }
    plan.push("auto-restart", Op::Write { reg: R20FE_DEVICE_AUTO_RESTART_CFG, value: 0xF })?; // max98373.c:407
    plan.push("tx-src-2", Op::Write { reg: R2023_PCM_TX_SRC_2, value: spkfb_slot }) // max98373.c:412
}

/// `set_fmt` + the machine driver's `hw_params` TDM call + the codec's `hw_params`, in the order soc-pcm runs them (the link's format first,
/// the machine's `hw_params` before the codec's) for the one stream the topology offers: S16 at 48 kHz, DSP_B, 8 x 16-bit slots.
fn dai(plan: &mut Plan, amp: &Amp) -> Result<(), PlanError> {
    // max98373_dai_set_fmt (max98373-i2c.c:121-171): SND_SOC_DAIFMT_NB_NF clears BCLKEDGE; DSP_B is TDM mode 0
    plan.push("dai-bclk-edge", Op::Update { reg: R2026_PCM_CLOCK_RATIO, mask: PCM_MODE_CFG_PCM_BCLKEDGE, value: 0 })?;
    plan.push("dai-format", Op::Update { reg: R2024_PCM_DATA_FMT_CFG, mask: PCM_MODE_CFG_FORMAT_MASK, value: PCM_FORMAT_TDM_MODE0 << PCM_MODE_CFG_FORMAT_SHIFT })?;
    // max98373_dai_tdm_slot (max98373-i2c.c:317-401), sets tdm_mode so hw_params leaves BSEL alone
    let bsel = bclk_sel(TDM_SLOTS * TDM_SLOT_WIDTH).ok_or(PlanError::Ctl { ctl: "bclk-sel", why: CtlError::Unknown })?;
    plan.push("tdm-bclk", Op::Update { reg: R2026_PCM_CLOCK_RATIO, mask: PCM_CLK_SETUP_BSEL_MASK, value: bsel })?;
    plan.push("tdm-chan-size", Op::Update { reg: R2024_PCM_DATA_FMT_CFG, mask: PCM_MODE_CFG_CHANSZ_MASK, value: PCM_MODE_CFG_CHANSZ_16 })?;
    // rx slots: the lowest set bit feeds channel 0 (R2029 low nibble), the second R202A (max98373-i2c.c:367-384)
    let mut found = 0;
    for x in 0..16u8 {
        if TDM_RX_MASK & (1 << x) != 0 {
            if found == 0 {
                plan.push("tdm-rx-slot-0", Op::Update { reg: R2029_PCM_TO_SPK_MONO_MIX_1, mask: PCM_TO_SPK_CH0_SRC_MASK, value: x })?;
            } else {
                plan.push("tdm-rx-slot-1", Op::Write { reg: R202A_PCM_TO_SPK_MONO_MIX_2, value: x })?;
            }
            found += 1;
            if found > 1 {
                break;
            }
        }
    }
    let tx = amp.tx_mask();
    plan.push("tdm-tx-hiz-1", Op::Write { reg: R2020_PCM_TX_HIZ_EN_1, value: !tx as u8 })?; // max98373-i2c.c:386-390 (~tx_mask & 0xFF)
    plan.push("tdm-tx-hiz-2", Op::Write { reg: R2021_PCM_TX_HIZ_EN_2, value: ((!tx & 0xFF00) >> 8) as u8 })?; // max98373-i2c.c:391-394
    // max98373_dai_hw_params (max98373-i2c.c:213-315), S16 / 48 kHz
    plan.push("hw-chan-size", Op::Update { reg: R2024_PCM_DATA_FMT_CFG, mask: PCM_MODE_CFG_CHANSZ_MASK, value: PCM_MODE_CFG_CHANSZ_16 })?;
    plan.push("hw-sample-rate", Op::Update { reg: R2027_PCM_SR_SETUP_1, mask: PCM_SR_SET1_SR_MASK, value: PCM_SR_SET1_SR_48000 })?;
    plan.push("hw-sample-rate-2", Op::Update { reg: R2028_PCM_SR_SETUP_2, mask: PCM_SR_SET2_SR_MASK, value: PCM_SR_SET1_SR_48000 << PCM_SR_SET2_SR_SHIFT })?;
    plan.push("hw-iv-sample-rate", Op::Update { reg: R2028_PCM_SR_SETUP_2, mask: PCM_SR_SET2_IVADC_SR_MASK, value: PCM_SR_SET1_SR_48000 })
}

/// The register image of one UCM list for one amplifier, in the file's order, as `(label, reg, mask, value)`.
pub fn ucm_writes(list: &[ctl::Set], side: Side) -> Result<Vec<(&'static str, u16, u8, u8)>, PlanError> {
    let mut out = Vec::new();
    for s in list {
        let ctl = ctl::find(s.ctl).ok_or(PlanError::Ctl { ctl: s.ctl, why: CtlError::Unknown })?;
        if ctl.kind == Kind::Pin {
            continue;
        }
        let (mask, value) = ctl.encode(s.value(side)).map_err(|why| PlanError::Ctl { ctl: s.ctl, why })?;
        out.try_reserve(1).map_err(|_| PlanError::OutOfMemory)?;
        out.push((ctl.name, ctl.reg, mask, value));
    }
    Ok(out)
}

/// What the bring-up does to ONE amplifier, in order: reset, probe defaults, DAI format. (The protection set follows in [`ucm_plan`], for the
/// pair, so the UCM's interleaved Left/Right order is kept.)
pub fn bring_up(amp: &Amp) -> Result<Plan, PlanError> {
    let mut p = Plan::new();
    reset(&mut p)?;
    probe(&mut p, amp, SPKFB_SLOT_DEFAULT)?;
    dai(&mut p, amp)?;
    Ok(p)
}

/// One cset of the UCM, as the step that applies it to the amplifier the side names.
pub fn cset_step(c: Cset) -> Result<Option<Step>, PlanError> {
    let mut p = Plan::new();
    ucm_step(&mut p, c)?;
    Ok(p.into_steps().into_iter().next())
}

/// The UCM lists, applied: every cset in file order, as `(side, step)` (a pin switch yields no step). `VERB` then `DEVICE`.
pub fn ucm_plan() -> Result<Vec<(Side, Step)>, PlanError> {
    let mut out = Vec::new();
    for list in [&ctl::VERB[..], &ctl::DEVICE[..]] {
        for c in ctl::csets(list) {
            if let Some(step) = cset_step(c)? {
                out.try_reserve(1).map_err(|_| PlanError::OutOfMemory)?;
                out.push((c.side, step));
            }
        }
    }
    Ok(out)
}

/// The readback of everything [`ucm_plan`] set: each register field must read as it was written. A register shared by several controls is
/// verified under the union of their masks at the value the LAST write left (`ucm_plan` order), because that is what the part holds.
pub fn ucm_verify(side: Side) -> Result<Vec<Step>, PlanError> {
    let mut want: Vec<(&'static str, u16, u8, u8)> = Vec::new();
    for list in [&ctl::VERB[..], &ctl::DEVICE[..]] {
        for (label, reg, mask, value) in ucm_writes(list, side)? {
            if let Some(e) = want.iter_mut().find(|e| e.1 == reg) {
                e.2 |= mask;
                e.3 = (e.3 & !mask) | value;
                e.0 = label;
            } else {
                want.try_reserve(1).map_err(|_| PlanError::OutOfMemory)?;
                want.push((label, reg, mask, value));
            }
        }
    }
    let mut p = Plan::new();
    for (label, reg, mask, value) in want {
        p.push(label, Op::Verify { reg, mask, want: value })?;
    }
    Ok(p.into_steps())
}

/// Bring the amplifier up from its Hi-Z DAPM state to playing, once the clocks run and the protection has been read back:
/// `SPK_EN` (max98373.c:424), `Amp Enable`'s register `PCM_RX_EN` (max98373.c:64-66), then the POST_PMU event `GLOBAL_EN` (:26-32) and its settle.
pub fn power_up() -> Result<Plan, PlanError> {
    let mut p = Plan::new();
    p.push("spk-enable", Op::Update { reg: R2043_AMP_EN, mask: SPK_EN_MASK, value: SPK_EN_MASK })?;
    p.push("pcm-rx-enable", Op::Update { reg: R202B_PCM_RX_EN, mask: 1, value: 1 })?;
    p.push("global-enable", Op::Update { reg: R20FF_GLOBAL_SHDN, mask: GLOBAL_EN_MASK, value: GLOBAL_EN_MASK })?;
    p.push("global-enable-settle", Op::DelayMs(30))?; // max98373.c:31 usleep_range(30000, 31000)
    p.push("global-enable-readback", Op::Verify { reg: R20FF_GLOBAL_SHDN, mask: GLOBAL_EN_MASK, want: GLOBAL_EN_MASK })?;
    Ok(p)
}

/// The PRE_PMD event and the widget's register (max98373.c:33-39, :64-66), then the proof.
pub fn power_down() -> Result<Plan, PlanError> {
    let mut p = Plan::new();
    p.push("global-disable", Op::Update { reg: R20FF_GLOBAL_SHDN, mask: GLOBAL_EN_MASK, value: 0 })?;
    p.push("global-disable-settle", Op::DelayMs(30))?; // max98373.c:37 usleep_range(30000, 31000)
    p.push("pcm-rx-disable", Op::Update { reg: R202B_PCM_RX_EN, mask: 1, value: 0 })?;
    for c in SAFE_CHECKS {
        p.push(c.label, Op::Verify { reg: c.reg, mask: c.mask, want: c.want })?;
    }
    Ok(p)
}

/// One readback requirement: `(value & mask) == want`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Check {
    pub label: &'static str,
    pub reg: u16,
    pub mask: u8,
    pub want: u8,
}

/// What "safe" means for one amplifier that is reachable on the bus: not globally enabled, no PCM receive. (The strongest safe state, the
/// hardware reset, is the pad's and is proven by the pad's readback, not by this bus.)
pub const SAFE_CHECKS: [Check; 2] = [
    Check { label: "global-disabled", reg: R20FF_GLOBAL_SHDN, mask: GLOBAL_EN_MASK, want: 0 },
    Check { label: "pcm-rx-disabled", reg: R202B_PCM_RX_EN, mask: 1, want: 0 },
];

/// The one write a dying task makes without allocating: `GLOBAL_EN` off (a plain write of the whole byte: the register holds nothing else,
/// max98373.h:204). Followed, always, by the pad's reset.
pub const EMERGENCY_WRITE: (u16, u8) = (R20FF_GLOBAL_SHDN, 0x00);

/// The I2C message forms (`regmap` with `reg_bits = 16`, `val_bits = 8`, max98373-i2c.c:`max98373_regmap`; the register goes out big-endian).
pub mod wire {
    /// A register write: `[reg_hi, reg_lo, value]`.
    pub const fn write(reg: u16, value: u8) -> [u8; 3] {
        [(reg >> 8) as u8, reg as u8, value]
    }
    /// The address phase of a register read: `[reg_hi, reg_lo]`, then one byte is read back.
    pub const fn read_addr(reg: u16) -> [u8; 2] {
        [(reg >> 8) as u8, reg as u8]
    }
    /// Is this message a register write the amplifier plans can produce? (`reg` is the register it writes.)
    pub fn decode_write(msg: &[u8]) -> Option<(u16, u8)> {
        match msg {
            [hi, lo, v] => Some((u16::from_be_bytes([*hi, *lo]), *v)),
            _ => None,
        }
    }
    /// The register an address phase names.
    pub fn decode_read_addr(msg: &[u8]) -> Option<u16> {
        match msg {
            [hi, lo] => Some(u16::from_be_bytes([*hi, *lo])),
            _ => None,
        }
    }
}

/// Every register any plan of this crate WRITES (the I2C fence's write set): derived from the plans themselves, so a plan that writes a new
/// register cannot be forgotten. The device's allow-list is built from this; `tests/fence.rs` runs it against a hand-written literal.
pub fn written_registers(amps: &[Amp]) -> Result<Vec<u16>, PlanError> {
    let mut regs: Vec<u16> = Vec::new();
    let mut add = |op: &Op| -> Result<(), PlanError> {
        let reg = match op {
            Op::Write { reg, .. } | Op::Update { reg, .. } => *reg,
            _ => return Ok(()),
        };
        if !regs.contains(&reg) {
            regs.try_reserve(1).map_err(|_| PlanError::OutOfMemory)?;
            regs.push(reg);
        }
        Ok(())
    };
    for a in amps {
        for s in bring_up(a)?.steps() {
            add(&s.op)?;
        }
    }
    for (_, s) in ucm_plan()? {
        add(&s.op)?;
    }
    for s in power_up()?.steps().iter().chain(power_down()?.steps()) {
        add(&s.op)?;
    }
    add(&Op::Write { reg: EMERGENCY_WRITE.0, value: EMERGENCY_WRITE.1 })?;
    regs.sort_unstable();
    Ok(regs)
}
