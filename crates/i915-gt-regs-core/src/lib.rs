// SPDX-License-Identifier: GPL-2.0-only
//! i915-gt-regs-core: GT register offsets and fields, ONE definition each (the contract's "one home per register" rule,
//! docs/I915-CONTRACTS.md). A later crate that needs a GT register imports it from here and, if it is missing, ADDS it here -
//! it never re-types an offset.
//!
//! Ported from Linux drivers/gpu/drm/i915/gt/intel_gt_regs.h, intel_mchbar_regs.h and i915_reg.h at v7.0-rc6; every line
//! carries `file:line`. Where Linux gives two names to one offset (`FORCEWAKE_MT` and `FORCEWAKE_GT_GEN9` are both 0xa188) the
//! alias is a separate constant with its own citation and [`ALIASES`] lists it, so a duplicate offset is always deliberate.
//! Copyright (c) Intel Corporation and the i915 authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod engine_regs;
pub mod gt_wa;
pub mod guc_reg;
pub mod i915_reg;
pub mod pci_config;

pub use engine_regs::*;
pub use gt_wa::*;
pub use guc_reg::*;
pub use i915_reg::*;
pub use pci_config::*;

use i915_common_core::reg::{bit, genmask};
use i915_common_core::Reg;

// ---- Forcewake: Gen9 (SKL, KBL, ...) ----------------------------------------------------------------------------------

pub const FORCEWAKE_GT_GEN9: Reg = Reg::new(0xa188); // intel_gt_regs.h:864
pub const FORCEWAKE_MEDIA_GEN9: Reg = Reg::new(0xa270); // intel_gt_regs.h:909
pub const FORCEWAKE_RENDER_GEN9: Reg = Reg::new(0xa278); // intel_gt_regs.h:910
pub const FORCEWAKE_ACK_GT_GEN9: Reg = Reg::new(0x130044); // intel_gt_regs.h:1496
pub const FORCEWAKE_ACK_RENDER_GEN9: Reg = Reg::new(0xd84); // intel_gt_regs.h:56
pub const FORCEWAKE_ACK_MEDIA_GEN9: Reg = Reg::new(0xd88); // intel_gt_regs.h:57

/// Forcewake request/ack bits (the same bits in every domain's set and ack register).
pub const FORCEWAKE_KERNEL: u32 = bit(0); // intel_gt_regs.h:1497
pub const FORCEWAKE_USER: u32 = bit(1); // intel_gt_regs.h:1498
pub const FORCEWAKE_KERNEL_FALLBACK: u32 = bit(15); // intel_gt_regs.h:1499

// ---- Forcewake: Valleyview / Cherryview (Gen8 CHV) - the NEGATIVE CONTROL: never programmed on a Gen9 part -------------

pub const FORCEWAKE_VLV: Reg = Reg::new(0x1300b0); // intel_gt_regs.h:1513
pub const FORCEWAKE_ACK_VLV: Reg = Reg::new(0x1300b4); // intel_gt_regs.h:1514
pub const FORCEWAKE_MEDIA_VLV: Reg = Reg::new(0x1300b8); // intel_gt_regs.h:1515
pub const FORCEWAKE_ACK_MEDIA_VLV: Reg = Reg::new(0x1300bc); // intel_gt_regs.h:1516

/// Linux's second names for an offset another constant already holds (kept so a duplicate is deliberate).
pub const FORCEWAKE_MT: Reg = Reg::new(0xa188); // intel_gt_regs.h:863
pub const FORCEWAKE_ACK_HSW: Reg = Reg::new(0x130044); // intel_gt_regs.h:1495
pub const ALIASES: [(Reg, Reg); 2] = [(FORCEWAKE_MT, FORCEWAKE_GT_GEN9), (FORCEWAKE_ACK_HSW, FORCEWAKE_ACK_GT_GEN9)];

// ---- GT fuses and topology (read by intel_sseu.c: gen9_sseu_info_init) -------------------------------------------------

pub const GEN8_FUSE2: Reg = Reg::new(0x9120); // intel_gt_regs.h:601
/// Enabled-slice mask, bits 27:25 (Gen8/Gen9).
pub const GEN8_F2_S_ENA_MASK: u32 = genmask(27, 25); // intel_gt_regs.h:604
/// Subslice-disable field, bits 23:20 (Gen9; Gen8's is 23:21, `GEN8_F2_SS_DIS_MASK`).
pub const GEN9_F2_SS_DIS_MASK: u32 = genmask(23, 20); // intel_gt_regs.h:605
pub const GEN8_F2_SS_DIS_MASK: u32 = genmask(23, 21); // intel_gt_regs.h:606

pub const GEN8_EU_DISABLE0: Reg = Reg::new(0x9134); // intel_gt_regs.h:608
pub const GEN8_EU_DISABLE1: Reg = Reg::new(0x9138); // intel_gt_regs.h:617
pub const GEN8_EU_DISABLE2: Reg = Reg::new(0x913c); // intel_gt_regs.h:624
/// `GEN9_EU_DISABLE(slice)` = 0x9134 + slice * 4 (intel_gt_regs.h:609).
pub const fn gen9_eu_disable(slice: u32) -> Reg {
    Reg::nth(0x9134, slice, 4)
}

// ---- Clocks -----------------------------------------------------------------------------------------------------------

pub const RPM_CONFIG0: Reg = Reg::new(0xd00); // intel_gt_regs.h:32
pub const GEN9_RPM_CONFIG0_CRYSTAL_CLOCK_FREQ_MASK: u32 = bit(3); // intel_gt_regs.h:38
pub const GEN10_RPM_CONFIG0_CTC_SHIFT_PARAMETER_MASK: u32 = genmask(2, 1); // intel_gt_regs.h:41

pub const CTC_MODE: Reg = Reg::new(0xa26c); // intel_gt_regs.h:881
pub const CTC_SHIFT_PARAMETER_MASK: u32 = genmask(2, 1); // intel_gt_regs.h:882
pub const CTC_SOURCE_PARAMETER_MASK: u32 = bit(0); // intel_gt_regs.h:883
/// `CTC_SOURCE_PARAMETER_MASK` field value 0: the timestamp counts a crystal-derived clock (intel_gt_regs.h:884).
pub const CTC_SOURCE_CRYSTAL_CLOCK: u32 = 0; // intel_gt_regs.h:884
/// Field value 1: the timestamp comes from the divide logic and `GEN9_TIMESTAMP_OVERRIDE` (intel_gt_regs.h:885).
pub const CTC_SOURCE_DIVIDE_LOGIC: u32 = 1; // intel_gt_regs.h:885

pub const GEN9_TIMESTAMP_OVERRIDE: Reg = Reg::new(0x44074); // i915_reg.h:1207
pub const GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DIVIDER_MASK: u32 = 0x3ff; // i915_reg.h:1209
pub const GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_SHIFT: u32 = 12; // i915_reg.h:1210
pub const GEN9_TIMESTAMP_OVERRIDE_US_COUNTER_DENOMINATOR_MASK: u32 = 0xf << 12; // i915_reg.h:1211

// ---- Render P-state (read only here) ----------------------------------------------------------------------------------

/// `MCHBAR_MIRROR_BASE_SNB` (intel_mchbar_regs.h:23): the MCHBAR mirror inside BAR0.
pub const MCHBAR_MIRROR_BASE_SNB: u32 = 0x140000; // intel_mchbar_regs.h:23
/// `GEN6_RP_STATE_CAP` (intel_mchbar_regs.h:225) = MCHBAR_MIRROR_BASE_SNB + 0x5998 = 0x145998. Gen9 LP (BXT/GLK) uses
/// `BXT_RP_STATE_CAP` (0x138170, i915_reg.h:732) instead; KBL is not LP.
pub const GEN6_RP_STATE_CAP: Reg = Reg::new(MCHBAR_MIRROR_BASE_SNB + 0x5998); // intel_mchbar_regs.h:225
pub const RP0_CAP_MASK: u32 = genmask(7, 0); // intel_mchbar_regs.h:226
pub const RP1_CAP_MASK: u32 = genmask(15, 8); // intel_mchbar_regs.h:227
pub const RPN_CAP_MASK: u32 = genmask(23, 16); // intel_mchbar_regs.h:228

// ---- GGTT flush and the private PAT (moved from i915-memory-core, sentinel audit 2026-10-12) ---------------------------

/// GGTT invalidate: writing `GFX_FLSH_CNTL_EN` flushes the write-combining buffer of GGTT PTE stores and the GT TLB.
pub const GFX_FLSH_CNTL_GEN6: Reg = Reg::new(0x101008); // intel_gt_regs.h:1474
pub const GFX_FLSH_CNTL_EN: u32 = bit(0); // intel_gt_regs.h:1475
/// The private PAT (8 entries x 8 bits): low and high dword.
pub const GEN8_PRIVATE_PAT_LO: Reg = Reg::new(0x40e0); // intel_gt_regs.h:336
pub const GEN8_PRIVATE_PAT_HI: Reg = Reg::new(0x40e0 + 4); // intel_gt_regs.h:337

/// Every register defined above (not the aliases), by Linux's name - the catalog `tests/catalog.rs` checks for aligned,
/// unique, in-range offsets.
pub const CATALOG: [(&str, Reg); 23] = [
    ("FORCEWAKE_GT_GEN9", FORCEWAKE_GT_GEN9),
    ("FORCEWAKE_MEDIA_GEN9", FORCEWAKE_MEDIA_GEN9),
    ("FORCEWAKE_RENDER_GEN9", FORCEWAKE_RENDER_GEN9),
    ("FORCEWAKE_ACK_GT_GEN9", FORCEWAKE_ACK_GT_GEN9),
    ("FORCEWAKE_ACK_RENDER_GEN9", FORCEWAKE_ACK_RENDER_GEN9),
    ("FORCEWAKE_ACK_MEDIA_GEN9", FORCEWAKE_ACK_MEDIA_GEN9),
    ("FORCEWAKE_VLV", FORCEWAKE_VLV),
    ("FORCEWAKE_ACK_VLV", FORCEWAKE_ACK_VLV),
    ("FORCEWAKE_MEDIA_VLV", FORCEWAKE_MEDIA_VLV),
    ("FORCEWAKE_ACK_MEDIA_VLV", FORCEWAKE_ACK_MEDIA_VLV),
    ("GEN8_FUSE2", GEN8_FUSE2),
    ("GEN8_EU_DISABLE0", GEN8_EU_DISABLE0),
    ("GEN8_EU_DISABLE1", GEN8_EU_DISABLE1),
    ("GEN8_EU_DISABLE2", GEN8_EU_DISABLE2),
    ("RPM_CONFIG0", RPM_CONFIG0),
    ("CTC_MODE", CTC_MODE),
    ("GEN9_TIMESTAMP_OVERRIDE", GEN9_TIMESTAMP_OVERRIDE),
    ("GEN6_RP_STATE_CAP", GEN6_RP_STATE_CAP),
    ("GFX_FLSH_CNTL_GEN6", GFX_FLSH_CNTL_GEN6),
    ("GEN8_PRIVATE_PAT_LO", GEN8_PRIVATE_PAT_LO),
    ("GEN8_PRIVATE_PAT_HI", GEN8_PRIVATE_PAT_HI),
    ("GEN6_STOLEN_RESERVED", GEN6_STOLEN_RESERVED),
    ("GEN8_GTCR", GEN8_GTCR),
];
