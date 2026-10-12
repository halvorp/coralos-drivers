// SPDX-License-Identifier: GPL-2.0-only
//! Clock-gating set-up for Kaby Lake: `gen9_init_clock_gating` (intel_clock_gating.c:49-74) then `kbl_init_clock_gating`
//! (:387-409). Every statement is `intel_uncore_rmw(reg, 0, set)` - read, OR, write back UNCONDITIONALLY - so the plan is a
//! list of [`RmwPlan`]s with `skip_if_unchanged = false`. Four of the registers (`CHICKEN_PAR1_1`, `GEN8_CHICKEN_DCPR_1`,
//! `DISP_ARB_CTL`) belong to the DISPLAY block: a device must hold the display power well for them, and the write-allow-list
//! must name them before any of this runs (plan section 5: planners only until an owner-attended silicon increment).
use crate::effect::{Effect, RmwPlan};
use crate::error::GtError;
use crate::fixed::FixedVec;
use crate::step::Step;
use crate::wa_tables::{Cond, WaContext};
use i915_common_core::Reg;
use i915_gt_regs_core as r;

/// One `intel_uncore_rmw(&i915->uncore, reg, 0, set)` call.
#[derive(Clone, Copy, Debug)]
pub struct ClockGatingRow {
    pub name: &'static str,
    pub src: &'static str,
    pub linux_reg: &'static str,
    pub reg: Reg,
    pub set: u32,
    pub cond: Cond,
}

const fn row(name: &'static str, src: &'static str, linux_reg: &'static str, reg: Reg, set: u32, cond: Cond) -> ClockGatingRow {
    ClockGatingRow { name, src, linux_reg, reg, set, cond }
}

/// `gen9_init_clock_gating` (intel_clock_gating.c:49-74).
pub static GEN9_CLOCK_GATING_ROWS: [ClockGatingRow; 4] = [
    row("WaCompressedResourceDisplayNewHashMode:skl,kbl / Display WA #0390", "intel_clock_gating.c:59", "CHICKEN_PAR1_1", r::CHICKEN_PAR1_1, r::SKL_DE_COMPRESSED_HASH_MODE, Cond::HasLlc),
    row("Wa#828:skl,bxt,kbl,cfl (PSR2_CTL bit 31)", "intel_clock_gating.c:63", "CHICKEN_PAR1_1", r::CHICKEN_PAR1_1, r::SKL_EDP_PSR_FIX_RDWRAP, Cond::Always),
    row("WaEnableChickenDCPR:skl,bxt,kbl,glk,cfl", "intel_clock_gating.c:66", "GEN8_CHICKEN_DCPR_1", r::GEN8_CHICKEN_DCPR_1, r::MASK_WAKEMEM, Cond::Always),
    row("WaFbcWakeMemOn:skl,bxt,kbl,glk,cfl / Display WA #0859", "intel_clock_gating.c:72", "DISP_ARB_CTL", r::DISP_ARB_CTL, r::DISP_FBC_MEMORY_WAKE, Cond::Always),
];

/// `kbl_init_clock_gating` (intel_clock_gating.c:387-409), after the Gen9 rows.
pub static KBL_CLOCK_GATING_ROWS: [ClockGatingRow; 4] = [
    row("WAC6entrylatency:kbl", "intel_clock_gating.c:392", "FBC_LLC_READ_CTRL", r::FBC_LLC_READ_CTRL, r::FBC_LLC_FULLY_OPEN, Cond::Always),
    row("WaDisableSDEUnitClockGating:kbl", "intel_clock_gating.c:396", "GEN8_UCGCTL6", r::GEN8_UCGCTL6, r::GEN8_SDEUNIT_CLOCK_GATE_DISABLE, Cond::KabylakeStep { since: Step::NONE, until: Step::C0 }),
    row("WaDisableGamClockGating:kbl", "intel_clock_gating.c:401", "GEN6_UCGCTL1", r::GEN6_UCGCTL1, r::GEN6_GAMUNIT_CLOCK_GATE_DISABLE, Cond::KabylakeStep { since: Step::NONE, until: Step::C0 }),
    row("WaFbcTurnOffFbcWatermark:kbl / Display WA #0562", "intel_clock_gating.c:408", "DISP_ARB_CTL", r::DISP_ARB_CTL, r::DISP_FBC_WM_DIS, Cond::Always),
];

/// The plan `kbl_init_clock_gating` executes for `ctx`, in Linux's order.
pub fn plan_kbl_init_clock_gating(ctx: &WaContext) -> Result<FixedVec<Effect, 8>, GtError> {
    let mut v = FixedVec::new("clock gating plan", Effect::FILL);
    for row in GEN9_CLOCK_GATING_ROWS.iter().chain(KBL_CLOCK_GATING_ROWS.iter()) {
        if row.cond.holds(ctx) {
            v.push(Effect::Rmw(RmwPlan { reg: row.reg, clear: 0, set: row.set, skip_if_unchanged: false }))?;
        }
    }
    Ok(v)
}
