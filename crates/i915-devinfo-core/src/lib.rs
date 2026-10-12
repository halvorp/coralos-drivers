// SPDX-License-Identifier: GPL-2.0-only
//! i915-devinfo-core: PCI device id -> the ONE [`PlatformDescriptor`] the rest of the i915 port is written against.
//!
//! Ported from Linux drivers/gpu/drm/i915/i915_pci.c (`GEN8_FEATURES` / `GEN9_FEATURES` and the `chv_info`, `skl_gt1..4_info`,
//! `kbl_gt1..3_info` rows, i915_pci.c:412-562), the id lists in include/drm/intel/pciids.h, and the display version / sub-platform
//! lists of display/intel_display_device.c, at v7.0-rc6. Copyright (c) Intel Corporation and the i915 authors.
//!
//! WHAT IS CARRIED: Skylake and Kaby Lake (Gen9, the target) and Cherryview (Gen8) - the last ONLY as the negative control that
//! proves a Gen8 part is recognised BY NAME and refused ([`PlatformDescriptor::require_gen9`]), never aliased to a Gen9 row.
//! Every other id, including ids Linux binds (Broadwell, Coffee Lake, ...), is `I915Error::UnknownDevice { devid }` - the port
//! does not carry those rows, and says so rather than guessing a neighbour (Astra's ruling, 2026-10-12).
//!
//! The crate also hosts the two pure decoders the device needs for its proof of life: [`sseu`] (gt/intel_sseu.c, Gen9 slice /
//! subslice / EU topology from the fuse registers) and [`clock`] (gt/intel_gt_clock_utils.c, the Gen9 command-stream timestamp
//! frequency). Both take register VALUES and return values: the crate performs no MMIO.

#![no_std]
#![forbid(unsafe_code)]

pub mod clock;
mod ids;
mod rows;
pub mod sseu;

pub use i915_common_core::I915Error;
pub use ids::{lookup, ID_TABLE};
pub use rows::{CHV, KBL_GT1, KBL_GT2, KBL_GT3, SKL_GT1, SKL_GT2, SKL_GT3, SKL_GT4};

/// `enum intel_platform` (intel_device_info.h:43), restricted to the rows this crate carries. Order of appearance in Linux:
/// CHERRYVIEW (:72), SKYLAKE (:74), KABYLAKE (:76).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Cherryview,
    Skylake,
    Kabylake,
}

/// The display sub-platform groups (`INTEL_SUBPLATFORM_ULT` / `_ULX`, intel_display_device.c:686-695 for SKL, :723-735 for KBL).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubPlatform {
    None,
    Ult,
    Ulx,
}

/// `enum intel_ppgtt_type` (intel_device_info.h:138-142; values I915_GEM_PPGTT_* include/uapi/drm/i915_drm.h:591-593).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PpgttType {
    None = 0,
    Aliasing = 1,
    Full = 2,
}

/// `enum intel_engine_id` bit positions (gt/intel_engine_types.h:111-143): RCS0 = 0, BCS0 = 1 (BCS0..BCS8 = 1..9),
/// VCS0 = 10 (VCS0..VCS7 = 10..17), VECS0 = 18 (VECS0..VECS3 = 18..21).
pub mod engine {
    pub const RCS0: u32 = 0; // intel_engine_types.h:112
    pub const BCS0: u32 = 1; // intel_engine_types.h:113
    pub const VCS0: u32 = 10; // intel_engine_types.h:123
    pub const VCS1: u32 = 11; // intel_engine_types.h:124
    pub const VECS0: u32 = 18; // intel_engine_types.h:132
}

/// `I915_GTT_PAGE_SIZE_4K` / `_64K` (gt/intel_gtt.h:45-46).
pub const GTT_PAGE_SIZE_4K: u32 = 1 << 12; // intel_gtt.h:45
pub const GTT_PAGE_SIZE_64K: u32 = 1 << 16; // intel_gtt.h:46

/// One `struct intel_device_info` row (i915_pci.c) plus the display IP version of its `platform_desc`
/// (display/intel_display_device.c). Rows are `static`s in [`rows`](crate); this is their shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeviceInfo {
    pub platform: Platform,
    /// `__runtime.graphics.ip.ver` (`GEN(x)`, i915_pci.c:44-46).
    pub graphics_ver: u8,
    /// `__runtime_defaults.ip.ver` of the display (intel_display_device.c:653 CHV, :675 `skl_display`).
    pub display_ver: u8,
    /// `.gt` (0 = undefined).
    pub gt: u8,
    /// `.platform_engine_mask` (bit = `enum intel_engine_id`).
    pub engine_mask: u32,
    pub ppgtt_type: PpgttType,
    /// log2 of the PPGTT address space (`__runtime.ppgtt_size`).
    pub ppgtt_size: u8,
    pub dma_mask_size: u8,
    pub page_sizes: u32,
    pub max_pat_index: u8,
    pub has_llc: bool,
    pub has_snoop: bool,
    pub has_coherent_ggtt: bool,
    pub has_logical_ring_contexts: bool,
    pub has_64bit_reloc: bool,
    pub has_reset_engine: bool,
    pub has_rc6: bool,
    pub has_rps: bool,
    pub has_runtime_pm: bool,
    pub has_gt_uc: bool,
}

/// The descriptor for one PCI device id: the row it selects plus its identity. `Copy`, no allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformDescriptor {
    pub devid: u16,
    pub sub: SubPlatform,
    pub info: DeviceInfo,
}

impl PlatformDescriptor {
    pub const fn platform(&self) -> Platform {
        self.info.platform
    }
    pub const fn gen(&self) -> u8 {
        self.info.graphics_ver
    }
    pub const fn gt(&self) -> u8 {
        self.info.gt
    }
    /// `GEN9_LP`: Broxton / Geminilake. None is carried, so always false; kept so callers do not hard-code it.
    pub const fn is_gen9_lp(&self) -> bool {
        false
    }
    /// A short name for markers: `kbl-gt2`, `skl-gt3`, `chv`.
    pub const fn name(&self) -> &'static str {
        match (self.info.platform, self.info.gt) {
            (Platform::Cherryview, _) => "chv",
            (Platform::Skylake, 1) => "skl-gt1",
            (Platform::Skylake, 2) => "skl-gt2",
            (Platform::Skylake, 3) => "skl-gt3",
            (Platform::Skylake, 4) => "skl-gt4",
            (Platform::Kabylake, 1) => "kbl-gt1",
            (Platform::Kabylake, 2) => "kbl-gt2",
            (Platform::Kabylake, 3) => "kbl-gt3",
            _ => "unknown",
        }
    }
    /// The Gen9 gate: the i915 device drives Gen9 only; a Gen8 (Cherryview) id is a KNOWN row that is refused by generation.
    pub fn require_gen9(&self) -> Result<(), I915Error> {
        if self.info.graphics_ver == 9 {
            Ok(())
        } else {
            Err(I915Error::UnsupportedGeneration { devid: self.devid, gen: self.info.graphics_ver })
        }
    }
}
