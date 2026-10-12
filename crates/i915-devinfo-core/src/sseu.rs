// SPDX-License-Identifier: GPL-2.0-only
//! Gen9 slice / subslice / EU topology from the fuse registers: a port of `gen9_sseu_info_init` (gt/intel_sseu.c:377-480) for
//! the non-LP (Skylake, Kaby Lake) case, over register VALUES the caller read. The Gen9 LP (BXT/GLK) pooled-EU branch is
//! not ported: no LP row is carried.
use i915_common_core::reg::field_get;
use i915_gt_regs_core::{GEN8_F2_S_ENA_MASK, GEN9_F2_SS_DIS_MASK};

/// `GEN_MAX_SLICES` for the hsw-layout mask arrays (3 on Gen9: `intel_sseu_set_info(sseu, 3, 4, 8)`, intel_sseu.c:390-391).
pub const MAX_SLICES: usize = 3;
pub const MAX_SUBSLICES: usize = 4;

/// The result, with Linux's field names (`struct sseu_dev_info`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SseuInfo {
    pub slice_mask: u8,
    pub max_slices: u8,
    pub max_subslices: u8,
    pub max_eus_per_subslice: u8,
    /// `subslice_mask.hsw[s]`.
    pub subslice_mask: [u8; MAX_SLICES],
    /// `eu_mask.hsw[s][ss]`.
    pub eu_mask: [[u8; MAX_SUBSLICES]; MAX_SLICES],
    /// `subslice_7eu[s]`: subslices with exactly 7 EUs (die recovery).
    pub subslice_7eu: [u8; MAX_SLICES],
    pub eu_total: u16,
    pub eu_per_subslice: u8,
    pub has_slice_pg: bool,
    pub has_subslice_pg: bool,
    pub has_eu_pg: bool,
}

impl SseuInfo {
    /// `hweight8(slice_mask)`.
    pub fn slice_total(&self) -> u32 {
        self.slice_mask.count_ones()
    }
    /// `intel_sseu_subslice_total` (intel_sseu.c:24-36): the sum of `hweight8` over every `subslice_mask.hsw[]`.
    pub fn subslice_total(&self) -> u32 {
        self.subslice_mask.iter().map(|m| m.count_ones()).sum()
    }
}

/// `gen9_sseu_info_init` for a non-LP Gen9 part: `fuse2` = `GEN8_FUSE2` (0x9120), `eu_disable[s]` = `GEN9_EU_DISABLE(s)`
/// (0x9134 + 4 s). `eu_disable[s]` is consulted only for an ENABLED slice, as in Linux (intel_sseu.c:404-411).
pub fn gen9_sseu_info(fuse2: u32, eu_disable: [u32; MAX_SLICES]) -> SseuInfo {
    const EU_MASK: u32 = 0xff; // intel_sseu.c:383
    let slice_mask = field_get(GEN8_F2_S_ENA_MASK, fuse2) as u8; // intel_sseu.c:387
    // intel_sseu_set_info(sseu, IS_GEN9_LP ? 1 : 3, IS_GEN9_LP ? 3 : 4, 8) (intel_sseu.c:390-391)
    let (max_slices, max_subslices, max_eus) = (3u8, 4u8, 8u8);
    // The subslice disable field is global: it applies to each enabled slice (intel_sseu.c:397-398).
    let mut ss_mask = (1u32 << max_subslices) - 1;
    ss_mask &= !field_get(GEN9_F2_SS_DIS_MASK, fuse2);

    let mut info = SseuInfo {
        slice_mask,
        max_slices,
        max_subslices,
        max_eus_per_subslice: max_eus,
        subslice_mask: [0; MAX_SLICES],
        eu_mask: [[0; MAX_SUBSLICES]; MAX_SLICES],
        subslice_7eu: [0; MAX_SLICES],
        eu_total: 0,
        eu_per_subslice: 0,
        has_slice_pg: false,
        has_subslice_pg: false,
        has_eu_pg: false,
    };
    let mut s = 0;
    while s < max_slices as usize {
        if slice_mask & (1 << s) == 0 {
            s += 1; // skip disabled slice (intel_sseu.c:405-407)
            continue;
        }
        info.subslice_mask[s] = ss_mask as u8; // :409
        let dis = eu_disable[s]; // :411
        let mut ss = 0;
        while ss < max_subslices as usize {
            // intel_sseu_has_subslice (intel_sseu.h:122-130): bounded by max_*, then tested in the hsw mask
            if info.subslice_mask[s] & (1 << ss) != 0 {
                let dis_mask = ((dis >> (ss * 8)) & EU_MASK) as u8; // :420
                info.eu_mask[s][ss] = !dis_mask & EU_MASK as u8; // :422 sseu_set_eus
                let eu_per_ss = max_eus - dis_mask.count_ones() as u8; // :424-425
                if eu_per_ss == 7 {
                    info.subslice_7eu[s] |= 1 << ss; // :432-433
                }
            }
            ss += 1;
        }
        s += 1;
    }
    // compute_eu_total (intel_sseu.c:72-84): hweight over every (s, ss) mask
    let mut total = 0u16;
    for row in info.eu_mask.iter() {
        for m in row.iter() {
            total += m.count_ones() as u16;
        }
    }
    info.eu_total = total; // :437
    let sst = info.subslice_total();
    // DIV_ROUND_UP(eu_total, subslice_total) (intel_sseu.c:446-449)
    info.eu_per_subslice = if sst != 0 { total.div_ceil(sst as u16) as u8 } else { 0 };
    // not LP: slice power gating when more than one slice; EU power gating with more than one EU pair (:459-463)
    info.has_slice_pg = slice_mask.count_ones() > 1;
    info.has_subslice_pg = false;
    info.has_eu_pg = info.eu_per_subslice > 2;
    info
}
