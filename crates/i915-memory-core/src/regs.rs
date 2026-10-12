// SPDX-License-Identifier: GPL-2.0-only
//! Memory-layout facts of the Gen8+ GTT that are NOT registers: BAR sizes and the fence count. Every register and PCI config-space
//! word the memory planners name lives in `i915-gt-regs-core` (one home per register; it is re-exported here so the planners
//! read `crate::regs::*`), named for the Linux header that defines it.

pub use i915_gt_regs_core::{
    fence_hi, fence_lo, BDW_GMCH_GGMS_MASK, BDW_GMCH_GGMS_SHIFT, BDW_GMCH_GMS_MASK, BDW_GMCH_GMS_SHIFT, FENCE_REG_GEN6_BASE,
    FENCE_REG_STRIDE, GEN4_GMADR_BAR, GEN4_GTTMMADR_BAR, GEN6_FENCE_PITCH_SHIFT, GEN6_STOLEN_RESERVED,
    GEN6_STOLEN_RESERVED_ADDR_MASK, GEN6_STOLEN_RESERVED_ENABLE, GEN7_FENCE_MAX_PITCH_VAL, GEN8_GTCR, GEN8_GTCR_INVALIDATE,
    GEN8_PRIVATE_PAT_HI, GEN8_PRIVATE_PAT_LO, GFX_FLSH_CNTL_EN, GFX_FLSH_CNTL_GEN6, I965_FENCE_REG_VALID,
    I965_FENCE_TILING_Y_SHIFT, INTEL_BSM, INTEL_BSM_MASK, SNB_GMCH_CTRL,
};

pub const I965_FENCE_PAGE: u64 = 4096; // intel_ggtt_fencing.h:20
/// `intel_ggtt_init_fences` (intel_ggtt_fencing.c:857): Gen7+ non-VLV/CHV has 32.
pub const GEN7_NUM_FENCES: u32 = 32; // intel_ggtt_fencing.c:857
/// Gen8+: GTTMMADR is 16 MiB and the GTT starts halfway (`gen6_gttmmadr_size` / `gen6_gttadr_offset`, intel_ggtt.c:1180-1193).
pub const GEN8_GTTMMADR_SIZE: u64 = 16 << 20; // intel_ggtt.c:1187
pub const GEN8_GTTADR_OFFSET: u32 = 8 << 20; // intel_ggtt.c:1192 (= GTTMMADR_SIZE / 2)
