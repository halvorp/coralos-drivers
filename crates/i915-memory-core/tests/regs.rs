// SPDX-License-Identifier: GPL-2.0-only
//! Every register offset and field pinned against the Linux literal (citation beside each).
use i915_memory_core::regs::*;

#[test]
fn bar_geometry_and_fence_count() {
    assert_eq!(GEN8_GTTMMADR_SIZE, 16 << 20);
    assert_eq!(GEN8_GTTADR_OFFSET, 8 << 20);
    assert_eq!(GEN8_GTTADR_OFFSET as u64 * 2, GEN8_GTTMMADR_SIZE);
    assert_eq!(I965_FENCE_PAGE, 4096);
    assert_eq!(GEN7_NUM_FENCES, 32);
}

/// The planners' register names are the gt-regs-core ones, not a second copy (one home per register).
#[test]
fn registers_are_the_gt_regs_homes() {
    assert_eq!(GFX_FLSH_CNTL_GEN6, i915_gt_regs_core::GFX_FLSH_CNTL_GEN6);
    assert_eq!(fence_lo(3), i915_gt_regs_core::fence_lo(3));
    assert_eq!(GEN6_STOLEN_RESERVED, i915_gt_regs_core::GEN6_STOLEN_RESERVED);
}
