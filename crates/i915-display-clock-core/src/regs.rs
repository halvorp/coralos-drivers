// SPDX-License-Identifier: GPL-2.0-only
//! No register is DEFINED in this crate. Display registers live in `i915-kbl-display-core::regs` (intel_display_regs.h), the PCU
//! mailbox words in `i915-gt-regs-core` (i915_reg.h); both are re-exported here so the planners read `crate::regs::*`.

pub use i915_gt_regs_core::{
    GEN11_PCODE_ILLEGAL_SUBCOMMAND, GEN11_PCODE_LOCKED, GEN11_PCODE_REJECTED, GEN6_PCODE_DATA, GEN6_PCODE_DATA1,
    GEN6_PCODE_ERROR_MASK, GEN6_PCODE_ILLEGAL_CMD, GEN6_PCODE_MAILBOX, GEN6_PCODE_READY, GEN6_PCODE_SUCCESS,
    GEN7_PCODE_ILLEGAL_DATA, GEN7_PCODE_MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE, GEN7_PCODE_TIMEOUT, SKL_CDCLK_PREPARE_FOR_CHANGE,
    SKL_CDCLK_READY_FOR_CHANGE, SKL_PCODE_CDCLK_CONTROL,
};
pub use i915_kbl_display_core::regs::*;
