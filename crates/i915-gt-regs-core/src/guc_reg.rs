// SPDX-License-Identifier: GPL-2.0-only
//! The GuC-side register the memory code names: drivers/gpu/drm/i915/gt/uc/intel_guc_reg.h at v7.0-rc6.
//! Copyright (c) Intel Corporation and the i915 authors.

use i915_common_core::reg::bit;
use i915_common_core::Reg;

/// GuC-side GGTT TLB invalidate (carried for the GuC cut-line; Gen9 `intel_uc_wants_guc_submission` is false here).
pub const GEN8_GTCR: Reg = Reg::new(0x4274); // uc/intel_guc_reg.h:80
pub const GEN8_GTCR_INVALIDATE: u32 = bit(0); // uc/intel_guc_reg.h:81
