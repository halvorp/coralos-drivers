// SPDX-License-Identifier: GPL-2.0-only
//! The Skylake HDMI WRPLL divider solver: pixel clock -> DCO central frequency + P/Q/K dividers -> DPLL_CFGCR1 / CFGCR2.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dpll_mgr.c (`struct skl_wrpll_context`, `skl_wrpll_try_divider`,
//! `skl_wrpll_get_multipliers`, `struct skl_wrpll_params`, `skl_wrpll_params_populate`, `skl_ddi_calculate_wrpll`,
//! `skl_ddi_hdmi_pll_dividers`) at v7.0-rc6. Copyright (c) Intel Corporation.
//!
//! The arithmetic is integer and exact; every division is the same truncating division as the C (`div64_u64`, `div_u64`).
//! Intermediate values are `u64` as in Linux, so there is no overflow for any `u32` clock (`clock * 5000 * 98 < 2^64`).
use crate::error::ClockError;
use crate::regs::*;
use i915_kbl_display_core::dpll::{ctrl1_hdmi_mode, ctrl1_override, wrpll_freq_khz, SKL_REF_CLOCK_KHZ};

/// "DCO freq must be within +1%/-6% of the DCO central freq" (intel_dpll_mgr.c:1522-1524), in units of 0.01 %.
pub const DCO_MAX_PDEVIATION: u64 = 100; // intel_dpll_mgr.c:1523
pub const DCO_MAX_NDEVIATION: u64 = 600; // intel_dpll_mgr.c:1524

/// The three DCO central frequencies, Hz (intel_dpll_mgr.c:1688-1690), tried in this order.
pub const DCO_CENTRAL_FREQ_HZ: [u64; 3] = [8_400_000_000, 9_000_000_000, 9_600_000_000];

/// `even_dividers` (intel_dpll_mgr.c:1691-1695).
pub const EVEN_DIVIDERS: [u8; 36] = [
    4, 6, 8, 10, 12, 14, 16, 18, 20, 24, 28, 30, 32, 36, 40, 42, 44, 48, 52, 54, 56, 60, 64, 66, 68, 70, 72, 76, 78, 80, 84, 88, 90, 92, 96, 98,
];
/// `odd_dividers` (intel_dpll_mgr.c:1696).
pub const ODD_DIVIDERS: [u8; 7] = [3, 5, 7, 9, 15, 21, 35];

/// `struct skl_wrpll_params` (intel_dpll_mgr.c:1604-1612): the register-field values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WrpllParams {
    pub dco_fraction: u32,
    pub dco_integer: u32,
    pub qdiv_ratio: u32,
    pub qdiv_mode: u32,
    pub kdiv: u32,
    pub pdiv: u32,
    pub central_freq: u32,
}

/// `skl_wrpll_get_multipliers` (intel_dpll_mgr.c:1555-1602): the divider `p` as `(p0, p1, p2)`. Linux leaves the outputs
/// untouched (zero in the caller) for an `p` it cannot factor; that is `None` here, though no entry of either divider list
/// reaches it.
pub const fn multipliers(p: u32) -> Option<(u32, u32, u32)> {
    if p % 2 == 0 {
        let half = p / 2;
        if half == 1 || half == 2 || half == 3 || half == 5 {
            Some((2, 1, half)) // intel_dpll_mgr.c:1564-1567
        } else if half % 2 == 0 {
            Some((2, half / 2, 2)) // intel_dpll_mgr.c:1568-1571
        } else if half % 3 == 0 {
            Some((3, half / 3, 2)) // intel_dpll_mgr.c:1572-1575
        } else if half % 7 == 0 {
            Some((7, half / 7, 2)) // intel_dpll_mgr.c:1576-1579
        } else {
            None
        }
    } else if p == 3 || p == 9 {
        Some((3, 1, p / 3)) // intel_dpll_mgr.c:1581-1584
    } else if p == 5 || p == 7 {
        Some((p, 1, 1)) // intel_dpll_mgr.c:1585-1588
    } else if p == 15 {
        Some((3, 1, 5)) // intel_dpll_mgr.c:1589-1592
    } else if p == 21 {
        Some((7, 1, 3)) // intel_dpll_mgr.c:1593-1596
    } else if p == 35 {
        Some((7, 1, 5)) // intel_dpll_mgr.c:1597-1600
    } else {
        None
    }
}

/// `skl_wrpll_params_populate` (intel_dpll_mgr.c:1614-1680). `afe_clock` is in Hz; `ref_clock` in kHz.
/// Returns `None` where Linux `WARN`s (a P0 or P2 with no field encoding, or a central frequency it does not know).
pub fn params_populate(afe_clock_hz: u64, ref_clock_khz: u32, central_freq_hz: u64, p0: u32, p1: u32, p2: u32) -> Option<WrpllParams> {
    let central_freq = match central_freq_hz {
        9_600_000_000 => 0, // intel_dpll_mgr.c:1623-1625
        9_000_000_000 => 1, // intel_dpll_mgr.c:1626-1628
        8_400_000_000 => 3, // intel_dpll_mgr.c:1629-1630
        _ => return None,
    };
    let pdiv = match p0 {
        1 => 0, // intel_dpll_mgr.c:1634-1636
        2 => 1, // intel_dpll_mgr.c:1637-1639
        3 => 2, // intel_dpll_mgr.c:1640-1642
        7 => 4, // intel_dpll_mgr.c:1643-1645
        _ => return None, // WARN(1, "Incorrect PDiv\n"), intel_dpll_mgr.c:1647
    };
    let kdiv = match p2 {
        5 => 0, // intel_dpll_mgr.c:1651-1653
        2 => 1, // intel_dpll_mgr.c:1654-1656
        3 => 2, // intel_dpll_mgr.c:1657-1659
        1 => 3, // intel_dpll_mgr.c:1660-1662
        _ => return None, // WARN(1, "Incorrect KDiv\n"), intel_dpll_mgr.c:1664
    };
    let qdiv_ratio = p1; // intel_dpll_mgr.c:1667
    let qdiv_mode = if qdiv_ratio == 1 { 0 } else { 1 }; // intel_dpll_mgr.c:1668
    let dco_freq = p0 as u64 * p1 as u64 * p2 as u64 * afe_clock_hz; // intel_dpll_mgr.c:1670
    let ref_khz = ref_clock_khz as u64;
    // Intermediate values are in Hz. Divide by MHz to match bspec (intel_dpll_mgr.c:1672-1679)
    let dco_integer = dco_freq / (ref_khz * 1000);
    let dco_fraction = ((dco_freq / (ref_khz / 1000) - dco_integer * 1_000_000) * 0x8000) / 1_000_000;
    Some(WrpllParams {
        dco_fraction: dco_fraction as u32,
        dco_integer: dco_integer as u32,
        qdiv_ratio,
        qdiv_mode,
        kdiv,
        pdiv,
        central_freq,
    })
}

/// `skl_ddi_calculate_wrpll` (intel_dpll_mgr.c:1682-1752): search the DCO central frequencies and dividers for the
/// smallest-deviation solution. An EVEN divider solution always wins over an odd one (intel_dpll_mgr.c:1730-1736).
pub fn calculate_wrpll(clock_khz: u32, ref_clock_khz: u32) -> Result<WrpllParams, ClockError> {
    // No divider reaches the DCO window outside ~16.1..646.4 MHz, so Linux returns -EINVAL for such a clock; refusing early gives the
    // identical answer and keeps `10000 * deviation` inside u64 for every input.
    if clock_khz > 1_000_000 {
        return Err(ClockError::WrpllNoDivider { clock_khz });
    }
    let afe_clock = clock_khz as u64 * 1000 * 5; // AFE Clock is 5x Pixel clock, in Hz (intel_dpll_mgr.c:1708)
    let mut min_deviation = u64::MAX;
    let mut central_freq = 0u64;
    let mut best_p = 0u32;

    'lists: for (d, list) in [&EVEN_DIVIDERS[..], &ODD_DIVIDERS[..]].into_iter().enumerate() {
        'dco: for &central in DCO_CENTRAL_FREQ_HZ.iter() {
            for &p in list {
                let dco_freq = p as u64 * afe_clock;
                // skl_wrpll_try_divider (intel_dpll_mgr.c:1526-1553)
                let deviation = 10000 * dco_freq.abs_diff(central) / central;
                if dco_freq >= central {
                    if deviation < DCO_MAX_PDEVIATION && deviation < min_deviation {
                        min_deviation = deviation;
                        central_freq = central;
                        best_p = p as u32;
                    }
                } else if deviation < DCO_MAX_NDEVIATION && deviation < min_deviation {
                    min_deviation = deviation;
                    central_freq = central;
                    best_p = p as u32;
                }
                // "Skip the remaining dividers if we're sure to have found the definitive divider" (intel_dpll_mgr.c:1720-1726)
                if min_deviation == 0 {
                    break 'dco;
                }
            }
        }
        // "If a solution is found with an even divider, prefer this one." (intel_dpll_mgr.c:1730-1736)
        if d == 0 && best_p != 0 {
            break 'lists;
        }
    }
    if best_p == 0 {
        return Err(ClockError::WrpllNoDivider { clock_khz }); // -EINVAL, intel_dpll_mgr.c:1739-1740
    }
    let (p0, p1, p2) = multipliers(best_p).ok_or(ClockError::WrpllDividerUnencodable { clock_khz, divider: best_p })?;
    params_populate(afe_clock, ref_clock_khz, central_freq, p0, p1, p2)
        .ok_or(ClockError::WrpllDividerUnencodable { clock_khz, divider: best_p })
}

/// `struct skl_dpll_hw_state` (intel_dpll_mgr.h): the three words one DPLL is programmed from. `ctrl1` is held at DPLL 0's
/// bit positions and shifted into the PLL's own by `ctrl1 << (id * 6)` when written (intel_dpll_mgr.c:1388-1392).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SklDpllHwState {
    pub ctrl1: u32,
    pub cfgcr1: u32,
    pub cfgcr2: u32,
}

/// The packed words for solved parameters (`skl_ddi_hdmi_pll_dividers`, intel_dpll_mgr.c:1838-1856).
pub const fn pack(p: &WrpllParams) -> SklDpllHwState {
    SklDpllHwState {
        // DPLL_CTRL1_OVERRIDE(0) | DPLL_CTRL1_HDMI_MODE(0)  (intel_dpll_mgr.c:1842-1844)
        ctrl1: ctrl1_override(0) | ctrl1_hdmi_mode(0),
        // DPLL_CFGCR1_FREQ_ENABLE | DPLL_CFGCR1_DCO_FRACTION(f) | integer  (intel_dpll_mgr.c:1846-1849)
        cfgcr1: DPLL_CFGCR1_FREQ_ENABLE | (p.dco_fraction << 9) | p.dco_integer,
        // QDIV_RATIO | QDIV_MODE | KDIV | PDIV | central freq  (intel_dpll_mgr.c:1851-1856)
        cfgcr2: (p.qdiv_ratio << 8) | (p.qdiv_mode << 7) | (p.kdiv << 5) | (p.pdiv << 2) | p.central_freq,
    }
}

/// `skl_ddi_hdmi_pll_dividers` (intel_dpll_mgr.c:1826-1863): the hardware state for an HDMI pixel clock, and the port clock the
/// programmed state really produces (`skl_ddi_wrpll_get_freq`, which Linux stores back into `crtc_state->port_clock`).
pub fn hdmi_hw_state(clock_khz: u32) -> Result<(SklDpllHwState, u32), ClockError> {
    let params = calculate_wrpll(clock_khz, SKL_REF_CLOCK_KHZ)?;
    let hw = pack(&params);
    // The readout's decode IS skl_ddi_wrpll_get_freq; reusing it keeps one decode of these two words in the family.
    let port_clock = wrpll_freq_khz(0, hw.cfgcr1, hw.cfgcr2).map_err(|_| ClockError::WrpllNoDivider { clock_khz })?;
    Ok((hw, port_clock))
}
