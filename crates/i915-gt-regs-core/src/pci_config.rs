// SPDX-License-Identifier: GPL-2.0-only
//! PCI config-space words and BAR indices the i915 memory code reads: include/drm/intel/i915_drm.h and
//! drivers/gpu/drm/i915/intel_pci_config.h at v7.0-rc6. These are config-space offsets, not MMIO registers, so they are plain integers.
//! Copyright (c) Intel Corporation and the i915 authors.

use i915_common_core::reg::genmask;

/// PCI config-space words of the host bridge function (device 2, function 0 = the GPU itself for these two).
pub const SNB_GMCH_CTRL: u16 = 0x50; // include/drm/intel/i915_drm.h:49
pub const BDW_GMCH_GGMS_SHIFT: u32 = 6; // include/drm/intel/i915_drm.h:54
pub const BDW_GMCH_GGMS_MASK: u16 = 0x3; // include/drm/intel/i915_drm.h:55
pub const BDW_GMCH_GMS_SHIFT: u32 = 8; // include/drm/intel/i915_drm.h:56
pub const BDW_GMCH_GMS_MASK: u16 = 0xff; // include/drm/intel/i915_drm.h:57
pub const INTEL_BSM: u16 = 0x5c; // include/drm/intel/i915_drm.h:97
pub const INTEL_BSM_MASK: u32 = genmask(31, 20); // include/drm/intel/i915_drm.h:100 (-(1u << 20))

/// BAR indices: `GEN4_GTTMMADR_BAR` = 0 (MMIO + GTT), `GEN4_GMADR_BAR` = 2 (the mappable aperture).
pub const GEN4_GTTMMADR_BAR: u8 = 0; // intel_pci_config.h:19
pub const GEN4_GMADR_BAR: u8 = 2; // intel_pci_config.h:20
