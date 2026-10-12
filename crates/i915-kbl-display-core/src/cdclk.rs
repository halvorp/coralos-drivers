// SPDX-License-Identifier: GPL-2.0-only
//! The core display clock as Skylake / Kaby Lake firmware leaves it.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_cdclk.c (`skl_get_cdclk`, `skl_dpll0_update`,
//! `skl_calc_voltage_level`, `skl_cdclk_decimal`). Copyright (c) Intel Corporation.
//!
//! DPLL0 (LCPLL1) is the CDCLK VCO: it runs at 8.1 GHz or 8.64 GHz depending on which DisplayPort link rate the BIOS
//! put in DPLL_CTRL1 for it, and CDCLK_CTL's frequency select divides that down. With DPLL0 off the display runs on
//! the 24 MHz bypass clock.

use crate::dpll::{ctrl1_hdmi_mode, ctrl1_link_rate_mask, ctrl1_link_rate_shift, ctrl1_override, ctrl1_ssc};
use crate::regs::*;

/// `cdclk_config->ref` (`skl_dpll0_update`, intel_cdclk.c:970).
pub const CDCLK_REF_KHZ: u32 = 24000;

/// Why a CDCLK readout fell back to the bypass clock. Linux `drm_WARN_ON`s each of these and returns with vco = 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdclkAnomaly {
    None,
    /// DPLL0 enabled but not locked (intel_cdclk.c:977-978).
    Dpll0NotLocked,
    /// DPLL_CTRL1 for DPLL0 has HDMI mode or SSC set, or the override bit clear (intel_cdclk.c:982-987).
    Dpll0Ctrl1Unexpected { ctrl1: u32 },
    /// DPLL0's link-rate field is 6 or 7 (`MISSING_CASE`, intel_cdclk.c:1001).
    Dpll0LinkRateReserved { field: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CdclkState {
    pub ref_khz: u32,
    /// DPLL0's VCO in kHz (8_100_000 or 8_640_000), 0 when DPLL0 is off or anomalous.
    pub vco_khz: u32,
    pub cdclk_khz: u32,
    /// `skl_calc_voltage_level(cdclk)` - Linux cannot read it back either and assumes the minimum the clock needs.
    pub voltage_level: u8,
    /// The raw CDCLK_FREQ_DECIMAL field (`skl_cdclk_decimal(cdclk)` when firmware programmed it consistently).
    pub decimal: u32,
    pub anomaly: CdclkAnomaly,
}

/// `skl_calc_voltage_level` (intel_cdclk.c:953-963).
pub const fn voltage_level(cdclk_khz: u32) -> u8 {
    if cdclk_khz > 540000 {
        3 // intel_cdclk.c:956
    } else if cdclk_khz > 450000 {
        2 // intel_cdclk.c:958
    } else if cdclk_khz > 337500 {
        1 // intel_cdclk.c:960
    } else {
        0 // intel_cdclk.c:962
    }
}

/// `skl_cdclk_decimal` (intel_cdclk.c:1068-1071): kHz -> ".1 fixpoint MHz with -1 MHz offset", rounded to nearest.
pub const fn cdclk_decimal(cdclk_khz: u32) -> u32 {
    (cdclk_khz - 1000 + 250) / 500 // DIV_ROUND_CLOSEST(cdclk - 1000, 500)
}

/// `skl_get_cdclk` (intel_cdclk.c:1006-1066) from the three registers it reads.
pub fn decode_cdclk(lcpll1_ctl: u32, dpll_ctrl1: u32, cdclk_ctl: u32) -> CdclkState {
    let mut st = CdclkState {
        ref_khz: CDCLK_REF_KHZ,
        vco_khz: 0,
        cdclk_khz: CDCLK_REF_KHZ,
        voltage_level: 0,
        decimal: cdclk_ctl & CDCLK_FREQ_DECIMAL_MASK,
        anomaly: CdclkAnomaly::None,
    };
    // skl_dpll0_update
    if lcpll1_ctl & LCPLL_PLL_ENABLE == 0 {
        // intel_cdclk.c:974-975: DPLL0 off, bypass clock
    } else if lcpll1_ctl & LCPLL_PLL_LOCK == 0 {
        st.anomaly = CdclkAnomaly::Dpll0NotLocked; // intel_cdclk.c:977-978
    } else if dpll_ctrl1 & (ctrl1_hdmi_mode(0) | ctrl1_ssc(0) | ctrl1_override(0)) != ctrl1_override(0) {
        st.anomaly = CdclkAnomaly::Dpll0Ctrl1Unexpected { ctrl1: dpll_ctrl1 & 0x3f }; // intel_cdclk.c:982-987
    } else {
        let field = (dpll_ctrl1 & ctrl1_link_rate_mask(0)) >> ctrl1_link_rate_shift(0);
        match field {
            DPLL_CTRL1_LINK_RATE_810
            | DPLL_CTRL1_LINK_RATE_1350
            | DPLL_CTRL1_LINK_RATE_1620
            | DPLL_CTRL1_LINK_RATE_2700 => st.vco_khz = 8_100_000, // intel_cdclk.c:990-994
            DPLL_CTRL1_LINK_RATE_1080 | DPLL_CTRL1_LINK_RATE_2160 => st.vco_khz = 8_640_000, // intel_cdclk.c:996-998
            other => st.anomaly = CdclkAnomaly::Dpll0LinkRateReserved { field: other }, // intel_cdclk.c:1001
        }
    }
    // skl_get_cdclk
    if st.vco_khz != 0 {
        // The select field has four values and Linux defines all four (the `default: MISSING_CASE` arms at
        // intel_cdclk.c:1034 and :1052 are unreachable), so this match is exhaustive and the compiler pins it.
        let sel = cdclk_ctl & CDCLK_FREQ_SEL_MASK;
        st.cdclk_khz = if st.vco_khz == 8_640_000 {
            match sel {
                CDCLK_FREQ_450_432 => 432_000, // intel_cdclk.c:1022-1023
                CDCLK_FREQ_337_308 => 308_571, // intel_cdclk.c:1025-1026
                CDCLK_FREQ_540 => 540_000,     // intel_cdclk.c:1028-1029
                _ => 617_143,                  // CDCLK_FREQ_675_617, intel_cdclk.c:1031-1032
            }
        } else {
            match sel {
                CDCLK_FREQ_450_432 => 450_000, // intel_cdclk.c:1040-1041
                CDCLK_FREQ_337_308 => 337_500, // intel_cdclk.c:1043-1044
                CDCLK_FREQ_540 => 540_000,     // intel_cdclk.c:1046-1047
                _ => 675_000,                  // CDCLK_FREQ_675_617, intel_cdclk.c:1049-1050
            }
        };
    }
    st.voltage_level = voltage_level(st.cdclk_khz); // intel_cdclk.c:1063-1064
    st
}
