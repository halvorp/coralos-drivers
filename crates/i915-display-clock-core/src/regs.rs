// SPDX-License-Identifier: GPL-2.0-only
//! Registers this crate needs that `i915-kbl-display-core` does not define yet.
//!
//! HOME DECISION. `i915-kbl-display-core::regs` is the home of the display registers it already defines, and this crate imports
//! those (`pub use` below) rather than re-typing them. The handful below are NEW: they are used only by the write side
//! (CFGCR packing, the CDCLK divmux override, the PCU mailbox, the DFSM limit fuse) that the read-only readout never touched.
//! They live here, in one flagged module, because the worker brief forbids editing another crate while sibling workers edit it
//! in parallel; the merge step moves them into `i915-kbl-display-core/src/regs.rs` (the `intel_display_regs.h` home) verbatim.
//! Nothing else in this crate declares a register offset.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display_regs.h and drivers/gpu/drm/i915/i915_reg.h at v7.0-rc6.
//! Copyright (c) Intel Corporation.

pub use i915_kbl_display_core::regs::*;

/// `DPLL_CFGCR1_FREQ_ENABLE` (intel_display_regs.h:2587).
pub const DPLL_CFGCR1_FREQ_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2587
/// `DPLL_CFGCR2_CENTRAL_FREQ_MASK` (intel_display_regs.h:2612).
pub const DPLL_CFGCR2_CENTRAL_FREQ_MASK: u32 = 3; // intel_display_regs.h:2612
/// `CDCLK_DIVMUX_CD_OVERRIDE` (intel_display_regs.h:2533).
pub const CDCLK_DIVMUX_CD_OVERRIDE: u32 = 1 << 19; // intel_display_regs.h:2533

/// `SKL_DFSM` (intel_display_regs.h:1539): the display fuse word whose bits 24:23 cap CDCLK.
pub const SKL_DFSM: u32 = 0x51000; // intel_display_regs.h:1539
pub const SKL_DFSM_CDCLK_LIMIT_MASK: u32 = 3 << 23; // intel_display_regs.h:1542
pub const SKL_DFSM_CDCLK_LIMIT_675: u32 = 0 << 23; // intel_display_regs.h:1543
pub const SKL_DFSM_CDCLK_LIMIT_540: u32 = 1 << 23; // intel_display_regs.h:1544
pub const SKL_DFSM_CDCLK_LIMIT_450: u32 = 2 << 23; // intel_display_regs.h:1545
pub const SKL_DFSM_CDCLK_LIMIT_337_5: u32 = 3 << 23; // intel_display_regs.h:1546

/// `HSW_PWR_WELL_CTL3` / `CTL4`: the KVMR and DEBUG requester registers (intel_display_regs.h:2127-2128). Never written here.
pub const HSW_PWR_WELL_CTL3: u32 = 0x45408; // intel_display_regs.h:2127
pub const HSW_PWR_WELL_CTL4: u32 = 0x4540c; // intel_display_regs.h:2128

/// `enum skl_power_gate` (intel_display_regs.h:2215-2221): the fuse-distribution gates SKL_FUSE_STATUS reports.
pub const SKL_PG0: u32 = 0; // intel_display_regs.h:2216
pub const SKL_PG1: u32 = 1; // intel_display_regs.h:2217
pub const SKL_PG2: u32 = 2; // intel_display_regs.h:2218

/// The PCU mailbox: `GEN6_PCODE_MAILBOX` / `DATA` / `DATA1` (i915_reg.h:1079, :1179, :1182). Outside the forcewake domains
/// (intel_pcode.c:65-67 uses the `_fw` accessors).
pub const GEN6_PCODE_MAILBOX: u32 = 0x138124; // i915_reg.h:1079
pub const GEN6_PCODE_DATA: u32 = 0x138128; // i915_reg.h:1179
pub const GEN6_PCODE_DATA1: u32 = 0x13812c; // i915_reg.h:1182
pub const GEN6_PCODE_READY: u32 = 1 << 31; // i915_reg.h:1080
pub const GEN6_PCODE_ERROR_MASK: u32 = 0xff; // i915_reg.h:1084
pub const GEN6_PCODE_SUCCESS: u32 = 0x0; // i915_reg.h:1085
pub const GEN6_PCODE_ILLEGAL_CMD: u32 = 0x1; // i915_reg.h:1086
pub const GEN7_PCODE_TIMEOUT: u32 = 0x2; // i915_reg.h:1090
pub const GEN7_PCODE_ILLEGAL_DATA: u32 = 0x3; // i915_reg.h:1091
pub const GEN11_PCODE_ILLEGAL_SUBCOMMAND: u32 = 0x4; // i915_reg.h:1092
pub const GEN11_PCODE_LOCKED: u32 = 0x6; // i915_reg.h:1093
pub const GEN11_PCODE_REJECTED: u32 = 0x11; // i915_reg.h:1094
pub const GEN7_PCODE_MIN_FREQ_TABLE_GT_RATIO_OUT_OF_RANGE: u32 = 0x10; // i915_reg.h:1095
/// `SKL_PCODE_CDCLK_CONTROL` and its two request/reply words (i915_reg.h:1107-1109).
pub const SKL_PCODE_CDCLK_CONTROL: u32 = 0x7; // i915_reg.h:1107
pub const SKL_CDCLK_PREPARE_FOR_CHANGE: u32 = 0x3; // i915_reg.h:1108
pub const SKL_CDCLK_READY_FOR_CHANGE: u32 = 0x1; // i915_reg.h:1109

/// `SWF_ILK(0x18)`: the pre-OS "display initialised" scratch word `skl_sanitize_cdclk` reads (intel_display_regs.h:1004,
/// intel_cdclk.c:1239).
pub const SWF_ILK_18: u32 = 0x4f000 + 0x18 * 4; // intel_display_regs.h:1004
