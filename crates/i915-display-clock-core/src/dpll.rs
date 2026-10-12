// SPDX-License-Identifier: GPL-2.0-only
//! Skylake / Kaby Lake DPLL0..3: DisplayPort link-rate selection, the enable / disable sequences, DDI clock routing, and the
//! shared-PLL allocator.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dpll_mgr.c (`skl_compute_dpll`, `skl_dpll_regs`, `skl_ddi_pll_write_ctrl1`,
//! `skl_ddi_pll_enable`, `skl_ddi_dpll0_enable`, `skl_ddi_pll_disable`, `skl_ddi_dp_set_dpll_hw_state`, `skl_get_dpll`,
//! `intel_find_dpll`, `skl_plls`) and intel_ddi.c (`skl_ddi_enable_clock`, `skl_ddi_disable_clock`) at v7.0-rc6.
//! Copyright (c) Intel Corporation. The HDMI divider solver is in `wrpll`; DPLL0's LCPLL1 enable/lock that CDCLK owns is in `cdclk`.
//!
//! DPLL0 is `.always_on` in Linux: its VCO is CDCLK's, so `skl_ddi_dpll0_enable` writes only DPLL_CTRL1 (the link rate) and
//! `skl_ddi_dpll0_disable` does nothing (intel_dpll_mgr.c:1418-1425, :1438-1441).
use crate::cdclk::{VCO_8100_KHZ, VCO_8640_KHZ};
use crate::error::{ClockError, Handshake};
use crate::plan::{OnTimeout, PollPlan, Transaction, WritePlan};
use crate::regs::*;
use crate::wrpll::{hdmi_hw_state, SklDpllHwState};
use i915_common_core::Reg;
use i915_kbl_display_core::dpll::{
    ctrl1_hdmi_mode, ctrl1_link_rate_mask, ctrl1_link_rate_shift, ctrl1_override, ctrl1_ssc, ctrl2_clk_off, ctrl2_clk_sel_shift,
    ctrl2_sel_override, dpll_cfgcr1, dpll_cfgcr2, dpll_ctl, dpll_lock,
};
use i915_kbl_display_core::types::Ddi;

/// `skl_ddi_pll_enable` waits up to 5 ms for DPLL_STATUS lock (intel_dpll_mgr.c:1415).
pub const DPLL_LOCK_TIMEOUT_MS: u32 = 5; // intel_dpll_mgr.c:1415

/// `skl_ddi_dp_set_dpll_hw_state` (intel_dpll_mgr.c:1864-1900): the DPLL_CTRL1 word for a DP / eDP link, from `port_clock`
/// (the link rate in kHz: 162000, 270000, 540000, and the eDP 1.4 rates 324000, 216000, 432000). Linux's `switch` has no default,
/// so an unknown rate leaves only the override bit (rate field 0 = 5.4 Gbit/s); this refuses it.
pub fn dp_hw_state(port_clock_khz: u32) -> Result<SklDpllHwState, ClockError> {
    let field = match port_clock_khz / 2 {
        81000 => DPLL_CTRL1_LINK_RATE_810,    // intel_dpll_mgr.c:1877-1879
        135000 => DPLL_CTRL1_LINK_RATE_1350,  // intel_dpll_mgr.c:1880-1882
        270000 => DPLL_CTRL1_LINK_RATE_2700,  // intel_dpll_mgr.c:1883-1885
        162000 => DPLL_CTRL1_LINK_RATE_1620,  // eDP 1.4, intel_dpll_mgr.c:1886-1888
        108000 => DPLL_CTRL1_LINK_RATE_1080,  // intel_dpll_mgr.c:1889-1891
        216000 => DPLL_CTRL1_LINK_RATE_2160,  // intel_dpll_mgr.c:1892-1894
        _ => return Err(ClockError::DpRateUnsupported { port_clock_khz }),
    };
    Ok(SklDpllHwState { ctrl1: ctrl1_override(0) | (field << ctrl1_link_rate_shift(0)), cfgcr1: 0, cfgcr2: 0 })
}

/// The DPLL0 VCO a DP_CTRL1 link-rate field needs: the eDP 1.4 rates 2.16 / 4.32 Gbit/s (fields 1080 / 2160, port clock 216000 /
/// 432000) need 8640 MHz, every other rate 8100 MHz (`skl_dpll0_vco`, intel_cdclk.c:3192-3197; `skl_calc_cdclk` and `lcpll_link_clock_khz` agree).
pub const fn link_rate_vco_khz(link_rate_field: u32) -> u32 {
    match link_rate_field {
        DPLL_CTRL1_LINK_RATE_1080 | DPLL_CTRL1_LINK_RATE_2160 => VCO_8640_KHZ, // intel_cdclk.c:996-998
        _ => VCO_8100_KHZ,                                                     // intel_cdclk.c:990-994
    }
}

/// What a CRTC's encoder needs a clock for (`intel_crtc_has_type(INTEL_OUTPUT_HDMI)` / `intel_crtc_has_dp_encoder`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// HDMI / DVI at this pixel clock, kHz.
    Hdmi { clock_khz: u32 },
    /// DP / eDP at this link rate (`port_clock`), kHz.
    Dp { port_clock_khz: u32 },
}

/// `skl_compute_dpll` (intel_dpll_mgr.c:1937-1950), the body `hsw_crtc_compute_clock` reaches for display ver 9 through
/// `intel_dpll_compute` (intel_dpll.c:1152-1179, :1805): the DPLL hardware state for the link, and the `port_clock` the
/// programmed state produces (HDMI: what the WRPLL words really give; DP: the requested rate).
pub fn compute_dpll(link: Link) -> Result<(SklDpllHwState, u32), ClockError> {
    match link {
        Link::Hdmi { clock_khz } => hdmi_hw_state(clock_khz),
        Link::Dp { port_clock_khz } => Ok((dp_hw_state(port_clock_khz)?, port_clock_khz)),
    }
}

/// The registers one DPLL is programmed through at the moment a plan is made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpllSnapshot {
    pub ctrl1: u32,
    pub cfgcr1: u32,
    pub cfgcr2: u32,
    /// LCPLL1/LCPLL2/WRPLL_CTL1/WRPLL_CTL2 for the PLL.
    pub ctl: u32,
}

const fn ctrl1_field_mask(id: u8) -> u32 {
    ctrl1_hdmi_mode(id) | ctrl1_ssc(id) | ctrl1_link_rate_mask(id)
}

/// `skl_ddi_pll_write_ctrl1` (intel_dpll_mgr.c:1382-1394): RMW the PLL's slice of DPLL_CTRL1, then a posting read.
fn write_ctrl1(p: &mut WritePlan, id: u8, hw: &SklDpllHwState) -> Result<(), ClockError> {
    p.rmw(DPLL_CTRL1, ctrl1_field_mask(id), hw.ctrl1 << (id as u32 * 6))?; // intel_dpll_mgr.c:1388-1392
    p.posting_read(DPLL_CTRL1) // intel_dpll_mgr.c:1393
}

/// Restore the PLL's DPLL_CTRL1 slice (override bit included) to the snapshot.
fn restore_ctrl1(p: &mut WritePlan, id: u8, snap: &DpllSnapshot) -> Result<(), ClockError> {
    let m = ctrl1_field_mask(id) | ctrl1_override(id);
    p.rmw(DPLL_CTRL1, m, snap.ctrl1 & m)?;
    p.posting_read(DPLL_CTRL1)
}

fn check_id(id: u8) -> Result<(), ClockError> {
    if id > 3 {
        Err(ClockError::NoSuchDpll { id })
    } else {
        Ok(())
    }
}

/// `skl_ddi_pll_enable` (DPLL 1..3, intel_dpll_mgr.c:1396-1416) or `skl_ddi_dpll0_enable` (DPLL 0, :1418-1425).
///
/// Rollback: stop the PLL (if it was off in the snapshot), then put DPLL_CTRL1's slice and both CFGCR words back.
pub fn plan_enable(id: u8, hw: &SklDpllHwState, snap: &DpllSnapshot) -> Result<Transaction, ClockError> {
    check_id(id)?;
    let mut fwd = WritePlan::new();
    let mut rb = WritePlan::new();
    write_ctrl1(&mut fwd, id, hw)?;
    if id != 0 {
        fwd.write(dpll_cfgcr1(id), hw.cfgcr1)?; // intel_dpll_mgr.c:1406
        fwd.write(dpll_cfgcr2(id), hw.cfgcr2)?; // intel_dpll_mgr.c:1407
        fwd.posting_read(dpll_cfgcr1(id))?; // intel_dpll_mgr.c:1408
        fwd.posting_read(dpll_cfgcr2(id))?; // intel_dpll_mgr.c:1409
        fwd.rmw(dpll_ctl(id), 0, LCPLL_PLL_ENABLE)?; // "the enable bit is always bit 31", intel_dpll_mgr.c:1412
        // Linux: drm_err("DPLL %d not locked") and carry on. Here the transaction fails and the executor rolls back.
        fwd.poll(PollPlan::wait_set(Handshake::DpllLock(id), Reg::new(DPLL_STATUS), dpll_lock(id), DPLL_LOCK_TIMEOUT_MS, OnTimeout::Fail))?; // :1415

        if snap.ctl & LCPLL_PLL_ENABLE == 0 {
            rb.rmw(dpll_ctl(id), LCPLL_PLL_ENABLE, 0)?;
            rb.posting_read(dpll_ctl(id))?;
        }
        rb.write(dpll_cfgcr1(id), snap.cfgcr1)?;
        rb.write(dpll_cfgcr2(id), snap.cfgcr2)?;
        // Not in Linux (it has no rollback): a restore that a clock-gated display domain dropped must not report success.
        rb.posting_read(dpll_cfgcr1(id))?;
        rb.posting_read(dpll_cfgcr2(id))?;
    }
    restore_ctrl1(&mut rb, id, snap)?;
    Ok(Transaction { forward: fwd, rollback: rb })
}

/// DPLL0 for an eDP link: refuse a link rate whose VCO family is not the one DPLL0 (CDCLK's PLL) is running, then
/// [`plan_enable`]. The VCO is chosen by CDCLK from the eDP port clock first (`skl_dpll0_vco`); this is the check that the two agree.
pub fn plan_dpll0_edp(hw: &SklDpllHwState, running_vco_khz: u32, snap: &DpllSnapshot) -> Result<Transaction, ClockError> {
    let field = (hw.ctrl1 & ctrl1_link_rate_mask(0)) >> ctrl1_link_rate_shift(0);
    let needs = link_rate_vco_khz(field);
    if needs != running_vco_khz {
        return Err(ClockError::Dpll0VcoMismatch { needs_khz: needs, running_khz: running_vco_khz });
    }
    plan_enable(0, hw, snap)
}

/// `skl_ddi_pll_disable` (intel_dpll_mgr.c:1427-1441) for DPLL 1..3; DPLL 0 is `skl_ddi_dpll0_disable`, an empty plan.
/// `current` is the state being switched off, so the rollback can switch it back on.
pub fn plan_disable(id: u8, current: &SklDpllHwState, snap: &DpllSnapshot) -> Result<Transaction, ClockError> {
    check_id(id)?;
    if id == 0 {
        return Ok(Transaction::new());
    }
    let mut fwd = WritePlan::new();
    fwd.rmw(dpll_ctl(id), LCPLL_PLL_ENABLE, 0)?; // intel_dpll_mgr.c:1434
    fwd.posting_read(dpll_ctl(id))?; // intel_dpll_mgr.c:1435
    let rb = if snap.ctl & LCPLL_PLL_ENABLE != 0 { plan_enable(id, current, snap)?.forward } else { WritePlan::new() };
    Ok(Transaction { forward: fwd, rollback: rb })
}

/// `skl_ddi_enable_clock` (intel_ddi.c:1952-1971): route DDI `port` to DPLL `pll`.
pub fn plan_ddi_clock_enable(port: Ddi, pll: u8, ctrl2_snapshot: u32) -> Result<Transaction, ClockError> {
    check_id(pll)?;
    let sel_mask = 3 << ctrl2_clk_sel_shift(port);
    let mut fwd = WritePlan::new();
    fwd.rmw(DPLL_CTRL2, ctrl2_clk_off(port) | sel_mask, ((pll as u32) << ctrl2_clk_sel_shift(port)) | ctrl2_sel_override(port))?; // intel_ddi.c:1964-1968
    // Linux does not post here; we do (REGISTER-WRITE-READBACK): a dropped CTRL2 write would otherwise report success.
    fwd.posting_read(DPLL_CTRL2)?;
    let mut rb = WritePlan::new();
    let m = ctrl2_clk_off(port) | sel_mask | ctrl2_sel_override(port);
    rb.rmw(DPLL_CTRL2, m, ctrl2_snapshot & m)?;
    rb.posting_read(DPLL_CTRL2)?;
    Ok(Transaction { forward: fwd, rollback: rb })
}

/// `skl_ddi_disable_clock` (intel_ddi.c:1973-1984): gate DDI `port`'s clock.
pub fn plan_ddi_clock_disable(port: Ddi, ctrl2_snapshot: u32) -> Result<Transaction, ClockError> {
    let mut fwd = WritePlan::new();
    fwd.rmw(DPLL_CTRL2, 0, ctrl2_clk_off(port))?; // intel_ddi.c:1980-1981
    fwd.posting_read(DPLL_CTRL2)?; // not in Linux; see plan_ddi_clock_enable
    let mut rb = WritePlan::new();
    rb.rmw(DPLL_CTRL2, ctrl2_clk_off(port), ctrl2_snapshot & ctrl2_clk_off(port))?;
    rb.posting_read(DPLL_CTRL2)?;
    Ok(Transaction { forward: fwd, rollback: rb })
}

/// Which PLLs `skl_get_dpll` may hand a CRTC (intel_dpll_mgr.c:1952-1979).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputKind {
    /// `INTEL_OUTPUT_EDP`: only DPLL0 (its VCO is the display core clock's).
    Edp,
    /// Everything else: DPLL 1, 2 or 3.
    Other,
}

impl OutputKind {
    /// `BIT(DPLL_ID_SKL_DPLL0)` / `BIT(DPLL3) | BIT(DPLL2) | BIT(DPLL1)` (intel_dpll_mgr.c:1960-1969).
    pub const fn allowed_mask(self) -> u8 {
        match self {
            OutputKind::Edp => 0b0001,
            OutputKind::Other => 0b1110,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slot {
    users: u8,
    hw: SklDpllHwState,
}

/// The shared-PLL bookkeeping (`dpll_state[]` `pipe_mask` / `hw_state`): who holds each PLL and with what state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpllAllocator {
    slots: [Slot; 4],
}

const EMPTY: Slot = Slot { users: 0, hw: SklDpllHwState { ctrl1: 0, cfgcr1: 0, cfgcr2: 0 } };

impl DpllAllocator {
    pub const fn new() -> DpllAllocator {
        DpllAllocator { slots: [EMPTY; 4] }
    }
    pub fn users(&self, id: u8) -> u8 {
        self.slots.get(id as usize).map_or(0, |s| s.users)
    }
    /// Record a PLL the firmware left running with this state and one user (readout adoption).
    pub fn adopt(&mut self, id: u8, hw: SklDpllHwState) -> Result<(), ClockError> {
        check_id(id)?;
        self.slots[id as usize] = Slot { users: 1, hw };
        Ok(())
    }
    /// `intel_find_dpll` (intel_dpll_mgr.c:376-427) then `intel_reference_dpll`: scan the allowed PLLs in ascending id order,
    /// share the first IN-USE one whose state is identical, else take the first unused one.
    pub fn allocate(&mut self, kind: OutputKind, hw: &SklDpllHwState) -> Result<u8, ClockError> {
        let mask = kind.allowed_mask();
        let mut unused: Option<u8> = None;
        for id in 0..4u8 {
            if mask & (1 << id) == 0 {
                continue;
            }
            let s = &self.slots[id as usize];
            if s.users == 0 {
                if unused.is_none() {
                    unused = Some(id); // intel_dpll_mgr.c:400-404
                }
                continue;
            }
            if s.hw == *hw {
                self.slots[id as usize].users += 1; // "sharing existing", intel_dpll_mgr.c:406-416
                return Ok(id);
            }
        }
        match unused {
            Some(id) => {
                self.slots[id as usize] = Slot { users: 1, hw: *hw }; // "allocated", intel_dpll_mgr.c:420-425
                Ok(id)
            }
            None => Err(ClockError::NoFreeDpll { allowed_mask: mask }), // intel_dpll_mgr.c:427
        }
    }
    /// `intel_put_dpll`: drop one user. Returns `true` when it was the last (the PLL may be disabled).
    pub fn release(&mut self, id: u8) -> Result<bool, ClockError> {
        check_id(id)?;
        let s = &mut self.slots[id as usize];
        if s.users == 0 {
            return Err(ClockError::UnbalancedPut { what: "dpll" });
        }
        s.users -= 1;
        Ok(s.users == 0)
    }
}

impl Default for DpllAllocator {
    fn default() -> Self {
        DpllAllocator::new()
    }
}
