// SPDX-License-Identifier: GPL-2.0-only
//! The Skylake / Kaby Lake core display clock: choosing it (a constraint solver) and changing it (a bounded transaction).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_cdclk.c (`skl_calc_cdclk`, `skl_calc_voltage_level`, `skl_cdclk_decimal`,
//! `skl_dpll0_link_rate`, `skl_dpll0_enable`, `skl_dpll0_disable`, `skl_cdclk_freq_sel`, `skl_set_cdclk`, `skl_sanitize_cdclk`,
//! `skl_cdclk_init_hw`, `skl_cdclk_uninit_hw`, `skl_dpll0_vco`, `skl_modeset_calc_cdclk`, `intel_update_max_cdclk`,
//! `_intel_pixel_rate_to_cdclk`, `intel_cdclk_ppc`, `intel_cdclk_guardband`, `intel_compute_min_cdclk`) and intel_audio.c
//! (`intel_audio_min_cdclk`) at v7.0-rc6. Copyright (c) Intel Corporation.
//!
//! The READ side (`skl_get_cdclk`, `skl_dpll0_update`, `skl_calc_voltage_level`, `skl_cdclk_decimal`) already lives in
//! `i915-kbl-display-core::cdclk` and is re-exported, not re-typed.
//!
//! NOT ported here, with the owner that should: the per-plane minimum (`plane_min_cdclk`, skl_watermark / skl_universal_plane:
//! it needs plane formats and scaling, an `i915-kbl-display-core` write-planner concern), the dbuf bandwidth minimum
//! (`dbuf_bw_min_cdclk`, ver >= 12 only) and FBC/IPS minimums (`intel_fbc_min_cdclk` is ver >= 10, `hsw_ips_min_cdclk` is
//! Broadwell). A caller folds those into `force_min_khz` / its own `max()`.
use i915_kbl_display_core::dpll::{ctrl1_hdmi_mode, ctrl1_link_rate_mask, ctrl1_link_rate_shift, ctrl1_override, ctrl1_ssc};
use crate::error::{ClockError, Handshake};
use crate::pcode::{PcodeOp, PcodeRequest, PcodeWrite};
use crate::plan::{OnTimeout, PollPlan, Transaction, WritePlan};
use crate::regs::*;
use i915_common_core::Reg;
pub use i915_kbl_display_core::cdclk::{cdclk_decimal, decode_cdclk, voltage_level, CdclkState, CDCLK_REF_KHZ};

/// `cdclk.hw.bypass` for SKL: the 24 MHz reference (`skl_dpll0_update`, intel_cdclk.c:970-971).
pub const BYPASS_KHZ: u32 = CDCLK_REF_KHZ;
/// DPLL0 VCOs (`skl_calc_cdclk`, intel_cdclk.c:930-951).
pub const VCO_8100_KHZ: u32 = 8_100_000;
pub const VCO_8640_KHZ: u32 = 8_640_000;
/// Linux's `hw.vco = ~0` after `skl_sanitize_cdclk` decides the firmware state is not trustworthy: "force full PLL disable +
/// enable" (intel_cdclk.c:1268-1269).
pub const VCO_UNKNOWN: u32 = u32::MAX;
/// `skl_dpll0_enable` waits up to 5 ms for lock; `skl_dpll0_disable` 1 ms (intel_cdclk.c:1115, :1129).
pub const DPLL0_LOCK_TIMEOUT_MS: u32 = 5; // intel_cdclk.c:1115
pub const DPLL0_UNLOCK_TIMEOUT_MS: u32 = 1; // intel_cdclk.c:1129

/// `skl_calc_cdclk` (intel_cdclk.c:930-951): the smallest SKL step >= `min_cdclk` for the VCO (the largest if none is).
pub const fn calc_cdclk(min_cdclk_khz: u32, vco_khz: u32) -> u32 {
    if vco_khz == VCO_8640_KHZ {
        if min_cdclk_khz > 540000 {
            617143 // intel_cdclk.c:933-934
        } else if min_cdclk_khz > 432000 {
            540000 // intel_cdclk.c:935-936
        } else if min_cdclk_khz > 308571 {
            432000 // intel_cdclk.c:937-938
        } else {
            308571 // intel_cdclk.c:939-940
        }
    } else if min_cdclk_khz > 540000 {
        675000 // intel_cdclk.c:942-943
    } else if min_cdclk_khz > 450000 {
        540000 // intel_cdclk.c:944-945
    } else if min_cdclk_khz > 337500 {
        450000 // intel_cdclk.c:946-947
    } else {
        337500 // intel_cdclk.c:948-949
    }
}

/// `intel_cdclk_ppc(display, double_wide)` for display ver 9: 1 (`DISPLAY_VER >= 10 || double_wide ? 2 : 1`, intel_cdclk.c:2869;
/// no Gen9 part has double-wide).
pub const PIXELS_PER_CLOCK: u64 = 1; // intel_cdclk.c:2869
/// `intel_cdclk_guardband` for display ver >= 9: 100 % (intel_cdclk.c:2875-2877).
pub const GUARDBAND_PERCENT: u64 = 100; // intel_cdclk.c:2877

/// `_intel_pixel_rate_to_cdclk` (intel_cdclk.c:2884-2891): `DIV_ROUND_UP(pixel_rate * 100, guardband * ppc)`.
pub const fn pixel_rate_to_cdclk(pixel_rate_khz: u32) -> u32 {
    let num = pixel_rate_khz as u64 * 100;
    let den = GUARDBAND_PERCENT * PIXELS_PER_CLOCK;
    num.div_ceil(den) as u32
}

/// What one enabled CRTC asks of CDCLK, from the inputs `intel_crtc_min_cdclk` uses that exist on Gen9 desktop-class parts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CrtcLoad {
    /// `crtc_state->pixel_rate`, kHz.
    pub pixel_rate_khz: u32,
    /// `crtc_state->has_audio`.
    pub has_audio: bool,
    /// For a DP encoder: `crtc_state->port_clock` (kHz) and `lane_count`. `None` for HDMI.
    pub dp_link: Option<(u32, u8)>,
}

/// `intel_audio_min_cdclk` for display ver 9 (intel_audio.c:996-1040).
pub const fn audio_min_cdclk(c: &CrtcLoad) -> u32 {
    if !c.has_audio {
        return 0; // intel_audio.c:1001-1002
    }
    let mut min = 0;
    if let Some((port_clock, lanes)) = c.dp_link {
        // Display WA #1144 (intel_audio.c:1004-1018): HBR2 x4 with audio needs 432 MHz.
        if port_clock >= 540000 && lanes == 4 {
            min = 432000; // intel_audio.c:1017
        }
    }
    // "must be at least twice the frequency of the Azalia BCLK", 96 MHz (intel_audio.c:1021-1026)
    if min < 2 * 96000 {
        min = 2 * 96000;
    }
    min
}

/// `intel_crtc_min_cdclk` for an enabled CRTC, Gen9 terms (intel_cdclk.c:2911-2928): pixel-rate and audio minimums.
pub const fn crtc_min_cdclk(c: &CrtcLoad) -> u32 {
    let px = pixel_rate_to_cdclk(c.pixel_rate_khz);
    let au = audio_min_cdclk(c);
    if px > au {
        px
    } else {
        au
    }
}

/// `intel_update_max_cdclk` for display ver 9 (intel_cdclk.c:3668-3680): the SKL_DFSM fuse caps the clock; the guess is made
/// on the 8640 table and `skl_calc_cdclk` corrects it for the VCO in use.
pub const fn max_cdclk(dfsm: u32, vco_khz: u32) -> u32 {
    let limit = dfsm & SKL_DFSM_CDCLK_LIMIT_MASK;
    let guess = if limit == SKL_DFSM_CDCLK_LIMIT_675 {
        617143 // intel_cdclk.c:3670-3671
    } else if limit == SKL_DFSM_CDCLK_LIMIT_540 {
        540000 // intel_cdclk.c:3672-3673
    } else if limit == SKL_DFSM_CDCLK_LIMIT_450 {
        432000 // intel_cdclk.c:3674-3675
    } else {
        308571 // intel_cdclk.c:3676-3677
    };
    calc_cdclk(guess, vco_khz) // intel_cdclk.c:3679
}

/// `intel_compute_min_cdclk` (intel_cdclk.c:3024-3056) over the enabled CRTCs: the largest per-CRTC minimum and the forced
/// minimum, refused above `max_khz` (`-EINVAL`).
pub fn compute_min_cdclk(crtcs: &[CrtcLoad], force_min_khz: u32, max_khz: u32) -> Result<u32, ClockError> {
    let mut min = force_min_khz;
    for c in crtcs {
        min = min.max(crtc_min_cdclk(c));
    }
    if min > max_khz {
        return Err(ClockError::CdclkAboveMax { min_khz: min, max_khz }); // intel_cdclk.c:3048-3053
    }
    Ok(min)
}

/// `skl_dpll0_vco` (intel_cdclk.c:3167-3203): the VCO DPLL0 should run at. An enabled eDP CRTC decides it from its port clock
/// (the eDP 1.4 rates 2.16 / 4.32 Gbit/s need 8640 MHz, every other rate 8100 MHz); otherwise the logical VCO is kept, falling
/// back to the preferred one.
pub fn dpll0_vco(logical_vco_khz: u32, preferred_vco_khz: u32, edp_port_clock_khz: Option<u32>) -> u32 {
    let mut vco = logical_vco_khz;
    if vco == 0 {
        vco = preferred_vco_khz; // intel_cdclk.c:3177-3178
    }
    if let Some(port_clock) = edp_port_clock_khz {
        vco = match port_clock / 2 {
            108000 | 216000 => VCO_8640_KHZ, // intel_cdclk.c:3192-3194
            _ => VCO_8100_KHZ,               // intel_cdclk.c:3196-3197
        };
    }
    vco
}

/// One CDCLK configuration (`struct intel_cdclk_config`, the fields SKL uses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdclkConfig {
    pub cdclk_khz: u32,
    pub vco_khz: u32,
    pub voltage_level: u8,
}

/// `skl_modeset_calc_cdclk` for the active-pipes case (intel_cdclk.c:3205-3230): the smallest step >= `min_cdclk` for `vco`.
pub fn modeset_calc_cdclk(min_cdclk_khz: u32, vco_khz: u32, dfsm: u32) -> Result<CdclkConfig, ClockError> {
    check_vco(vco_khz)?;
    let max = max_cdclk(dfsm, vco_khz);
    if min_cdclk_khz > max {
        return Err(ClockError::CdclkAboveMax { min_khz: min_cdclk_khz, max_khz: max });
    }
    let cdclk = calc_cdclk(min_cdclk_khz, vco_khz);
    Ok(CdclkConfig { cdclk_khz: cdclk, vco_khz, voltage_level: voltage_level(cdclk) })
}

/// `skl_cdclk_init_hw`'s fallback target (intel_cdclk.c:1292-1296): the preferred VCO (8100000 if none) at its smallest step.
pub fn init_target(preferred_vco_khz: u32) -> CdclkConfig {
    let vco = if preferred_vco_khz == 0 { VCO_8100_KHZ } else { preferred_vco_khz };
    let cdclk = calc_cdclk(0, vco);
    CdclkConfig { cdclk_khz: cdclk, vco_khz: vco, voltage_level: voltage_level(cdclk) }
}

const fn check_vco(vco_khz: u32) -> Result<(), ClockError> {
    if vco_khz == VCO_8100_KHZ || vco_khz == VCO_8640_KHZ {
        Ok(())
    } else {
        Err(ClockError::CdclkVcoInvalid { vco_khz }) // drm_WARN_ON, intel_cdclk.c:1085
    }
}

/// `skl_cdclk_freq_sel` (intel_cdclk.c:1135-1156). Linux's `default:` arm accepts anything and picks 337.5/308.57; this refuses a
/// frequency that is not a step of the VCO's table.
pub const fn freq_sel(cdclk_khz: u32, vco_khz: u32) -> Result<u32, ClockError> {
    match (cdclk_khz, vco_khz) {
        (308571, VCO_8640_KHZ) | (337500, VCO_8100_KHZ) => Ok(CDCLK_FREQ_337_308), // intel_cdclk.c:1144-1146
        (432000, VCO_8640_KHZ) | (450000, VCO_8100_KHZ) => Ok(CDCLK_FREQ_450_432), // intel_cdclk.c:1147-1149
        (540000, VCO_8640_KHZ) | (540000, VCO_8100_KHZ) => Ok(CDCLK_FREQ_540),     // intel_cdclk.c:1150-1151
        (617143, VCO_8640_KHZ) | (675000, VCO_8100_KHZ) => Ok(CDCLK_FREQ_675_617), // intel_cdclk.c:1152-1154
        _ => Err(ClockError::CdclkNotAStep { cdclk_khz, vco_khz }),
    }
}

/// `skl_dpll0_link_rate` (intel_cdclk.c:1083-1100): DPLL0 is enabled at the LOWEST link rate its VCO supports.
const fn dpll0_link_rate_field(vco_khz: u32) -> u32 {
    if vco_khz == VCO_8640_KHZ {
        DPLL_CTRL1_LINK_RATE_1080 // intel_cdclk.c:1096-1097
    } else {
        DPLL_CTRL1_LINK_RATE_810 // intel_cdclk.c:1099
    }
}

/// The CDCLK hardware snapshot a change is planned from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdclkHw {
    /// `cdclk.hw.vco`: 0 (DPLL0 off), 8100000, 8640000, or [`VCO_UNKNOWN`].
    pub vco_khz: u32,
    /// The raw CDCLK_CTL word, read under the same lock immediately before planning (Linux reads it mid-sequence,
    /// intel_cdclk.c:1194; nothing in the earlier steps writes it).
    pub cdclk_ctl: u32,
    /// `cdclk.hw.cdclk`, kHz.
    pub cdclk_khz: u32,
    /// `cdclk.hw.voltage_level`.
    pub voltage_level: u8,
}

impl CdclkHw {
    /// From the readout (`decode_cdclk`) and the raw word.
    pub fn from_readout(st: &CdclkState, cdclk_ctl: u32) -> CdclkHw {
        CdclkHw { vco_khz: st.vco_khz, cdclk_khz: st.cdclk_khz, voltage_level: st.voltage_level, cdclk_ctl }
    }
    pub fn config(&self) -> CdclkConfig {
        CdclkConfig { cdclk_khz: self.cdclk_khz, vco_khz: self.vco_khz, voltage_level: self.voltage_level }
    }
}

/// `skl_sanitize_cdclk` (intel_cdclk.c:1230-1270): is the firmware's CDCLK trustworthy, or must it be reprogrammed from
/// scratch? `true` = sanitize: set `cdclk = 0`, `vco = ~0` (see [`sanitized`]).
pub fn needs_sanitize(swf18: u32, st: &CdclkState, cdclk_ctl: u32) -> bool {
    // "the pre-OS did not initialise the display" (intel_cdclk.c:1239-1240)
    if swf18 & 0x00ff_ffff == 0 {
        return true;
    }
    // "Is PLL enabled and locked?" (intel_cdclk.c:1246-1248)
    if st.vco_khz == 0 || st.cdclk_khz == BYPASS_KHZ {
        return true;
    }
    // The decimal part may be programmed wrong (intel_cdclk.c:1256-1261).
    let expected = (cdclk_ctl & CDCLK_FREQ_SEL_MASK) | cdclk_decimal(st.cdclk_khz);
    cdclk_ctl != expected
}

/// The hardware view `skl_sanitize_cdclk` leaves behind to force a full PLL disable + enable (intel_cdclk.c:1268-1269).
pub const fn sanitized(hw: &CdclkHw) -> CdclkHw {
    CdclkHw { vco_khz: VCO_UNKNOWN, cdclk_khz: 0, voltage_level: hw.voltage_level, cdclk_ctl: hw.cdclk_ctl }
}

/// `skl_set_cdclk` (intel_cdclk.c:1158-1228) as a transaction. `skylake` is true for SKL (not KBL/CFL): Display WA #1183
/// forbids the 8640 MHz VCO there (a `WARN_ON_ONCE` in Linux, a refusal here).
///
/// The sequence, each step with its Linux line:
///   PCU prepare -> [DPLL0 off] -> CDCLK_CTL (new VCO only) -> CDCLK_CTL | DIVMUX override -> [DPLL0 on] -> CDCLK_CTL cleared
///   -> CDCLK_CTL select + decimal -> CDCLK_CTL override off -> PCU voltage level.
///
/// The ROLLBACK is the whole sequence run again toward the snapshot's configuration from the "unknown" state, i.e. with a
/// forced DPLL0 off/on, exactly how Linux treats a state it cannot trust (`skl_sanitize_cdclk`). If the snapshot had DPLL0 off
/// the rollback turns it off and tells the PCU level 0.
pub fn plan_set_cdclk(hw: &CdclkHw, target: &CdclkConfig, skylake: bool) -> Result<Transaction, ClockError> {
    let forward = set_cdclk_plan(hw, target, skylake)?;
    let rollback = if hw.vco_khz == 0 || hw.vco_khz == VCO_UNKNOWN {
        bypass_plan(&sanitized(hw), voltage_level(BYPASS_KHZ))?
    } else {
        set_cdclk_plan(&sanitized(hw), &hw.config(), skylake)?
    };
    Ok(Transaction { forward, rollback })
}

/// `skl_cdclk_uninit_hw` (intel_cdclk.c:1301-1310) as a transaction: back to the 24 MHz bypass with DPLL0 off. Linux calls
/// `skl_set_cdclk` with `vco = 0`, whose `hw.vco != vco` test then calls `skl_dpll0_enable(.., 0)` - re-enabling DPLL0 with an
/// invalid VCO and a `drm_WARN_ON`. That quirk is not reproduced: DPLL0 stays off.
pub fn plan_uninit(hw: &CdclkHw) -> Result<Transaction, ClockError> {
    let forward = bypass_plan(hw, voltage_level(BYPASS_KHZ))?;
    let rollback = if hw.vco_khz == 0 || hw.vco_khz == VCO_UNKNOWN {
        WritePlan::new()
    } else {
        set_cdclk_plan(&sanitized(hw), &hw.config(), false)?
    };
    Ok(Transaction { forward, rollback })
}

fn dpll0_off(p: &mut WritePlan) -> Result<(), ClockError> {
    p.rmw(LCPLL1_CTL, LCPLL_PLL_ENABLE, 0)?; // intel_cdclk.c:1126-1127
    p.poll(PollPlan::wait_clear(Handshake::Dpll0Unlock, Reg::new(LCPLL1_CTL), LCPLL_PLL_LOCK, DPLL0_UNLOCK_TIMEOUT_MS, OnTimeout::Fail))
    // intel_cdclk.c:1129
}

fn bypass_plan(hw: &CdclkHw, level: u8) -> Result<WritePlan, ClockError> {
    let mut p = WritePlan::new();
    p.pcode(PcodeOp::Request(PcodeRequest::cdclk_prepare()))?; // intel_cdclk.c:1178-1181
    if hw.vco_khz != 0 {
        dpll0_off(&mut p)?;
    }
    p.pcode(PcodeOp::Write(PcodeWrite::new(SKL_PCODE_CDCLK_CONTROL, level as u32)))?; // intel_cdclk.c:1224-1225
    Ok(p)
}

fn set_cdclk_plan(hw: &CdclkHw, target: &CdclkConfig, skylake: bool) -> Result<WritePlan, ClockError> {
    check_vco(target.vco_khz)?;
    if skylake && target.vco_khz == VCO_8640_KHZ {
        return Err(ClockError::CdclkSkylakeVco8640 { vco_khz: target.vco_khz }); // intel_cdclk.c:1175-1176
    }
    let freq_select = freq_sel(target.cdclk_khz, target.vco_khz)?;
    let decimal = cdclk_decimal(target.cdclk_khz);
    let vco_changes = hw.vco_khz != target.vco_khz;
    let mut p = WritePlan::new();

    p.pcode(PcodeOp::Request(PcodeRequest::cdclk_prepare()))?; // intel_cdclk.c:1178-1181
    if hw.vco_khz != 0 && vco_changes {
        dpll0_off(&mut p)?; // intel_cdclk.c:1190-1192
    }
    let mut ctl = hw.cdclk_ctl; // intel_cdclk.c:1194
    if vco_changes {
        // Wa Display #1183: skl,kbl,cfl (intel_cdclk.c:1196-1200)
        ctl &= !(CDCLK_FREQ_SEL_MASK | CDCLK_FREQ_DECIMAL_MASK);
        ctl |= freq_select | decimal;
        p.write(CDCLK_CTL, ctl)?;
    }
    ctl |= CDCLK_DIVMUX_CD_OVERRIDE; // intel_cdclk.c:1204
    p.write(CDCLK_CTL, ctl)?;
    p.posting_read(CDCLK_CTL)?; // intel_cdclk.c:1206
    if vco_changes {
        // skl_dpll0_enable (intel_cdclk.c:1102-1122), called at intel_cdclk.c:1208-1209
        p.rmw(
            DPLL_CTRL1,
            ctrl1_hdmi_mode(0) | ctrl1_ssc(0) | ctrl1_link_rate_mask(0),
            ctrl1_override(0) | (dpll0_link_rate_field(target.vco_khz) << ctrl1_link_rate_shift(0)),
        )?; // intel_cdclk.c:1104-1109
        p.posting_read(DPLL_CTRL1)?; // intel_cdclk.c:1110
        p.rmw(LCPLL1_CTL, 0, LCPLL_PLL_ENABLE)?; // intel_cdclk.c:1112-1113
        p.poll(PollPlan::wait_set(Handshake::Dpll0Lock, Reg::new(LCPLL1_CTL), LCPLL_PLL_LOCK, DPLL0_LOCK_TIMEOUT_MS, OnTimeout::Fail))?;
        // intel_cdclk.c:1115
    }
    ctl &= !(CDCLK_FREQ_SEL_MASK | CDCLK_FREQ_DECIMAL_MASK); // intel_cdclk.c:1212-1213
    p.write(CDCLK_CTL, ctl)?;
    ctl |= freq_select | decimal; // intel_cdclk.c:1215-1216
    p.write(CDCLK_CTL, ctl)?;
    ctl &= !CDCLK_DIVMUX_CD_OVERRIDE; // intel_cdclk.c:1219-1220
    p.write(CDCLK_CTL, ctl)?;
    p.posting_read(CDCLK_CTL)?; // intel_cdclk.c:1221
    p.pcode(PcodeOp::Write(PcodeWrite::new(SKL_PCODE_CDCLK_CONTROL, target.voltage_level as u32)))?; // intel_cdclk.c:1224-1225
    Ok(p)
}
