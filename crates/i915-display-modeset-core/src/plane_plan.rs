// SPDX-License-Identifier: GPL-2.0-only
//! Plane, watermark / DDB and cursor effect plans: the register values of `i915-kbl-display-core` (`skl_plane`, `skl_wm`) put in the
//! order Linux writes them, each ending in the bounded confirmation Linux leaves to the next vblank.
//!
//! Ported from Linux drivers/gpu/drm/i915/display at v7.0-rc6: skl_universal_plane.c `skl_plane_update_noarm`,
//! `skl_plane_update_arm`, `skl_plane_disable_arm`, `skl_write_plane_wm` (the watermark block: `PLANE_WM(level)` for every level, then
//! `PLANE_WM_TRANS`, `PLANE_BUF_CFG`, and for display ver < 11 `PLANE_NV12_BUF_CFG`); intel_cursor.c `skl_write_cursor_wm`,
//! `i9xx_cursor_update_arm` (the no-state arm path: `CURPOS` then `CURBASE`). Copyright (c) Intel Corporation.
//!
//! DEVIATION: Linux does not wait for the surface latch; `PLANE_SURFLIVE` is read back here until it shows the programmed address,
//! under a hard sample bound ([`LATCH_TIMEOUT_US`] / [`LATCH_INTERVAL_US`] -> `max_polls`), because "the write was issued" is not
//! "the plane is scanning it out".
use crate::plan::{restore_plan, Plan};
use alloc::vec;
use i915_common_core::Reg;
use i915_display_link_core::plan::{Effect, PollPlan};
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::regs_write::*;
use i915_kbl_display_core::skl_plane::{PlaneRegs, RegWrites};
use i915_kbl_display_core::skl_wm::{cursor_ddb_reg_val, cursor_wm_reg_val, plane_ddb_reg_val, plane_wm_reg_val, Ddb, PipeWm, PlaneWm, WmLevel, NUM_LEVELS};
use i915_kbl_display_core::types::Pipe;

/// Spacing of `PLANE_SURFLIVE` reads while waiting for the latch.
pub const LATCH_INTERVAL_US: u32 = 500;
/// The latch happens at the next vblank: six frames at 60 Hz is the bound.
pub const LATCH_TIMEOUT_US: u32 = 100_000;

/// A `RegWrites` as `Write` effects, in order.
pub fn writes_plan<const N: usize>(w: &RegWrites<N>) -> Plan {
    w.as_slice().iter().map(|(r, v)| Effect::Write { reg: Reg::new(*r), value: *v }).collect()
}

/// The bounded wait for `PLANE_SURFLIVE` to show `surf` (address bits only).
pub fn latch_poll(pipe: Pipe, plane_id: u8, surf: u32) -> PollPlan {
    PollPlan::with_timeout(
        Reg::new(plane_reg(PLANE_SURFLIVE_1_A, pipe, plane_id as u32)),
        PLANE_SURF_ADDR_MASK,
        surf & PLANE_SURF_ADDR_MASK,
        LATCH_INTERVAL_US,
        LATCH_TIMEOUT_US,
        "PLANE_SURFLIVE latch",
    )
}

/// `skl_write_plane_wm()` - skl_universal_plane.c: `PLANE_WM(pipe, plane, level)` for every level, `PLANE_WM_TRANS`, `PLANE_BUF_CFG`
/// (`plane_ddb`), `PLANE_NV12_BUF_CFG` (`plane_ddb_y`, display ver < 11). The SAGV registers are `HAS_HW_SAGV_WM` (ver >= 12): absent.
pub fn plane_wm_writes(pipe: Pipe, plane_id: u8, pw: &PlaneWm, ddb: &Ddb, ddb_y: &Ddb) -> RegWrites<{ NUM_LEVELS + 3 }> {
    let mut w = RegWrites::new();
    for (level, l) in pw.wm.iter().enumerate() {
        w.push(plane_wm(pipe, plane_id as u32, level as u32), plane_wm_reg_val(l));
    }
    w.push(plane_reg(PLANE_WM_TRANS_1_A, pipe, plane_id as u32), plane_wm_reg_val(&pw.trans_wm));
    w.push(plane_reg(PLANE_BUF_CFG_1_A, pipe, plane_id as u32), plane_ddb_reg_val(ddb));
    w.push(plane_reg(PLANE_NV12_BUF_CFG_1_A, pipe, plane_id as u32), plane_ddb_reg_val(ddb_y));
    w
}

/// The two halves of a plane update, in Linux's order, plus the latch confirmation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaneCommit {
    /// `skl_plane_update_noarm()`: stride, pos, size, then the watermark block. Takes effect only at the arm write.
    pub noarm: Plan,
    /// `skl_plane_update_arm()`: keys, offset, aux, `PLANE_CTL`, `PLANE_SURF` last, then a posting read of `PLANE_SURF`.
    pub arm: Plan,
    /// Wait for the latch (`PLANE_SURFLIVE`).
    pub latch: PollPlan,
}

/// Build the update of one plane from its encoded registers and watermark block.
pub fn plane_commit(regs: &PlaneRegs, wm: &RegWrites<{ NUM_LEVELS + 3 }>) -> PlaneCommit {
    let mut noarm = writes_plan(&regs.noarm_writes());
    noarm.extend(writes_plan(wm));
    let mut arm = writes_plan(&regs.arm_writes());
    arm.push(Effect::PostingRead { reg: Reg::new(regs.surf_reg()) });
    PlaneCommit { noarm, arm, latch: latch_poll(regs.pipe, regs.plane_id, regs.surf) }
}

/// `skl_plane_disable_arm()`: the watermark block first, then `PLANE_CTL = 0` and `PLANE_SURF = 0`.
pub fn plane_disable_plan(pipe: Pipe, plane_id: u8, wm: &RegWrites<{ NUM_LEVELS + 3 }>) -> Plan {
    let mut p = writes_plan(wm);
    p.extend(writes_plan(&i915_kbl_display_core::skl_plane::disable_writes(pipe, plane_id)));
    p
}

/// The cursor block of a pipe whose cursor is not visible: `skl_write_cursor_wm()` (`CUR_WM(level)` for every level from the
/// cursor's own - all-zero - watermarks, `CUR_WM_TRANS`, `CUR_BUF_CFG` = the cursor DDB reserve) followed by the no-state arm path of
/// `i9xx_cursor_update_arm()` (`CURPOS = 0`, `CURBASE = 0`). The reserve is written even though nothing is shown: it is what keeps the
/// other planes out of the cursor's blocks.
pub fn cursor_hidden_plan(pipe: Pipe, pm: &PipeWm) -> Plan {
    let zero = WmLevel::default();
    let mut p = Plan::new();
    for level in 0..NUM_LEVELS {
        p.push(Effect::Write { reg: Reg::new(cur_wm(pipe, level as u32)), value: cursor_wm_reg_val(&zero) });
    }
    p.push(Effect::Write { reg: Reg::new(cur_wm_trans(pipe)), value: cursor_wm_reg_val(&zero) });
    p.push(Effect::Write { reg: Reg::new(cur_buf_cfg(pipe)), value: cursor_ddb_reg_val(&pm.cursor_ddb) });
    p.push(Effect::Write { reg: Reg::new(curpos(pipe)), value: 0 });
    p.push(Effect::Write { reg: Reg::new(curbase(pipe)), value: 0 });
    p
}

/// The undo of a set of plans: every register they write restored to its pre-transaction value (`snap`), in reverse order of
/// first write. Plane enable has no other inverse worth planning: a previously-disabled plane's registers snapshot to zero CTL / SURF.
pub fn restore_all(plans: &[&Plan], snap: &dyn Fn(u32) -> u32) -> Plan {
    let mut all = vec![];
    for p in plans {
        all.extend_from_slice(p);
    }
    restore_plan(&all, snap)
}
