// SPDX-License-Identifier: GPL-2.0-only
//! Digital audio interface: format (`da7219_set_dai_fmt`, da7219.c:1308-1392), sample rate (`da7219_set_sr`, da7219.c:1511-1557) and
//! the parameters of `da7219_hw_params` (da7219.c:1559-1668), for the I2S link on SSP1.
//!
//! On Atlas the AVS machine driver sets `SND_SOC_DAIFMT_I2S | SND_SOC_DAIFMT_NB_NF | SND_SOC_DAIFMT_CBC_CFC` (avs/boards/da7219.c:199):
//! standard I2S, normal bit and frame clock, and the codec is the clock CONSUMER (the SSP provides BCLK and WCLK). In consumer mode the
//! codec detects the BCLK rate itself and `hw_params` does not program BCLKs-per-WCLK (the `da7219->master` branch of da7219.c:1559-1668).
use crate::bus::*;
use crate::fields::*;
use crate::regs::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    I2s,
    LeftJ,
    RightJ,
    DspB,
}

/// `SND_SOC_DAIFMT_*_*F` clock inversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Inversion {
    NbNf,
    NbIf,
    IbNf,
    IbIf,
}

/// The DAI clock role of the CODEC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// `SND_SOC_DAIFMT_CBC_CFC`: the codec consumes BCLK and WCLK (`da7219->master = false`, da7219.c:1318-1320).
    Consumer,
    /// `SND_SOC_DAIFMT_CBP_CFP`: the codec provides them (da7219.c:1315-1317). Not used on Atlas; the BCLK/DAI-clock enable is not planned.
    Provider,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DaiFmt {
    pub format: Format,
    pub inversion: Inversion,
    pub role: Role,
}

/// The Atlas link (avs/boards/da7219.c:199).
pub const ATLAS_DAI_FMT: DaiFmt = DaiFmt { format: Format::I2s, inversion: Inversion::NbNf, role: Role::Consumer };

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaiError {
    /// Provider mode needs the DAI clock-enable choreography this port does not carry.
    ProviderNotSupported,
    /// `da7219_set_sr`: a rate the chip has no code for (`da7219_set_sr`, da7219.c:1511-1557).
    UnsupportedRate { hz: u32 },
    /// `da7219_hw_params`: a word length other than 16, 20, 24 or 32 (`da7219_hw_params`, da7219.c:1559-1668).
    UnsupportedWidth { bits: u32 },
    /// `da7219_hw_params`: channels outside 1 .. [`DA7219_DAI_CH_NUM_MAX`] (`da7219_hw_params`, da7219.c:1559-1668).
    UnsupportedChannels { channels: u32 },
    Plan(crate::PlanError),
}

/// The `(DAI_CLK_MODE polarity bits, DAI_CTRL format bits)` pair `da7219_set_dai_fmt` computes (da7219.c:1326-1385).
pub fn fmt_bits(fmt: DaiFmt) -> (u8, u8) {
    let (wclk, bclk) = match fmt.inversion {
        Inversion::NbNf => (false, false),
        Inversion::NbIf => (true, false),
        Inversion::IbNf => (false, true),
        Inversion::IbIf => (true, true),
    };
    // I2S / LEFT_J / RIGHT_J use the table directly (da7219.c:1326-1345); DSP_B inverts the BCLK sense (da7219.c:1346-1365).
    let (wclk, bclk) = match fmt.format {
        Format::DspB => match fmt.inversion {
            Inversion::NbNf => (false, true),
            Inversion::NbIf => (true, true),
            Inversion::IbNf => (false, false),
            Inversion::IbIf => (true, false),
        },
        _ => (wclk, bclk),
    };
    let mut clk_mode = 0;
    if wclk {
        clk_mode |= DA7219_DAI_WCLK_POL_INV;
    }
    if bclk {
        clk_mode |= DA7219_DAI_CLK_POL_INV;
    }
    let ctrl = match fmt.format {
        Format::I2s => DA7219_DAI_FORMAT_I2S,
        Format::LeftJ => DA7219_DAI_FORMAT_LEFT_J,
        Format::RightJ => DA7219_DAI_FORMAT_RIGHT_J,
        Format::DspB => DA7219_DAI_FORMAT_DSP,
    };
    (clk_mode, ctrl)
}

/// `da7219_set_dai_fmt` as a plan (da7219.c:1387-1391).
pub fn fmt_plan(fmt: DaiFmt) -> Result<Plan, DaiError> {
    if fmt.role == Role::Provider {
        return Err(DaiError::ProviderNotSupported);
    }
    let (clk_mode, ctrl) = fmt_bits(fmt);
    let mut p = Plan::new();
    p.push(Step::abort("dai-clk-polarity", Op::Update { reg: DA7219_DAI_CLK_MODE, mask: DA7219_DAI_CLK_POL_MASK | DA7219_DAI_WCLK_POL_MASK, value: clk_mode }));
    p.push(Step::abort("dai-format", Op::Update { reg: DA7219_DAI_CTRL, mask: DA7219_DAI_FORMAT_MASK, value: ctrl }));
    p.checked().map_err(DaiError::Plan)
}

/// `da7219_set_sr` (da7219.c:1511-1557): the `DA7219_SR` code of a sample rate.
pub fn sr_code(rate_hz: u32) -> Result<u8, DaiError> {
    Ok(match rate_hz {
        8000 => DA7219_SR_8000,
        11025 => DA7219_SR_11025,
        12000 => DA7219_SR_12000,
        16000 => DA7219_SR_16000,
        22050 => DA7219_SR_22050,
        24000 => DA7219_SR_24000,
        32000 => DA7219_SR_32000,
        44100 => DA7219_SR_44100,
        48000 => DA7219_SR_48000,
        88200 => DA7219_SR_88200,
        96000 => DA7219_SR_96000,
        hz => return Err(DaiError::UnsupportedRate { hz }),
    })
}

/// The `DAI_CTRL` word-length and channel-count bits of `da7219_hw_params` (da7219.c:1559-1604).
pub fn word_and_channels(width_bits: u32, channels: u32) -> Result<u8, DaiError> {
    let word = match width_bits {
        16 => DA7219_DAI_WORD_LENGTH_S16_LE,
        20 => DA7219_DAI_WORD_LENGTH_S20_LE,
        24 => DA7219_DAI_WORD_LENGTH_S24_LE,
        32 => DA7219_DAI_WORD_LENGTH_S32_LE,
        bits => return Err(DaiError::UnsupportedWidth { bits }),
    };
    if channels < 1 || channels > DA7219_DAI_CH_NUM_MAX as u32 {
        return Err(DaiError::UnsupportedChannels { channels });
    }
    Ok(word | ((channels as u8) << DA7219_DAI_CH_NUM_SHIFT))
}

/// `da7219_hw_params` for a consumer-mode codec: the sample-rate code, then the word length and channel count (da7219.c:1606-1668).
pub fn hw_params_plan(rate_hz: u32, width_bits: u32, channels: u32) -> Result<Plan, DaiError> {
    let dai_ctrl = word_and_channels(width_bits, channels)?;
    let sr = sr_code(rate_hz)?;
    let mut p = Plan::new();
    p.push(Step::abort("dai-sample-rate", Op::Write { reg: DA7219_SR, value: sr }));
    p.push(Step::abort("dai-word-channels", Op::Update { reg: DA7219_DAI_CTRL, mask: DA7219_DAI_WORD_LENGTH_MASK | DA7219_DAI_CH_NUM_MASK, value: dai_ctrl }));
    p.checked().map_err(DaiError::Plan)
}
