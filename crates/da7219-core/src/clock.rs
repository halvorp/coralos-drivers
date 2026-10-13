// SPDX-License-Identifier: GPL-2.0-only
//! MCLK source and PLL configuration: `da7219_set_dai_sysclk` (da7219.c:1163-1218), `da7219_set_pll` (da7219.c:1220-1292) and
//! `da7219_set_dai_pll` (da7219.c:1294), with the values the AVS machine driver passes (`avs/boards/da7219.c`).
use crate::bus::*;
use crate::fields::*;
use crate::regs::*;

/// `enum da7219_clk_src` (da7219.h:785-788).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClkSrc {
    Mclk,
    MclkSqr,
}

/// `enum da7219_sys_clk` (da7219.h:790-794): bypass the PLL, normal PLL, or sample-rate-matching PLL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysClk {
    Mclk,
    Pll,
    PllSrm,
}

/// da7219.c:1177: "(freq < 2000000) || (freq > 54000000)" is "Unsupported MCLK value".
pub const MCLK_MIN_HZ: u32 = 2_000_000; // da7219.c:1177
pub const MCLK_MAX_HZ: u32 = 54_000_000; // da7219.c:1177
/// Kaby Lake: `clk_freq = 24576000` (avs/boards/da7219.c:106). Apollo Lake is 19.2 MHz and is not an Atlas value.
pub const ATLAS_MCLK_HZ: u32 = 24_576_000; // avs/boards/da7219.c:106
/// The SRM PLL output the machine driver selects when the platform clock turns on (avs/boards/da7219.c:47-48).
pub const ATLAS_PLL_OUT_HZ: u32 = DA7219_PLL_FREQ_OUT_98304;
/// The two PLL output frequencies the chip is specified for (da7219.h:762-763). Linux's `da7219_set_pll` accepts any `fout`; this port
/// refuses everything else by name.
pub const PLL_OUT_FREQS: [u32; 2] = [DA7219_PLL_FREQ_OUT_90316, DA7219_PLL_FREQ_OUT_98304];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// MCLK outside 2 .. 54 MHz (da7219.c:1177, :1229-1256).
    MclkOutOfRange { hz: u32 },
    /// `fout` is not one of [`PLL_OUT_FREQS`].
    UnsupportedPllOut { hz: u32 },
    Plan(crate::PlanError),
}

/// The computed PLL setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pll {
    /// `PLL_CTRL` INDIV | MODE bits (the value written under `PLL_INDIV_MASK | PLL_MODE_MASK`).
    pub ctrl: u8,
    /// The input divider value (1, 2, 4, 8 or 16).
    pub indiv: u32,
    pub integer: u8,
    pub frac_top: u8,
    pub frac_bot: u8,
}

/// `da7219_set_pll`'s input-divider selection (da7219.c:1229-1256). Returns (INDIV bits, divider value).
pub fn indiv_for(mclk_hz: u32) -> Result<(u8, u32), ClockError> {
    if mclk_hz < MCLK_MIN_HZ || mclk_hz > MCLK_MAX_HZ {
        return Err(ClockError::MclkOutOfRange { hz: mclk_hz });
    }
    Ok(if mclk_hz <= 4_500_000 {
        (DA7219_PLL_INDIV_2_TO_4_5_MHZ, DA7219_PLL_INDIV_2_TO_4_5_MHZ_VAL as u32) // da7219.c:1236
    } else if mclk_hz <= 9_000_000 {
        (DA7219_PLL_INDIV_4_5_TO_9_MHZ, DA7219_PLL_INDIV_4_5_TO_9_MHZ_VAL as u32) // da7219.c:1239
    } else if mclk_hz <= 18_000_000 {
        (DA7219_PLL_INDIV_9_TO_18_MHZ, DA7219_PLL_INDIV_9_TO_18_MHZ_VAL as u32) // da7219.c:1242
    } else if mclk_hz <= 36_000_000 {
        (DA7219_PLL_INDIV_18_TO_36_MHZ, DA7219_PLL_INDIV_18_TO_36_MHZ_VAL as u32) // da7219.c:1245
    } else {
        (DA7219_PLL_INDIV_36_TO_54_MHZ, DA7219_PLL_INDIV_36_TO_54_MHZ_VAL as u32) // da7219.c:1248
    })
}

/// The dividers for `source` at `fout` from an MCLK of `mclk_hz` (da7219.c:1257-1281). `SysClk::Mclk` is the bypass: no dividers.
pub fn pll_dividers(mclk_hz: u32, source: SysClk, fout: u32) -> Result<Pll, ClockError> {
    let (indiv_bits, indiv) = indiv_for(mclk_hz)?;
    let freq_ref = mclk_hz / indiv;
    let mode = match source {
        SysClk::Mclk => {
            return Ok(Pll { ctrl: indiv_bits | DA7219_PLL_MODE_BYPASS, indiv, integer: 0, frac_top: 0, frac_bot: 0 });
        }
        SysClk::Pll => DA7219_PLL_MODE_NORMAL,
        SysClk::PllSrm => DA7219_PLL_MODE_SRM,
    };
    if !PLL_OUT_FREQS.contains(&fout) {
        return Err(ClockError::UnsupportedPllOut { hz: fout });
    }
    // TYPE-PINNED: every reference is at most 4.5 MHz and at least 2 MHz / 1 = 2 MHz (indiv_for), and `fout` is one of the two
    // frequencies above, so the integer divider is between 20 and 49 - inside the 7-bit field; the sweep test proves it for every bucket.
    let integer = fout / freq_ref; // da7219.c:1277
    let frac_div = ((fout % freq_ref) as u64 * 8192) / freq_ref as u64; // da7219.c:1278
    Ok(Pll {
        ctrl: indiv_bits | mode,
        indiv,
        integer: integer as u8,
        frac_top: ((frac_div >> DA7219_BYTE_SHIFT) & DA7219_BYTE_MASK as u64) as u8, // da7219.c:1280
        frac_bot: (frac_div & DA7219_BYTE_MASK as u64) as u8,                       // da7219.c:1281
    })
}

/// `da7219_set_dai_sysclk` (da7219.c:1163-1218): choose the MCLK source (square-wave or not) after validating the rate. The plan is the
/// `update_bits` on `PLL_CTRL`; the caller records `mclk_hz` for the PLL computation.
pub fn sysclk_plan(src: ClkSrc, mclk_hz: u32) -> Result<Plan, ClockError> {
    indiv_for(mclk_hz)?; // da7219.c:1177: the range check
    let value = match src {
        ClkSrc::MclkSqr => DA7219_PLL_MCLK_SQR_EN_MASK, // da7219.c:1185-1189
        ClkSrc::Mclk => 0,                                // da7219.c:1190-1193
    };
    let mut p = Plan::new();
    p.push(Step::abort("pll-mclk-source", Op::Update { reg: DA7219_PLL_CTRL, mask: DA7219_PLL_MCLK_SQR_EN_MASK, value }));
    p.checked().map_err(ClockError::Plan)
}

/// `da7219_set_pll` as a plan: the fractional and integer dividers first, then `PLL_CTRL` (da7219.c:1283-1291). The bypass writes only
/// `PLL_CTRL` (da7219.c:1259-1264) - that is how the machine driver STOPS the PLL (avs/boards/da7219.c:43).
pub fn pll_plan(mclk_hz: u32, source: SysClk, fout: u32) -> Result<Plan, ClockError> {
    let pll = pll_dividers(mclk_hz, source, fout)?;
    let mut p = Plan::new();
    if source != SysClk::Mclk {
        p.push(Step::abort("pll-frac-top", Op::Write { reg: DA7219_PLL_FRAC_TOP, value: pll.frac_top }));
        p.push(Step::abort("pll-frac-bot", Op::Write { reg: DA7219_PLL_FRAC_BOT, value: pll.frac_bot }));
        p.push(Step::abort("pll-integer", Op::Write { reg: DA7219_PLL_INTEGER, value: pll.integer }));
    }
    p.push(Step::abort("pll-ctrl", Op::Update { reg: DA7219_PLL_CTRL, mask: DA7219_PLL_INDIV_MASK | DA7219_PLL_MODE_MASK, value: pll.ctrl }));
    p.checked().map_err(ClockError::Plan)
}

/// What the AVS machine driver does when the platform clock widget powers up: the SRM PLL at 98.304 MHz from the 24.576 MHz MCLK
/// (avs/boards/da7219.c:46-49, :106-108). Reads DAI/sysclk setup and PLL start in one plan.
pub fn atlas_clock_up() -> Result<Plan, ClockError> {
    let mut p = sysclk_plan(ClkSrc::Mclk, ATLAS_MCLK_HZ)?;
    p.extend(&pll_plan(ATLAS_MCLK_HZ, SysClk::PllSrm, ATLAS_PLL_OUT_HZ)?);
    p.checked().map_err(ClockError::Plan)
}

/// `platform_clock_control` power-off: the PLL back to bypass (avs/boards/da7219.c:42-46).
pub fn atlas_clock_down() -> Result<Plan, ClockError> {
    pll_plan(ATLAS_MCLK_HZ, SysClk::Mclk, 0)
}
