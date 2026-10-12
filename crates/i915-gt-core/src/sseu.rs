// SPDX-License-Identifier: GPL-2.0-only
//! Slice / subslice / EU topology (gt/intel_sseu.c). The fuse DECODE itself (`gen9_sseu_info_init`, intel_sseu.c:377-480) lives
//! in i915-devinfo-core, which the proof-of-life already uses - one home, re-exported here so a GT consumer imports it from the
//! GT crate. What this module adds is what the GT code does WITH the topology: pick the (slice, subslice) pair every MCR
//! register read is steered to (`gen9_wa_init_mcr`, intel_workarounds.c:1140-1176).
pub use i915_devinfo_core::sseu::{gen9_sseu_info, SseuInfo, MAX_SLICES, MAX_SUBSLICES};

use crate::error::GtError;
use i915_gt_regs_core::{gen8_mcr_slice, gen8_mcr_subslice, GEN8_MCR_SLICE_MASK, GEN8_MCR_SUBSLICE_MASK};

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
