// SPDX-License-Identifier: GPL-2.0-only
//! The i915_reg.h registers the GT code reads or writes: ring bases, the Gen8 GT interrupt block, and the two chicken registers the Gen9 clock-gating code touches.
//! Ported from Linux drivers/gpu/drm/i915/i915_reg.h at v7.0-rc6; every line carries `file:line`.
//! Copyright (c) Intel Corporation and the i915 authors.

use i915_common_core::reg::{bit};
use i915_common_core::Reg;

pub const RENDER_RING_BASE: u32 = 0x02000; // i915_reg.h:243
pub const GEN6_BSD_RING_BASE: u32 = 0x12000; // i915_reg.h:245
pub const GEN8_BSD2_RING_BASE: u32 = 0x1c000; // i915_reg.h:246
pub const BSD_RING_BASE: u32 = 0x04000; // i915_reg.h:244
pub const VEBOX_RING_BASE: u32 = 0x1a000; // i915_reg.h:255
pub const BLT_RING_BASE: u32 = 0x22000; // i915_reg.h:265
pub const GEN11_BSD_RING_BASE: u32 = 0x1c0000; // i915_reg.h:247
pub const GEN11_BSD2_RING_BASE: u32 = 0x1c4000; // i915_reg.h:248
pub const GEN11_VEBOX_RING_BASE: u32 = 0x1c8000; // i915_reg.h:256
pub const GT_WAIT_SEMAPHORE_INTERRUPT: u32 = bit(11); // i915_reg.h:508
pub const GT_CONTEXT_SWITCH_INTERRUPT: u32 = 1 << 8; // i915_reg.h:509
pub const GT_RENDER_PIPECTL_NOTIFY_INTERRUPT: u32 = 1 << 4; // i915_reg.h:511
pub const GT_CS_MASTER_ERROR_INTERRUPT: u32 = bit(3); // i915_reg.h:512
pub const GT_RENDER_SYNC_STATUS_INTERRUPT: u32 = 1 << 2; // i915_reg.h:513
pub const GT_RENDER_DEBUG_INTERRUPT: u32 = 1 << 1; // i915_reg.h:514
pub const GT_RENDER_USER_INTERRUPT: u32 = 1 << 0; // i915_reg.h:515
pub const GEN8_MASTER_IRQ: Reg = Reg::new(0x44200); // i915_reg.h:861
pub const GEN8_MASTER_IRQ_CONTROL: u32 = 1 << 31; // i915_reg.h:862
pub const GEN8_GT_VECS_IRQ: u32 = 1 << 6; // i915_reg.h:871
pub const GEN8_GT_GUC_IRQ: u32 = 1 << 5; // i915_reg.h:872
pub const GEN8_GT_PM_IRQ: u32 = 1 << 4; // i915_reg.h:873
pub const GEN8_GT_VCS1_IRQ: u32 = 1 << 3; // i915_reg.h:874
pub const GEN8_GT_VCS0_IRQ: u32 = 1 << 2; // i915_reg.h:875
pub const GEN8_GT_BCS_IRQ: u32 = 1 << 1; // i915_reg.h:876
pub const GEN8_GT_RCS_IRQ: u32 = 1 << 0; // i915_reg.h:877
pub const GEN8_RCS_IRQ_SHIFT: u32 = 0; // i915_reg.h:888
pub const GEN8_BCS_IRQ_SHIFT: u32 = 16; // i915_reg.h:889
pub const GEN8_VCS0_IRQ_SHIFT: u32 = 0; // i915_reg.h:890
pub const GEN8_VCS1_IRQ_SHIFT: u32 = 16; // i915_reg.h:891
pub const GEN8_VECS_IRQ_SHIFT: u32 = 0; // i915_reg.h:892
pub const CHICKEN_PAR1_1: Reg = Reg::new(0x42080); // i915_reg.h:944
pub const SKL_DE_COMPRESSED_HASH_MODE: u32 = bit(15); // i915_reg.h:948
pub const SKL_EDP_PSR_FIX_RDWRAP: u32 = bit(3); // i915_reg.h:951
pub const DISP_ARB_CTL: Reg = Reg::new(0x45000); // i915_reg.h:980
pub const DISP_FBC_MEMORY_WAKE: u32 = bit(31); // i915_reg.h:981
pub const DISP_FBC_WM_DIS: u32 = bit(15); // i915_reg.h:983
pub const GEN8_CHICKEN_DCPR_1: Reg = Reg::new(0x46430); // i915_reg.h:985
pub const MASK_WAKEMEM: u32 = bit(13); // i915_reg.h:998

/// Every plain register (not the aliases) of this module, by name: checked for alignment and uniqueness by the tests.
pub const I915_REG_CATALOG: [(&str, Reg); 4] = [
    ("GEN8_MASTER_IRQ", GEN8_MASTER_IRQ),
    ("CHICKEN_PAR1_1", CHICKEN_PAR1_1),
    ("DISP_ARB_CTL", DISP_ARB_CTL),
    ("GEN8_CHICKEN_DCPR_1", GEN8_CHICKEN_DCPR_1),
];

// ---- Gen8 GT interrupt register banks (i915_reg.h:879-882) ---------------------------------------------------------------

/// `GEN8_GT_ISR(which)` (i915_reg.h:879).
pub const fn gen8_gt_isr(which: u32) -> Reg {
    Reg::new(0x44300 + 0x10 * which)
}
/// `GEN8_GT_IMR(which)` (i915_reg.h:880).
pub const fn gen8_gt_imr(which: u32) -> Reg {
    Reg::new(0x44304 + 0x10 * which)
}
/// `GEN8_GT_IIR(which)` (i915_reg.h:881).
pub const fn gen8_gt_iir(which: u32) -> Reg {
    Reg::new(0x44308 + 0x10 * which)
}
/// `GEN8_GT_IER(which)` (i915_reg.h:882).
pub const fn gen8_gt_ier(which: u32) -> Reg {
    Reg::new(0x4430c + 0x10 * which)
}
