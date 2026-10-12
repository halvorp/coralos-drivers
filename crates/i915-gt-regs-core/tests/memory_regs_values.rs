// SPDX-License-Identifier: GPL-2.0-only
//! Registers moved here from i915-memory-core (sentinel audit 2026-10-12): pinned against the Linux literals.
use i915_gt_regs_core::*;

#[test]
fn registers_have_linux_values() {
    assert_eq!(GFX_FLSH_CNTL_GEN6.offset(), 0x101008); // intel_gt_regs.h:1474
    assert_eq!(GFX_FLSH_CNTL_EN, 1);
    assert_eq!(GEN8_PRIVATE_PAT_LO.offset(), 0x40e0); // intel_gt_regs.h:336
    assert_eq!(GEN8_PRIVATE_PAT_HI.offset(), 0x40e4); // :337
    assert_eq!(GEN8_GTCR.offset(), 0x4274); // uc/intel_guc_reg.h:80
    assert_eq!(GEN8_GTCR_INVALIDATE, 1);
    assert_eq!(GEN6_STOLEN_RESERVED.offset(), 0x1082C0); // i915_reg.h:128
    assert_eq!(GEN6_STOLEN_RESERVED_ADDR_MASK, 0xFFF00000);
    assert_eq!(GEN6_STOLEN_RESERVED_ENABLE, 1);
}

#[test]
fn fence_register_layout() {
    assert_eq!(fence_lo(0).offset(), 0x100000);
    assert_eq!(fence_hi(0).offset(), 0x100004);
    assert_eq!(fence_lo(1).offset(), 0x100008);
    assert_eq!(fence_lo(31).offset(), 0x1000f8);
    assert_eq!(fence_hi(31).offset(), 0x1000fc);
    assert_eq!(GEN6_FENCE_PITCH_SHIFT, 32);
    assert_eq!(I965_FENCE_TILING_Y_SHIFT, 1);
    assert_eq!(I965_FENCE_REG_VALID, 1);
    assert_eq!(GEN7_FENCE_MAX_PITCH_VAL, 0x800);
    assert_eq!((FENCE_REG_GEN6_BASE, FENCE_REG_STRIDE), (0x100000, 8));
}
#[test]
fn config_space_words() {
    assert_eq!(SNB_GMCH_CTRL, 0x50); // include/drm/intel/i915_drm.h:49
    assert_eq!(BDW_GMCH_GGMS_SHIFT, 6);
    assert_eq!(BDW_GMCH_GGMS_MASK, 0x3);
    assert_eq!(BDW_GMCH_GMS_SHIFT, 8);
    assert_eq!(BDW_GMCH_GMS_MASK, 0xff);
    assert_eq!(INTEL_BSM, 0x5c);
    assert_eq!(INTEL_BSM_MASK, 0xfff0_0000);
    assert_eq!((GEN4_GTTMMADR_BAR, GEN4_GMADR_BAR), (0, 2));
}
