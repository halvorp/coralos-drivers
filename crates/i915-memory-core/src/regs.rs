// SPDX-License-Identifier: GPL-2.0-only
//! The registers and config-space offsets the memory planners name.
//!
//! HOMES: the contract says a register lives in the crate named for its Linux header. `i915-gt-regs-core` does not yet carry
//! these (it covers forcewake / fuses / timestamps), and this crate may not edit it (one worker per crate), so they are typed
//! HERE with their Linux citations; the next wave should MOVE `GFX_FLSH_CNTL_GEN6`, `GEN8_PRIVATE_PAT_LO/HI` (both
//! intel_gt_regs.h), `GEN8_GTCR` (uc/intel_guc_reg.h) and the `i915_reg.h` fence / stolen-reserved block into gt-regs-core and
//! import them back. Values are pinned by `tests/regs.rs`.
use i915_common_core::reg::{bit, genmask};
use i915_common_core::Reg;

/// GGTT invalidate: writing `GFX_FLSH_CNTL_EN` flushes the write-combining buffer of GGTT PTE stores and the GT TLB.
pub const GFX_FLSH_CNTL_GEN6: Reg = Reg::new(0x101008); // intel_gt_regs.h:1474
pub const GFX_FLSH_CNTL_EN: u32 = bit(0); // intel_gt_regs.h:1475
/// The private PAT (8 entries x 8 bits): low and high dword.
pub const GEN8_PRIVATE_PAT_LO: Reg = Reg::new(0x40e0); // intel_gt_regs.h:336
pub const GEN8_PRIVATE_PAT_HI: Reg = Reg::new(0x40e0 + 4); // intel_gt_regs.h:337
/// GuC-side GGTT TLB invalidate (carried for the GuC cut-line; Gen9 `intel_uc_wants_guc_submission` is false here).
pub const GEN8_GTCR: Reg = Reg::new(0x4274); // uc/intel_guc_reg.h:80
pub const GEN8_GTCR_INVALIDATE: u32 = bit(0); // uc/intel_guc_reg.h:81

/// Fence registers, Gen6+ layout (`i965_write_fence_reg`, intel_ggtt_fencing.c:61).
pub const FENCE_REG_GEN6_BASE: u32 = 0x100000; // i915_reg.h:216
pub const FENCE_REG_STRIDE: u32 = 8; // i915_reg.h:216
pub const GEN6_FENCE_PITCH_SHIFT: u32 = 32; // i915_reg.h:218
pub const I965_FENCE_TILING_Y_SHIFT: u32 = 1; // i915_reg.h:212
pub const I965_FENCE_REG_VALID: u32 = bit(0); // i915_reg.h:213
pub const GEN7_FENCE_MAX_PITCH_VAL: u32 = 0x0800; // i915_reg.h:219
pub const I965_FENCE_PAGE: u64 = 4096; // intel_ggtt_fencing.h:20
/// `intel_ggtt_init_fences` (intel_ggtt_fencing.c:857): Gen7+ non-VLV/CHV has 32.
pub const GEN7_NUM_FENCES: u32 = 32; // intel_ggtt_fencing.c:857

/// `FENCE_REG_GEN6_LO(i)` / `_HI(i)`.
pub const fn fence_lo(i: u32) -> Reg {
    Reg::new(FENCE_REG_GEN6_BASE + i * FENCE_REG_STRIDE) // i915_reg.h:216
}
pub const fn fence_hi(i: u32) -> Reg {
    Reg::new(FENCE_REG_GEN6_BASE + i * FENCE_REG_STRIDE + 4) // i915_reg.h:217
}

/// Stolen-memory reservation register (Gen8/9 use the GEN6 name and mask: `bdw_get_stolen_reserved`, i915_gem_stolen.c:385).
pub const GEN6_STOLEN_RESERVED: Reg = Reg::new(0x1082C0); // i915_reg.h:128
pub const GEN6_STOLEN_RESERVED_ADDR_MASK: u32 = 0xFFF << 20; // i915_reg.h:129
pub const GEN6_STOLEN_RESERVED_ENABLE: u32 = bit(0); // i915_reg.h:144

/// PCI config-space words of the host bridge function (device 2, function 0 = the GPU itself for these two).
pub const SNB_GMCH_CTRL: u16 = 0x50; // include/drm/intel/i915_drm.h:49
pub const BDW_GMCH_GGMS_SHIFT: u32 = 6; // include/drm/intel/i915_drm.h:54
pub const BDW_GMCH_GGMS_MASK: u16 = 0x3; // include/drm/intel/i915_drm.h:55
pub const BDW_GMCH_GMS_SHIFT: u32 = 8; // include/drm/intel/i915_drm.h:56
pub const BDW_GMCH_GMS_MASK: u16 = 0xff; // include/drm/intel/i915_drm.h:57
pub const INTEL_BSM: u16 = 0x5c; // include/drm/intel/i915_drm.h:97
pub const INTEL_BSM_MASK: u32 = genmask(31, 20); // include/drm/intel/i915_drm.h:100 (-(1u << 20))

/// BAR indices and sizes: `GEN4_GTTMMADR_BAR` = 0 (MMIO + GTT), `GEN4_GMADR_BAR` = 2 (the mappable aperture).
pub const GEN4_GTTMMADR_BAR: u8 = 0; // intel_pci_config.h:19
pub const GEN4_GMADR_BAR: u8 = 2; // intel_pci_config.h:20
/// Gen8+: GTTMMADR is 16 MiB and the GTT starts halfway (`gen6_gttmmadr_size` / `gen6_gttadr_offset`, intel_ggtt.c:1180-1193).
pub const GEN8_GTTMMADR_SIZE: u64 = 16 << 20; // intel_ggtt.c:1187
pub const GEN8_GTTADR_OFFSET: u32 = 8 << 20; // intel_ggtt.c:1192 (= GTTMMADR_SIZE / 2)
