// SPDX-License-Identifier: GPL-2.0-only
//! The logical ring context layout and the execlists wire constants: gt/intel_lrc_reg.h, the context-descriptor bits of
//! gt/intel_lrc.h, the CSB / execlist-status bits that live at the top of gt/intel_execlists_submission.c, and the HWSP
//! dword map of gt/intel_engine.h. These are indices into a memory image and bits of memory words, not MMIO registers; the
//! registers they mirror live in `i915-gt-regs-core`. Ported from Linux v7.0-rc6, Gen8/Gen9 subset (the Gen10+ indirect-ctx
//! defaults, the Gen11/Gen12/Xe_HP descriptor layouts and the 12-entry CSB are not carried: no Kaby Lake path reads them).
//! Copyright (c) Intel Corporation and the i915 authors.
use i915_common_core::reg::genmask;

// ---- gt/intel_lrc_reg.h: dword indices into `ce->lrc_reg_state` (the context's second page) --------------------------------

/// `CTX_DESC_FORCE_RESTORE` (intel_lrc_reg.h:11): bit 2 of the descriptor, the same bit as [`GEN8_CTX_FORCE_RESTORE`].
pub const CTX_DESC_FORCE_RESTORE: u64 = 1 << 2; // intel_lrc_reg.h:11
pub const CTX_CONTEXT_CONTROL: usize = 0x02 + 1; // intel_lrc_reg.h:14
pub const CTX_RING_HEAD: usize = 0x04 + 1; // intel_lrc_reg.h:15
pub const CTX_RING_TAIL: usize = 0x06 + 1; // intel_lrc_reg.h:16
pub const CTX_RING_START: usize = 0x08 + 1; // intel_lrc_reg.h:17
pub const CTX_RING_CTL: usize = 0x0a + 1; // intel_lrc_reg.h:18
pub const CTX_BB_STATE: usize = 0x10 + 1; // intel_lrc_reg.h:19
pub const CTX_TIMESTAMP: usize = 0x22 + 1; // intel_lrc_reg.h:20
pub const CTX_PDP3_UDW: usize = 0x24 + 1; // intel_lrc_reg.h:21
pub const CTX_PDP3_LDW: usize = 0x26 + 1; // intel_lrc_reg.h:22
pub const CTX_PDP2_UDW: usize = 0x28 + 1; // intel_lrc_reg.h:23
pub const CTX_PDP2_LDW: usize = 0x2a + 1; // intel_lrc_reg.h:24
pub const CTX_PDP1_UDW: usize = 0x2c + 1; // intel_lrc_reg.h:25
pub const CTX_PDP1_LDW: usize = 0x2e + 1; // intel_lrc_reg.h:26
pub const CTX_PDP0_UDW: usize = 0x30 + 1; // intel_lrc_reg.h:27
pub const CTX_PDP0_LDW: usize = 0x32 + 1; // intel_lrc_reg.h:28
pub const CTX_R_PWR_CLK_STATE: usize = 0x42 + 1; // intel_lrc_reg.h:29
pub const GEN9_CTX_RING_MI_MODE: usize = 0x54; // intel_lrc_reg.h:31
pub const GEN8_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT: u32 = 0x17; // intel_lrc_reg.h:47
pub const GEN9_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT: u32 = 0x26; // intel_lrc_reg.h:48

/// `GEN8_EXECLISTS_STATUS_BUF` (intel_lrc_reg.h:53): the MMIO mirror of the CSB entries, `mmio_base + 0x370 + 8 * entry`.
pub const GEN8_EXECLISTS_STATUS_BUF: u32 = 0x370; // intel_lrc_reg.h:53
/// `GEN8_CSB_ENTRIES` (intel_lrc_reg.h:64): the Gen8..Gen10 context status buffer holds six qword entries.
pub const GEN8_CSB_ENTRIES: u32 = 6; // intel_lrc_reg.h:64
pub const GEN8_CSB_PTR_MASK: u32 = 0x7; // intel_lrc_reg.h:65
pub const GEN8_CSB_READ_PTR_MASK: u32 = GEN8_CSB_PTR_MASK << 8; // intel_lrc_reg.h:66
pub const GEN8_CSB_WRITE_PTR_MASK: u32 = GEN8_CSB_PTR_MASK; // intel_lrc_reg.h:67
/// `MAX_CONTEXT_HW_ID` (intel_lrc_reg.h:74), exclusive: the Gen8/Gen9 hardware context id is 21 bits wide.
pub const MAX_CONTEXT_HW_ID: u32 = 1 << 21; // intel_lrc_reg.h:74

// ---- gt/intel_lrc.h ---------------------------------------------------------------------------------------------------------

/// `LRC_PPHWSP_PN` / `LRC_PPHWSP_SZ` (intel_lrc.h:23-24): the per-process HWSP is page 0, one page.
pub const LRC_PPHWSP_PN: usize = 0; // intel_lrc.h:23
pub const LRC_PPHWSP_SZ: usize = 1; // intel_lrc.h:24
/// `LRC_STATE_PN` (intel_lrc.h:26): the logical state follows the PPHWSP.
pub const LRC_STATE_PN: usize = LRC_PPHWSP_PN + LRC_PPHWSP_SZ; // intel_lrc.h:26
/// `LRC_STATE_OFFSET` (intel_lrc.h:27) in bytes (`LRC_STATE_PN * PAGE_SIZE`).
pub const LRC_STATE_OFFSET: u32 = LRC_STATE_PN as u32 * 4096; // intel_lrc.h:27
/// `LRC_PPHWSP_SCRATCH` (intel_lrc.h:30): the dword of the PPHWSP reserved as scratch. The byte address
/// (`LRC_PPHWSP_SCRATCH_ADDR`, :31) is owned by `i915_gt_core::emit`.
pub const LRC_PPHWSP_SCRATCH: u32 = 0x34; // intel_lrc.h:30

/// The unnamed enum of addressing modes (intel_lrc.h:76-81): `INTEL_ADVANCED_CONTEXT` is zero-valued.
pub const INTEL_ADVANCED_CONTEXT: u64 = 0; // intel_lrc.h:77
pub const INTEL_LEGACY_32B_CONTEXT: u64 = 1; // intel_lrc.h:78
pub const INTEL_ADVANCED_AD_CONTEXT: u64 = 2; // intel_lrc.h:79
pub const INTEL_LEGACY_64B_CONTEXT: u64 = 3; // intel_lrc.h:80

pub const CTX_GTT_ADDRESS_MASK: u32 = genmask(31, 12); // intel_lrc.h:90
pub const GEN8_CTX_VALID: u64 = 1 << 0; // intel_lrc.h:91
pub const GEN8_CTX_FORCE_PD_RESTORE: u64 = 1 << 1; // intel_lrc.h:92
pub const GEN8_CTX_FORCE_RESTORE: u64 = 1 << 2; // intel_lrc.h:93
pub const GEN8_CTX_L3LLC_COHERENT: u64 = 1 << 5; // intel_lrc.h:94
pub const GEN8_CTX_PRIVILEGE: u64 = 1 << 8; // intel_lrc.h:95
pub const GEN8_CTX_ADDRESSING_MODE_SHIFT: u32 = 3; // intel_lrc.h:96
pub const GEN8_CTX_ID_SHIFT: u32 = 32; // intel_lrc.h:101
pub const GEN8_CTX_ID_WIDTH: u32 = 21; // intel_lrc.h:102
/// `GEN11_SW_CTX_ID_SHIFT` (intel_lrc.h:103): `__execlists_schedule_in` builds the Gen8/Gen9 ccid with this shift too
/// (intel_execlists_submission.c:509), which on Gen9 lands inside the 21-bit hardware id field at bits 37..47.
pub const GEN11_SW_CTX_ID_SHIFT: u32 = 37; // intel_lrc.h:103
pub const GEN11_SW_CTX_ID_WIDTH: u32 = 11; // intel_lrc.h:104
/// `GEN12_MAX_CONTEXT_HW_ID` (intel_lrc_reg.h:77) = `GEN11_MAX_CONTEXT_HW_ID - 1` with `GEN11_MAX_CONTEXT_HW_ID` = 1 << 11.
pub const GEN12_MAX_CONTEXT_HW_ID: u32 = (1 << 11) - 1; // intel_lrc_reg.h:77

// ---- gt/intel_execlists_submission.c: execlist status and context-status bits ----------------------------------------------

pub const RING_EXECLIST_QFULL: u32 = 1 << 0x2; // intel_execlists_submission.c:143
pub const RING_EXECLIST1_VALID: u32 = 1 << 0x3; // intel_execlists_submission.c:144
pub const RING_EXECLIST0_VALID: u32 = 1 << 0x4; // intel_execlists_submission.c:145
pub const RING_EXECLIST_ACTIVE_STATUS: u32 = 3 << 0xe; // intel_execlists_submission.c:146
pub const RING_EXECLIST1_ACTIVE: u32 = 1 << 0x11; // intel_execlists_submission.c:147
pub const RING_EXECLIST0_ACTIVE: u32 = 1 << 0x12; // intel_execlists_submission.c:148

pub const GEN8_CTX_STATUS_IDLE_ACTIVE: u32 = 1 << 0; // intel_execlists_submission.c:150
pub const GEN8_CTX_STATUS_PREEMPTED: u32 = 1 << 1; // intel_execlists_submission.c:151
pub const GEN8_CTX_STATUS_ELEMENT_SWITCH: u32 = 1 << 2; // intel_execlists_submission.c:152
pub const GEN8_CTX_STATUS_ACTIVE_IDLE: u32 = 1 << 3; // intel_execlists_submission.c:153
pub const GEN8_CTX_STATUS_COMPLETE: u32 = 1 << 4; // intel_execlists_submission.c:154
pub const GEN8_CTX_STATUS_LITE_RESTORE: u32 = 1 << 15; // intel_execlists_submission.c:155
pub const GEN8_CTX_STATUS_COMPLETED_MASK: u32 = GEN8_CTX_STATUS_COMPLETE | GEN8_CTX_STATUS_PREEMPTED; // intel_execlists_submission.c:157

/// `EXECLISTS_REQUEST_SIZE` (intel_execlists_submission.c:174): "typical size of the average request", bytes of ring space
/// reserved on top of the breadcrumb while the request header (the invalidating flush) is emitted.
pub const EXECLISTS_REQUEST_SIZE: u32 = 64; // intel_execlists_submission.c:174

// ---- gt/intel_engine.h: the engine status page (HWSP) dword map -------------------------------------------------------------

/// Dword indices into the engine's status page. (`I915_GEM_HWS_PREEMPT_ADDR`, the byte address, is owned by `i915_gt_core::emit`.)
pub const I915_GEM_HWS_PREEMPT: u32 = 0x32; // intel_engine.h:188
pub const I915_GEM_HWS_SEQNO: u32 = 0x40; // intel_engine.h:190
pub const I915_GEM_HWS_MIGRATE_ADDR: u32 = 0x42 * 4; // intel_engine.h:192
pub const I915_GEM_HWS_GGTT_BIND: u32 = 0x46; // intel_engine.h:193
pub const I915_GEM_HWS_PXP: u32 = 0x60; // intel_engine.h:195
pub const I915_GEM_HWS_GSC: u32 = 0x62; // intel_engine.h:197
pub const I915_GEM_HWS_SCRATCH: u32 = 0x80; // intel_engine.h:199
/// `I915_HWS_CSB_BUF0_INDEX` (intel_engine.h:201): CSB entry 0 is the qword at dwords 0x10/0x11.
pub const I915_HWS_CSB_BUF0_INDEX: u32 = 0x10; // intel_engine.h:201
/// `I915_HWS_CSB_WRITE_INDEX` (intel_engine.h:202): the CSB write pointer dword (Gen8..Gen10).
pub const I915_HWS_CSB_WRITE_INDEX: u32 = 0x1f; // intel_engine.h:202

/// `CACHELINE_BYTES` (intel_ring_types.h:19).
pub const CACHELINE_BYTES: u32 = 64; // intel_ring_types.h:19
/// `CACHELINE_DWORDS` (intel_ring_types.h:20).
pub const CACHELINE_DWORDS: u32 = CACHELINE_BYTES / 4; // intel_ring_types.h:20
