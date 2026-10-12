// SPDX-License-Identifier: GPL-2.0-only
//! `intel_sseu_make_rpcs` (gt/intel_sseu.c:654-760) for Gen9: the value of the render ring's `R_PWR_CLK_STATE` dword in the
//! context image (`CTX_R_PWR_CLK_STATE`). It belongs next to the rest of intel_sseu.c in `i915_gt_core::sseu`; it is here
//! until that crate takes it (this wave did not edit gt-core). The Gen11 slice-doubling rule and the i915/perf pinned
//! configuration are not carried.
use crate::error::SubmitError;
use i915_devinfo_core::sseu::SseuInfo;
use i915_gt_regs_core::{
    GEN8_RPCS_ENABLE, GEN8_RPCS_EU_MAX_MASK, GEN8_RPCS_EU_MAX_SHIFT, GEN8_RPCS_EU_MIN_MASK, GEN8_RPCS_EU_MIN_SHIFT, GEN8_RPCS_SS_CNT_ENABLE,
    GEN8_RPCS_SS_CNT_MASK, GEN8_RPCS_SS_CNT_SHIFT, GEN8_RPCS_S_CNT_ENABLE, GEN8_RPCS_S_CNT_MASK, GEN8_RPCS_S_CNT_SHIFT,
};

/// `struct intel_sseu` (intel_sseu.h:101-106): what a context asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SseuRequest {
    pub slice_mask: u8,
    pub subslice_mask: u8,
    pub min_eus_per_subslice: u8,
    pub max_eus_per_subslice: u8,
}

/// `intel_sseu_from_device_info` (intel_sseu.h:108-119): the whole device. Note Linux sets BOTH eu bounds to
/// `max_eus_per_subslice`.
pub fn sseu_from_device_info(sseu: &SseuInfo) -> SseuRequest {
    SseuRequest { slice_mask: sseu.slice_mask, subslice_mask: sseu.subslice_mask[0], min_eus_per_subslice: sseu.max_eus_per_subslice, max_eus_per_subslice: sseu.max_eus_per_subslice }
}

/// `intel_sseu_make_rpcs` for graphics version 9.
pub fn make_rpcs_gen9(sseu: &SseuInfo, req: &SseuRequest) -> Result<u32, SubmitError> {
    let slices = req.slice_mask.count_ones();
    let subslices = req.subslice_mask.count_ones();
    let mut rpcs = 0u32;
    if sseu.has_slice_pg {
        // intel_sseu.c:720-735 (Gen8/Gen9 field layout)
        let val = slices << GEN8_RPCS_S_CNT_SHIFT;
        if val & !GEN8_RPCS_S_CNT_MASK != 0 {
            return Err(SubmitError::RpcsFieldOverflow { field: "slice count", value: slices });
        }
        rpcs |= GEN8_RPCS_ENABLE | GEN8_RPCS_S_CNT_ENABLE | val;
    }
    if sseu.has_subslice_pg {
        let val = subslices << GEN8_RPCS_SS_CNT_SHIFT; // intel_sseu.c:737-745
        if val & !GEN8_RPCS_SS_CNT_MASK != 0 {
            return Err(SubmitError::RpcsFieldOverflow { field: "subslice count", value: subslices });
        }
        rpcs |= GEN8_RPCS_ENABLE | GEN8_RPCS_SS_CNT_ENABLE | val;
    }
    if sseu.has_eu_pg {
        let min = (req.min_eus_per_subslice as u32) << GEN8_RPCS_EU_MIN_SHIFT; // intel_sseu.c:747-761
        let max = (req.max_eus_per_subslice as u32) << GEN8_RPCS_EU_MAX_SHIFT;
        if min & !GEN8_RPCS_EU_MIN_MASK != 0 || max & !GEN8_RPCS_EU_MAX_MASK != 0 {
            return Err(SubmitError::RpcsFieldOverflow { field: "eu min/max", value: req.max_eus_per_subslice.max(req.min_eus_per_subslice) as u32 });
        }
        rpcs |= min | max | GEN8_RPCS_ENABLE;
    }
    Ok(rpcs)
}
