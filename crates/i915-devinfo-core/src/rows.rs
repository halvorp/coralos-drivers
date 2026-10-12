// SPDX-License-Identifier: GPL-2.0-only
//! The `struct intel_device_info` rows (i915_pci.c), built the way Linux builds them: GEN8_FEATURES (:412-419, over G75_FEATURES
//! :386-390, over GEN7_FEATURES :311-327), then GEN9_FEATURES (:475-479), then the platform and `.gt`.
use crate::engine::{BCS0, RCS0, VCS0, VCS1, VECS0};
use crate::{DeviceInfo, Platform, PpgttType, GTT_PAGE_SIZE_4K, GTT_PAGE_SIZE_64K};

const fn bit(n: u32) -> u32 {
    1 << n
}

/// `GEN7_FEATURES` as inherited by `G75_FEATURES` and `GEN8_FEATURES`, with each override applied (i915_pci.c:311-327, :386-390,
/// :412-419): engines RCS0|VCS0|BCS0|VECS0 (:388), `has_llc` (:316), `has_coherent_ggtt` (:315), `has_rc6` (:317),
/// `has_reset_engine` (:319), `has_rps` (:320), `has_runtime_pm` (:390), `has_logical_ring_contexts` (:415),
/// `dma_mask_size` 39 (:416), full PPGTT of 48 bits (:417-418), `has_64bit_reloc` (:419), `max_pat_index` 3 (:322).
const GEN8_BASE: DeviceInfo = DeviceInfo {
    platform: Platform::Skylake, // overwritten by every row
    graphics_ver: 8,
    display_ver: 8,
    gt: 0,
    engine_mask: bit(RCS0) | bit(VCS0) | bit(BCS0) | bit(VECS0),
    ppgtt_type: PpgttType::Full,
    ppgtt_size: 48,
    dma_mask_size: 39,
    page_sizes: GTT_PAGE_SIZE_4K,
    max_pat_index: 3,
    has_llc: true,
    has_snoop: false,
    has_coherent_ggtt: true,
    has_logical_ring_contexts: true,
    has_64bit_reloc: true,
    has_reset_engine: true,
    has_rc6: true,
    has_rps: true,
    has_runtime_pm: true,
    has_gt_uc: false,
};

/// `GEN9_FEATURES` (i915_pci.c:475-479): `GEN(9)`, 4K|64K pages (:471-473), `has_gt_uc`; display version 9 (`skl_display`,
/// intel_display_device.c:675).
const GEN9_BASE: DeviceInfo = DeviceInfo {
    graphics_ver: 9,
    display_ver: 9,
    page_sizes: GTT_PAGE_SIZE_4K | GTT_PAGE_SIZE_64K,
    has_gt_uc: true,
    ..GEN8_BASE
};

/// `SKL_GT3_PLUS_PLATFORM`'s engine mask adds the second VDBOX (i915_pci.c:495-498); `kbl_gt3_info` does the same (:556-561).
const GT3_ENGINES: u32 = bit(RCS0) | bit(VCS0) | bit(BCS0) | bit(VECS0) | bit(VCS1);

/// `chv_info` (i915_pci.c:450-469): Cherryview, Gen8 - the NEGATIVE CONTROL row. Written out field by field as Linux does (it
/// does not use GEN8_FEATURES): no LLC, snooping, non-coherent GGTT, a 32-bit PPGTT, display version 8 (intel_display_device.c:653).
pub static CHV: DeviceInfo = DeviceInfo {
    platform: Platform::Cherryview,
    graphics_ver: 8,
    display_ver: 8,
    gt: 0,
    engine_mask: bit(RCS0) | bit(VCS0) | bit(BCS0) | bit(VECS0), // :453
    ppgtt_type: PpgttType::Full,                                   // :461
    ppgtt_size: 32,                                                // :462
    dma_mask_size: 39,                                             // :459
    page_sizes: GTT_PAGE_SIZE_4K,                                  // :466
    max_pat_index: 3,                                              // :460
    has_llc: false,
    has_snoop: true,              // :464
    has_coherent_ggtt: false,     // :465
    has_logical_ring_contexts: true, // :458
    has_64bit_reloc: true,        // :454
    has_reset_engine: true,       // :463
    has_rc6: true,                // :456
    has_rps: true,                // :457
    has_runtime_pm: true,         // :455
    has_gt_uc: false,
};

pub static SKL_GT1: DeviceInfo = DeviceInfo { platform: Platform::Skylake, gt: 1, ..GEN9_BASE }; // i915_pci.c:485-488
pub static SKL_GT2: DeviceInfo = DeviceInfo { platform: Platform::Skylake, gt: 2, ..GEN9_BASE }; // i915_pci.c:490-493
pub static SKL_GT3: DeviceInfo = DeviceInfo { platform: Platform::Skylake, gt: 3, engine_mask: GT3_ENGINES, ..GEN9_BASE }; // i915_pci.c:501-504
pub static SKL_GT4: DeviceInfo = DeviceInfo { platform: Platform::Skylake, gt: 4, engine_mask: GT3_ENGINES, ..GEN9_BASE }; // i915_pci.c:506-509
pub static KBL_GT1: DeviceInfo = DeviceInfo { platform: Platform::Kabylake, gt: 1, ..GEN9_BASE }; // i915_pci.c:546-549
pub static KBL_GT2: DeviceInfo = DeviceInfo { platform: Platform::Kabylake, gt: 2, ..GEN9_BASE }; // i915_pci.c:551-554
/// Also the row for the GT4 id 0x593b (`INTEL_KBL_GT4_IDS(..., &kbl_gt3_info)`, i915_pci.c:839).
pub static KBL_GT3: DeviceInfo = DeviceInfo { platform: Platform::Kabylake, gt: 3, engine_mask: GT3_ENGINES, ..GEN9_BASE }; // i915_pci.c:556-561
