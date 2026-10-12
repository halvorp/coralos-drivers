// SPDX-License-Identifier: GPL-2.0-only
//! Slice / subslice / EU topology (gt/intel_sseu.c). The fuse DECODE itself (`gen9_sseu_info_init`, intel_sseu.c:377-480) lives
//! in i915-devinfo-core, which the proof-of-life already uses - one home, re-exported here so a GT consumer imports it from the
//! GT crate. What this module adds is what the GT code does WITH the topology: pick the (slice, subslice) pair every MCR
//! register read is steered to (`gen9_wa_init_mcr`, intel_workarounds.c:1140-1176), and `intel_sseu_make_rpcs` (intel_sseu.c:654-760) for Gen9 - the render
//! context's `R_PWR_CLK_STATE` dword. The Gen11 slice-doubling rule and the i915/perf pinned configuration are not carried.
pub use i915_devinfo_core::sseu::{gen9_sseu_info, SseuInfo, MAX_SLICES, MAX_SUBSLICES};

use crate::error::GtError;
use i915_gt_regs_core::{
    gen8_mcr_slice, gen8_mcr_subslice, GEN8_MCR_SLICE_MASK, GEN8_MCR_SUBSLICE_MASK, GEN8_RPCS_ENABLE, GEN8_RPCS_EU_MAX_MASK,
    GEN8_RPCS_EU_MAX_SHIFT, GEN8_RPCS_EU_MIN_MASK, GEN8_RPCS_EU_MIN_SHIFT, GEN8_RPCS_SS_CNT_ENABLE, GEN8_RPCS_SS_CNT_MASK,
    GEN8_RPCS_SS_CNT_SHIFT, GEN8_RPCS_S_CNT_ENABLE, GEN8_RPCS_S_CNT_MASK, GEN8_RPCS_S_CNT_SHIFT,
};

/// The MCR steering the GT workaround list programs into `GEN8_MCR_SELECTOR`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct McrSteer {
    pub slice: u32,
    pub subslice: u32,
    /// `mcr = GEN8_MCR_SLICE(slice) | GEN8_MCR_SUBSLICE(subslice)` (intel_workarounds.c:1169).
    pub value: u32,
    /// `mcr_mask = GEN8_MCR_SLICE_MASK | GEN8_MCR_SUBSLICE_MASK` (intel_workarounds.c:1170).
    pub mask: u32,
}

/// `ffs(x) - 1` for a non-zero mask: the lowest set bit.
const fn lowest_bit(x: u32) -> Option<u32> {
    if x == 0 {
        None
    } else {
        Some(x.trailing_zeros())
    }
}

/// `gen9_wa_init_mcr` (intel_workarounds.c:1140-1176): the first enabled slice, and that slice's first enabled subslice
/// (`intel_sseu_get_hsw_subslices`, intel_sseu.c:39-45 = `subslice_mask.hsw[slice]`).
pub fn gen9_mcr_steer(sseu: &SseuInfo) -> Result<McrSteer, GtError> {
    let slice = lowest_bit(sseu.slice_mask as u32).ok_or(GtError::NoEnabledSlice)?;
    if slice as usize >= MAX_SLICES {
        return Err(GtError::NoEnabledSlice);
    }
    let subslice = lowest_bit(sseu.subslice_mask[slice as usize] as u32).ok_or(GtError::NoEnabledSubslice { slice })?;
    Ok(McrSteer {
        slice,
        subslice,
        value: gen8_mcr_slice(slice) | gen8_mcr_subslice(subslice),
        mask: GEN8_MCR_SLICE_MASK | GEN8_MCR_SUBSLICE_MASK,
    })
}

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
pub fn make_rpcs_gen9(sseu: &SseuInfo, req: &SseuRequest) -> Result<u32, GtError> {
    let slices = req.slice_mask.count_ones();
    let subslices = req.subslice_mask.count_ones();
    let mut rpcs = 0u32;
    if sseu.has_slice_pg {
        // intel_sseu.c:720-735 (Gen8/Gen9 field layout)
        let val = slices << GEN8_RPCS_S_CNT_SHIFT;
        if val & !GEN8_RPCS_S_CNT_MASK != 0 {
            return Err(GtError::RpcsFieldOverflow { field: "slice count", value: slices });
        }
        rpcs |= GEN8_RPCS_ENABLE | GEN8_RPCS_S_CNT_ENABLE | val;
    }
    if sseu.has_subslice_pg {
        let val = subslices << GEN8_RPCS_SS_CNT_SHIFT; // intel_sseu.c:737-745
        if val & !GEN8_RPCS_SS_CNT_MASK != 0 {
            return Err(GtError::RpcsFieldOverflow { field: "subslice count", value: subslices });
        }
        rpcs |= GEN8_RPCS_ENABLE | GEN8_RPCS_SS_CNT_ENABLE | val;
    }
    if sseu.has_eu_pg {
        let min = (req.min_eus_per_subslice as u32) << GEN8_RPCS_EU_MIN_SHIFT; // intel_sseu.c:747-761
        let max = (req.max_eus_per_subslice as u32) << GEN8_RPCS_EU_MAX_SHIFT;
        if min & !GEN8_RPCS_EU_MIN_MASK != 0 || max & !GEN8_RPCS_EU_MAX_MASK != 0 {
            return Err(GtError::RpcsFieldOverflow { field: "eu min/max", value: req.max_eus_per_subslice.max(req.min_eus_per_subslice) as u32 });
        }
        rpcs |= min | max | GEN8_RPCS_ENABLE;
    }
    Ok(rpcs)
}
