// SPDX-License-Identifier: GPL-2.0-only
//! The mixer controls ChromeOS's UCM sets for the Pixelbook Go's speakers, and the registers they are. Ported from Linux
//! `sound/soc/codecs/max98373.c` (`max98373_snd_controls`, `max98373_dapm_widgets`; Copyright (c) 2017 Maxim Integrated, `GPL-2.0`) and
//! `sound/soc/intel/avs/boards/max98373.c` (the card's `Left Spk` / `Right Spk` pin switches).
//!
//! A control is a register field and a rule for turning the UCM's number into field bits - exactly ALSA's `soc_mixer_control` /
//! `soc_enum` arithmetic (`snd_soc_put_volsw`: the value must not exceed `max`, an inverted control stores `max - value`, the mask is
//! `(1 << fls(max)) - 1` shifted into place; `snd_soc_put_enum_double`: the index must be below `items`, the mask is
//! `roundup_pow_of_two(items) - 1`). Nothing here talks to hardware.
//!
//! THE UCM (`usr/share/alsa/ucm/avs_max98373/HiFi.conf` of the ChromeOS recovery image for ATLAS; sha256 in `docs`/DECISIONS D17) is two
//! lists: [`VERB`], its `SectionVerb.EnableSequence` (68 `cset`s, every one a PROTECTION or a gain the DSM-free path needs, with the
//! speaker pin OFF), and [`DEVICE`], `SectionDevice."Speaker".EnableSequence` (ten `cset`s: the output voltage that goes with the speaker,
//! DHT, BDE and VI sense ON, the speaker pin ON). The two `cset-tlv` blobs of the verb (`DSM data`, `WAVES data`) are NOT here: DSM is the
//! named later upgrade (DECISIONS D17). Left then Right for every control, in the file's order.
use crate::regs;

/// What a control is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `SOC_SINGLE*`: a number `0..=max` in a field.
    Single,
    /// `SOC_ENUM` / `SOC_DAPM_ENUM`: an index `0..max` (`max` is the item count).
    Enum,
    /// A DAPM pin switch: no register. Its state decides whether the amplifier's DAPM path may be powered.
    Pin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ctl {
    /// The control's name without the card's `Left ` / `Right ` prefix.
    pub name: &'static str,
    pub kind: Kind,
    pub reg: u16,
    pub shift: u8,
    /// `Single`: the largest value; `Enum`: the number of items.
    pub max: u8,
    pub invert: bool,
}

/// Why a UCM number does not fit its control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CtlError {
    /// A value above `max` (`snd_soc_put_volsw`: -EINVAL), or an enum index at or past `items`.
    OutOfRange { value: u8, max: u8 },
    /// A pin switch has no register.
    NoRegister,
    /// No control of that name.
    Unknown,
}

/// `fls(x)`: the 1-based index of the highest set bit, 0 for 0.
const fn fls(x: u8) -> u32 {
    8 - x.leading_zeros()
}

/// `roundup_pow_of_two`.
const fn roundup_pow2(x: u8) -> u16 {
    let mut p: u16 = 1;
    while p < x as u16 {
        p <<= 1;
    }
    p
}

impl Ctl {
    /// The register bits this control owns.
    pub const fn mask(&self) -> u8 {
        let m: u16 = match self.kind {
            Kind::Single => (1u16 << fls(self.max)) - 1,
            Kind::Enum => roundup_pow2(self.max) - 1,
            Kind::Pin => 0,
        };
        ((m << self.shift) & 0xFF) as u8
    }

    /// `(mask, bits)` for a register update that sets this control to `value`.
    pub fn encode(&self, value: u8) -> Result<(u8, u8), CtlError> {
        match self.kind {
            Kind::Pin => Err(CtlError::NoRegister),
            Kind::Single => {
                if value > self.max {
                    return Err(CtlError::OutOfRange { value, max: self.max });
                }
                let stored = if self.invert { self.max - value } else { value };
                Ok((self.mask(), stored << self.shift))
            }
            Kind::Enum => {
                if value >= self.max {
                    return Err(CtlError::OutOfRange { value, max: self.max });
                }
                Ok((self.mask(), value << self.shift))
            }
        }
    }
}

/// Look a control up by name.
pub fn find(name: &str) -> Option<&'static Ctl> {
    CTLS.iter().find(|c| c.name == name)
}

/// One UCM line pair: the control, and the value for the LEFT amplifier and for the RIGHT one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Set {
    pub ctl: &'static str,
    pub left: u8,
    pub right: u8,
}

/// Every control the UCM sets. Registers are `crate::regs`, fields and ranges are the Linux table's.
pub const CTLS: [Ctl; 37] = [
    Ctl { name: "Digital Vol Sel Switch", kind: Kind::Single, reg: regs::R203F_AMP_DSP_CFG, shift: 7, max: 1, invert: false }, // max98373.c:200
    Ctl { name: "Digital Volume", kind: Kind::Single, reg: regs::R203D_AMP_DIG_VOL_CTRL, shift: 0, max: 127, invert: true }, // max98373.c:223
    Ctl { name: "Speaker Volume", kind: Kind::Single, reg: regs::R203E_AMP_PATH_GAIN, shift: 4, max: 10, invert: false }, // max98373.c:225
    Ctl { name: "Output Voltage", kind: Kind::Enum, reg: regs::R203E_AMP_PATH_GAIN, shift: 0, max: 10, invert: false }, // max98373.c:229
    Ctl { name: "DHT Switch", kind: Kind::Single, reg: regs::R20D4_DHT_EN, shift: 0, max: 1, invert: false }, // max98373.c:231
    Ctl { name: "DHT Min Volume", kind: Kind::Single, reg: regs::R20D1_DHT_CFG, shift: 4, max: 9, invert: false }, // max98373.c:233
    Ctl { name: "DHT Rot Pnt Volume", kind: Kind::Single, reg: regs::R20D1_DHT_CFG, shift: 0, max: 15, invert: true }, // max98373.c:235
    Ctl { name: "DHT Attack Step Volume", kind: Kind::Single, reg: regs::R20D2_DHT_ATTACK_CFG, shift: 3, max: 4, invert: false }, // max98373.c:237
    Ctl { name: "DHT Release Step Volume", kind: Kind::Single, reg: regs::R20D3_DHT_RELEASE_CFG, shift: 3, max: 4, invert: false }, // max98373.c:239
    Ctl { name: "DHT Attack Rate", kind: Kind::Enum, reg: regs::R20D2_DHT_ATTACK_CFG, shift: 0, max: 8, invert: false }, // max98373.c:241
    Ctl { name: "DHT Release Rate", kind: Kind::Enum, reg: regs::R20D3_DHT_RELEASE_CFG, shift: 0, max: 8, invert: false }, // max98373.c:242
    Ctl { name: "BDE Switch", kind: Kind::Single, reg: regs::R20B5_BDE_EN, shift: 0, max: 1, invert: false }, // max98373.c:259
    Ctl { name: "BDE LVL4 Mute Switch", kind: Kind::Single, reg: regs::R20B2_BDE_L4_CFG_2, shift: 7, max: 1, invert: false }, // max98373.c:260
    Ctl { name: "BDE LVL4 Hold Switch", kind: Kind::Single, reg: regs::R20B2_BDE_L4_CFG_2, shift: 6, max: 1, invert: false }, // max98373.c:262
    Ctl { name: "BDE LVL1 Thresh", kind: Kind::Single, reg: regs::R2097_BDE_L1_THRESH, shift: 0, max: 255, invert: false }, // max98373.c:264
    Ctl { name: "BDE LVL2 Thresh", kind: Kind::Single, reg: regs::R2098_BDE_L2_THRESH, shift: 0, max: 255, invert: false }, // max98373.c:265
    Ctl { name: "BDE LVL3 Thresh", kind: Kind::Single, reg: regs::R2099_BDE_L3_THRESH, shift: 0, max: 255, invert: false }, // max98373.c:266
    Ctl { name: "BDE LVL4 Thresh", kind: Kind::Single, reg: regs::R209A_BDE_L4_THRESH, shift: 0, max: 255, invert: false }, // max98373.c:267
    Ctl { name: "BDE Thresh Hysteresis", kind: Kind::Single, reg: regs::R209B_BDE_THRESH_HYST, shift: 0, max: 255, invert: false }, // max98373.c:271
    Ctl { name: "BDE Hold Time", kind: Kind::Single, reg: regs::R2090_BDE_LVL_HOLD, shift: 0, max: 255, invert: false }, // max98373.c:272
    Ctl { name: "BDE Attack Rate", kind: Kind::Single, reg: regs::R2091_BDE_GAIN_ATK_REL_RATE, shift: 4, max: 15, invert: false }, // max98373.c:273
    Ctl { name: "BDE Release Rate", kind: Kind::Single, reg: regs::R2091_BDE_GAIN_ATK_REL_RATE, shift: 0, max: 15, invert: false }, // max98373.c:274
    Ctl { name: "BDE LVL1 Clip Thresh Volume", kind: Kind::Single, reg: regs::R20A9_BDE_L1_CFG_2, shift: 0, max: 60, invert: true }, // max98373.c:275
    Ctl { name: "BDE LVL2 Clip Thresh Volume", kind: Kind::Single, reg: regs::R20AC_BDE_L2_CFG_2, shift: 0, max: 60, invert: true }, // max98373.c:277
    Ctl { name: "BDE LVL3 Clip Thresh Volume", kind: Kind::Single, reg: regs::R20AF_BDE_L3_CFG_2, shift: 0, max: 60, invert: true }, // max98373.c:279
    Ctl { name: "BDE LVL4 Clip Thresh Volume", kind: Kind::Single, reg: regs::R20B2_BDE_L4_CFG_2, shift: 0, max: 60, invert: true }, // max98373.c:281
    Ctl { name: "BDE LVL1 Clip Reduction Volume", kind: Kind::Single, reg: regs::R20AA_BDE_L1_CFG_3, shift: 0, max: 60, invert: true }, // max98373.c:283
    Ctl { name: "BDE LVL2 Clip Reduction Volume", kind: Kind::Single, reg: regs::R20AD_BDE_L2_CFG_3, shift: 0, max: 60, invert: true }, // max98373.c:285
    Ctl { name: "BDE LVL3 Clip Reduction Volume", kind: Kind::Single, reg: regs::R20B0_BDE_L3_CFG_3, shift: 0, max: 60, invert: true }, // max98373.c:287
    Ctl { name: "BDE LVL4 Clip Reduction Volume", kind: Kind::Single, reg: regs::R20B3_BDE_L4_CFG_3, shift: 0, max: 60, invert: true }, // max98373.c:289
    Ctl { name: "BDE LVL1 Limiter Thresh Volume", kind: Kind::Single, reg: regs::R20A8_BDE_L1_CFG_1, shift: 0, max: 15, invert: true }, // max98373.c:291
    Ctl { name: "BDE LVL2 Limiter Thresh Volume", kind: Kind::Single, reg: regs::R20AB_BDE_L2_CFG_1, shift: 0, max: 15, invert: true }, // max98373.c:293
    Ctl { name: "BDE LVL3 Limiter Thresh Volume", kind: Kind::Single, reg: regs::R20AE_BDE_L3_CFG_1, shift: 0, max: 15, invert: true }, // max98373.c:295
    Ctl { name: "BDE LVL4 Limiter Thresh Volume", kind: Kind::Single, reg: regs::R20B1_BDE_L4_CFG_1, shift: 0, max: 15, invert: true }, // max98373.c:297
    Ctl { name: "DAI Sel Mux", kind: Kind::Enum, reg: regs::R2029_PCM_TO_SPK_MONO_MIX_1, shift: 6, max: 3, invert: false }, // max98373.c:50-55 (SOC_DAPM_ENUM over R2029 bits 7:6; items Left/Right/LeftRight, :46-47)
    Ctl { name: "VI Sense Switch", kind: Kind::Single, reg: regs::R202C_PCM_TX_EN, shift: 0, max: 1, invert: false }, // max98373.c:57-58 (SOC_DAPM_SINGLE on R202C bit 0)
    Ctl { name: "Spk Switch", kind: Kind::Pin, reg: 0, shift: 0, max: 1, invert: false }, // boards/max98373.c:34-37 (SOC_DAPM_PIN_SWITCH: a DAPM pin, no register)
];

/// `SectionVerb.EnableSequence` minus the two `cset-tlv` blobs, in the file's order (Left, then Right, for each control).
/// Enum values are the item index: `DAI Sel Mux` Left = 0, Right = 1 (max98373.c:46-47); `Spk Switch` off = 0.
pub const VERB: [Set; 34] = [
    Set { ctl: "DAI Sel Mux", left: 0, right: 1 },
    Set { ctl: "Output Voltage", left: 4, right: 4 },
    Set { ctl: "Speaker Volume", left: 0, right: 0 },
    Set { ctl: "Digital Volume", left: 127, right: 127 },
    Set { ctl: "DHT Min Volume", left: 0, right: 0 },
    Set { ctl: "DHT Rot Pnt Volume", left: 2, right: 2 },
    Set { ctl: "DHT Attack Step Volume", left: 0, right: 0 },
    Set { ctl: "DHT Attack Rate", left: 2, right: 2 },
    Set { ctl: "DHT Release Step Volume", left: 0, right: 0 },
    Set { ctl: "DHT Release Rate", left: 3, right: 3 },
    Set { ctl: "BDE LVL1 Thresh", left: 120, right: 120 },
    Set { ctl: "BDE LVL1 Clip Reduction Volume", left: 58, right: 58 },
    Set { ctl: "BDE LVL1 Clip Thresh Volume", left: 60, right: 60 },
    Set { ctl: "BDE LVL1 Limiter Thresh Volume", left: 14, right: 14 },
    Set { ctl: "BDE LVL2 Thresh", left: 117, right: 117 },
    Set { ctl: "BDE LVL2 Clip Reduction Volume", left: 58, right: 58 },
    Set { ctl: "BDE LVL2 Clip Thresh Volume", left: 60, right: 60 },
    Set { ctl: "BDE LVL2 Limiter Thresh Volume", left: 13, right: 13 },
    Set { ctl: "BDE LVL3 Thresh", left: 114, right: 114 },
    Set { ctl: "BDE LVL3 Clip Reduction Volume", left: 56, right: 56 },
    Set { ctl: "BDE LVL3 Clip Thresh Volume", left: 60, right: 60 },
    Set { ctl: "BDE LVL3 Limiter Thresh Volume", left: 12, right: 12 },
    Set { ctl: "BDE LVL4 Thresh", left: 112, right: 112 },
    Set { ctl: "BDE LVL4 Mute Switch", left: 0, right: 0 },
    Set { ctl: "BDE LVL4 Hold Switch", left: 0, right: 0 },
    Set { ctl: "BDE LVL4 Clip Reduction Volume", left: 55, right: 55 },
    Set { ctl: "BDE LVL4 Clip Thresh Volume", left: 60, right: 60 },
    Set { ctl: "BDE LVL4 Limiter Thresh Volume", left: 11, right: 11 },
    Set { ctl: "BDE Hold Time", left: 100, right: 100 },
    Set { ctl: "BDE Thresh Hysteresis", left: 2, right: 2 },
    Set { ctl: "BDE Attack Rate", left: 0, right: 0 },
    Set { ctl: "BDE Release Rate", left: 5, right: 5 },
    Set { ctl: "Digital Vol Sel Switch", left: 1, right: 1 },
    Set { ctl: "Spk Switch", left: 0, right: 0 },
];

/// `SectionDevice."Speaker".EnableSequence`: what a speaker device adds. `Spk Switch` on = 1.
pub const DEVICE: [Set; 5] = [
    Set { ctl: "Output Voltage", left: 5, right: 5 },
    Set { ctl: "DHT Switch", left: 1, right: 1 },
    Set { ctl: "BDE Switch", left: 1, right: 1 },
    Set { ctl: "VI Sense Switch", left: 1, right: 1 },
    Set { ctl: "Spk Switch", left: 1, right: 1 },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    pub const fn prefix(self) -> &'static str {
        match self {
            Side::Left => "Left",
            Side::Right => "Right",
        }
    }
}

impl Set {
    pub const fn value(&self, side: Side) -> u8 {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }
}

/// A UCM `cset` as the file writes it: `(side, control, value)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cset {
    pub side: Side,
    pub ctl: &'static str,
    pub value: u8,
}

/// The csets of a list in the UCM's order: for each control, Left then Right.
pub fn csets<'a>(list: &'a [Set]) -> impl Iterator<Item = Cset> + 'a {
    list.iter().flat_map(|s| [Cset { side: Side::Left, ctl: s.ctl, value: s.left }, Cset { side: Side::Right, ctl: s.ctl, value: s.right }])
}

/// Is every register-backed control of `regs` in the table listed in `regs.rs`? (Compile-time reachability only; kept so `regs` is used.)
pub const _REGS_ARE_THE_CRATES: u16 = regs::R21FF_REV_ID;
