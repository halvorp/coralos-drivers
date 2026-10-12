// SPDX-License-Identifier: GPL-2.0-only
//! The Skylake-class display PLLs (DPLL0..3): which one a DDI uses, whether it is on and locked, and the clock it makes.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dpll_mgr.c (`skl_dpll_regs`, `skl_ddi_pll_get_hw_state`,
//! `skl_ddi_dpll0_get_hw_state`, `skl_ddi_pll_get_freq`, `skl_ddi_lcpll_get_freq`, `skl_ddi_wrpll_get_freq`) and
//! intel_ddi.c (`skl_ddi_get_pll`, `skl_ddi_is_clock_enabled`). Copyright (c) Intel Corporation.

use crate::error::ReadoutError;
use crate::regs::*;
use crate::types::Ddi;

/// The reference clock of the SKL DPLLs: `display->dpll.ref_clks.nssc = display->cdclk.hw.ref`
/// (`skl_update_dpll_ref_clks`, intel_dpll_mgr.c:1997-2001), and `skl_dpll0_update` sets `ref` to 24000 kHz
/// (intel_cdclk.c:970).
pub const SKL_REF_CLOCK_KHZ: u32 = 24000;

/// DPLL ids (`SKL_DPLL0`..`SKL_DPLL3`, intel_dpll_mgr.h:402-405).
pub const NUM_DPLLS: usize = 4;

/// A DDI's clock routing in DPLL_CTRL2 (`skl_ddi_get_pll`, intel_ddi.c:1998-2018).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdiClock {
    /// `DPLL_CTRL2_DDI_SEL_OVERRIDE(port)`: the select field is honoured. Clear means Linux sees "no PLL"
    /// (`skl_ddi_get_pll` returns NULL, intel_ddi.c:2011-2012).
    pub select_override: bool,
    /// `DPLL_CTRL2_DDI_CLK_OFF(port)`: the DDI clock is gated (`skl_ddi_is_clock_enabled` is the inverse, intel_ddi.c:1986-1996).
    pub clk_off: bool,
    /// `DPLL_CTRL2_DDI_CLK_SEL(port)`: the DPLL id the port takes its clock from.
    pub pll: u8,
}

/// `DPLL_CTRL2_DDI_CLK_OFF(port)` = `1 << (port + 15)` (intel_display_regs.h:2572).
pub const fn ctrl2_clk_off(port: Ddi) -> u32 {
    1 << (port as u32 + 15)
}
/// `DPLL_CTRL2_DDI_CLK_SEL_SHIFT(port)` = `port * 3 + 1`; the mask is `3 <<` that (intel_display_regs.h:2573-2574).
pub const fn ctrl2_clk_sel_shift(port: Ddi) -> u32 {
    port as u32 * 3 + 1
}
/// `DPLL_CTRL2_DDI_SEL_OVERRIDE(port)` = `1 << (port * 3)` (intel_display_regs.h:2576).
pub const fn ctrl2_sel_override(port: Ddi) -> u32 {
    1 << (port as u32 * 3)
}

pub fn decode_ctrl2(port: Ddi, ctrl2: u32) -> DdiClock {
    DdiClock {
        select_override: ctrl2 & ctrl2_sel_override(port) != 0,
        clk_off: ctrl2 & ctrl2_clk_off(port) != 0,
        pll: ((ctrl2 >> ctrl2_clk_sel_shift(port)) & 3) as u8,
    }
}

/// The control register of DPLL `id` (`skl_dpll_regs[id].ctl`, intel_dpll_mgr.c:1356-1379): the enable bit is bit 31.
pub const fn dpll_ctl(id: u8) -> u32 {
    match id {
        0 => LCPLL1_CTL,
        1 => LCPLL2_CTL,
        2 => WRPLL_CTL1,
        _ => WRPLL_CTL2,
    }
}
/// `DPLL_CFGCR1(id)` for id 1..=3 (`_MMIO_PIPE(id - SKL_DPLL1, _DPLL1_CFGCR1, _DPLL2_CFGCR1)`, intel_display_regs.h:2586).
pub const fn dpll_cfgcr1(id: u8) -> u32 {
    DPLL1_CFGCR1 + (id as u32 - 1) * (DPLL2_CFGCR1 - DPLL1_CFGCR1)
}
/// `DPLL_CFGCR2(id)` for id 1..=3 (intel_display_regs.h:2595).
pub const fn dpll_cfgcr2(id: u8) -> u32 {
    DPLL1_CFGCR2 + (id as u32 - 1) * (DPLL2_CFGCR2 - DPLL1_CFGCR2)
}

/// `DPLL_CTRL1_HDMI_MODE(id)` / `_SSC` / `_LINK_RATE` / `_OVERRIDE` (intel_display_regs.h:2557-2562).
pub const fn ctrl1_hdmi_mode(id: u8) -> u32 {
    1 << (id as u32 * 6 + 5)
}
pub const fn ctrl1_ssc(id: u8) -> u32 {
    1 << (id as u32 * 6 + 4)
}
pub const fn ctrl1_link_rate_mask(id: u8) -> u32 {
    7 << (id as u32 * 6 + 1)
}
pub const fn ctrl1_link_rate_shift(id: u8) -> u32 {
    id as u32 * 6 + 1
}
pub const fn ctrl1_override(id: u8) -> u32 {
    1 << (id as u32 * 6)
}
/// `DPLL_LOCK(id)` = `1 << (id * 8)` in DPLL_STATUS (intel_display_regs.h:2580).
pub const fn dpll_lock(id: u8) -> u32 {
    1 << (id as u32 * 8)
}

/// `skl_ddi_lcpll_get_freq`: DPLL_CTRL1 link-rate field -> the DPLL's link clock in kHz, BEFORE the `* 2`
/// that makes it the port clock (intel_dpll_mgr.c:1902-1935). 810/1080/1350/1620/2160/2700 name 10 kHz units.
pub fn lcpll_link_clock_khz(field: u32) -> Option<u32> {
    match field {
        DPLL_CTRL1_LINK_RATE_810 => Some(81000),    // intel_dpll_mgr.c:1911-1912
        DPLL_CTRL1_LINK_RATE_1080 => Some(108000),  // intel_dpll_mgr.c:1914-1915
        DPLL_CTRL1_LINK_RATE_1350 => Some(135000),  // intel_dpll_mgr.c:1917-1918
        DPLL_CTRL1_LINK_RATE_1620 => Some(162000),  // intel_dpll_mgr.c:1920-1921
        DPLL_CTRL1_LINK_RATE_2160 => Some(216000),  // intel_dpll_mgr.c:1923-1924
        DPLL_CTRL1_LINK_RATE_2700 => Some(270000),  // intel_dpll_mgr.c:1926-1927
        _ => None,                                  // "Unsupported link rate", intel_dpll_mgr.c:1930
    }
}

/// `skl_ddi_wrpll_get_freq` (intel_dpll_mgr.c:1754-1824): the HDMI pixel clock in kHz from CFGCR1/CFGCR2.
pub fn wrpll_freq_khz(id: u8, cfgcr1: u32, cfgcr2: u32) -> Result<u32, ReadoutError> {
    let p0 = match cfgcr2 & DPLL_CFGCR2_PDIV_MASK {
        DPLL_CFGCR2_PDIV_1 => 1,
        DPLL_CFGCR2_PDIV_2 => 2,
        DPLL_CFGCR2_PDIV_3 => 3,
        // The HW seems to ignore bit 0 of the invalid 7 (a real ASUS Z170M BIOS programmed it): same as PDIV_7.
        DPLL_CFGCR2_PDIV_7_INVALID | DPLL_CFGCR2_PDIV_7 => 7,
        other => return Err(ReadoutError::WrpllPdivReserved { pll: id, field: other >> 2 }),
    };
    let p2 = match cfgcr2 & DPLL_CFGCR2_KDIV_MASK {
        DPLL_CFGCR2_KDIV_5 => 5,
        DPLL_CFGCR2_KDIV_2 => 2,
        DPLL_CFGCR2_KDIV_3 => 3,
        _ => 1, // DPLL_CFGCR2_KDIV_1: the field has exactly these four values
    };
    let p1 = if cfgcr2 & DPLL_CFGCR2_QDIV_MODE_BIT != 0 {
        (cfgcr2 & DPLL_CFGCR2_QDIV_RATIO_MASK) >> 8 // intel_dpll_mgr.c:1766
    } else {
        1
    };
    let ref_clock = SKL_REF_CLOCK_KHZ;
    let mut dco = (cfgcr1 & DPLL_CFGCR1_DCO_INTEGER_MASK) * ref_clock; // intel_dpll_mgr.c:1814-1815
    dco += ((cfgcr1 & DPLL_CFGCR1_DCO_FRACTION_MASK) >> 9) * ref_clock / 0x8000; // intel_dpll_mgr.c:1817-1818
    if p0 == 0 || p1 == 0 || p2 == 0 {
        return Err(ReadoutError::WrpllDividerZero { pll: id }); // drm_WARN_ON, intel_dpll_mgr.c:1820
    }
    Ok(dco / (p0 * p1 * p2 * 5)) // intel_dpll_mgr.c:1823
}

/// One DPLL as the hardware holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpllState {
    pub id: u8,
    /// Enable bit 31 of the PLL's control register.
    pub enabled: bool,
    /// `DPLL_LOCK(id)` in DPLL_STATUS.
    pub locked: bool,
    /// This PLL's 6-bit slice of DPLL_CTRL1 (`hw_state->ctrl1 = (val >> (id * 6)) & 0x3f`, intel_dpll_mgr.c:1466).
    pub ctrl1: u32,
    pub hdmi_mode: bool,
    pub ssc: bool,
    pub override_set: bool,
    /// The raw 3-bit link-rate field (meaningful in DP mode).
    pub link_rate_field: u32,
    /// CFGCR1/CFGCR2 (only read, as in Linux, when HDMI mode is on and the PLL is not DPLL0).
    pub cfgcr1: u32,
    pub cfgcr2: u32,
    /// The port clock this PLL makes in kHz (`skl_ddi_pll_get_freq`); 0 if the PLL is off. DP: the link symbol rate
    /// 162000 / 270000 / 540000 (`link_clock * 2`, intel_dpll_mgr.c:1934). HDMI/DVI: the WRPLL pixel clock.
    pub port_clock_khz: u32,
}

/// Decode one PLL. `ctrl1` is the whole DPLL_CTRL1 word.
pub fn decode_dpll(id: u8, ctl: u32, status: u32, ctrl1: u32, cfgcr1: u32, cfgcr2: u32) -> Result<DpllState, ReadoutError> {
    let enabled = ctl & LCPLL_PLL_ENABLE != 0; // "the enable bit is always bit 31", intel_dpll_mgr.c:1411
    let slice = (ctrl1 >> (id as u32 * 6)) & 0x3f;
    let hdmi_mode = ctrl1 & ctrl1_hdmi_mode(id) != 0;
    let link_rate_field = (ctrl1 & ctrl1_link_rate_mask(id)) >> ctrl1_link_rate_shift(id);
    let mut st = DpllState {
        id,
        enabled,
        locked: status & dpll_lock(id) != 0,
        ctrl1: slice,
        hdmi_mode,
        ssc: ctrl1 & ctrl1_ssc(id) != 0,
        override_set: ctrl1 & ctrl1_override(id) != 0,
        link_rate_field,
        cfgcr1: 0,
        cfgcr2: 0,
        port_clock_khz: 0,
    };
    if !enabled {
        return Ok(st);
    }
    if hdmi_mode {
        st.cfgcr1 = cfgcr1;
        st.cfgcr2 = cfgcr2;
        st.port_clock_khz = wrpll_freq_khz(id, cfgcr1, cfgcr2)?;
    } else {
        match lcpll_link_clock_khz(link_rate_field) {
            Some(k) => st.port_clock_khz = k * 2,
            None => return Err(ReadoutError::LinkRateReserved { pll: id, field: link_rate_field }),
        }
    }
    Ok(st)
}

/// The DP link rate in Gbit/s * 100 (162 = 1.62, 270 = 2.70, 540 = 5.40) for a DP port clock in kHz.
pub const fn dp_rate_cgbps(port_clock_khz: u32) -> u32 {
    port_clock_khz / 1000
}
