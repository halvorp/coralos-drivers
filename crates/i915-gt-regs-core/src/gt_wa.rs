// SPDX-License-Identifier: GPL-2.0-only
//! Registers the Gen9 / KBL workaround lists, MOCS, reset and clock-gating code touch (intel_gt_regs.h).
//! Ported from Linux drivers/gpu/drm/i915/gt/intel_gt_regs.h at v7.0-rc6; every line carries `file:line`.
//! Copyright (c) Intel Corporation and the i915 authors.

use i915_common_core::reg::{bit};
use i915_common_core::Reg;

pub const GEN8_MCR_SELECTOR: Reg = Reg::new(0xfdc); // intel_gt_regs.h:69
pub const GEN9_CSFE_CHICKEN1_RCS: Reg = Reg::new(0x20d4); // intel_gt_regs.h:161
pub const GEN9_PREEMPT_GPGPU_SYNC_SWITCH_DISABLE: u32 = 1 << 2; // intel_gt_regs.h:162
pub const GEN7_FF_SLICE_CS_CHICKEN1: Reg = Reg::new(0x20e0); // intel_gt_regs.h:167
pub const GEN9_FFSC_PERCTX_PREEMPT_CTRL: u32 = 1 << 14; // intel_gt_regs.h:168
pub const GEN9_CTX_PREEMPT_REG: Reg = Reg::new(0x2248); // intel_gt_regs.h:241
pub const FF_SLICE_CHICKEN: Reg = Reg::new(0x2088); // intel_gt_regs.h:104
pub const FF_SLICE_CHICKEN_CL_PROVOKING_VERTEX_FIX: u32 = 1 << 1; // intel_gt_regs.h:105
pub const _3D_CHICKEN3: Reg = Reg::new(0x2090); // intel_gt_regs.h:118
pub const _3D_CHICKEN_SF_PROVOKING_VERTEX_FIX: u32 = 1 << 12; // intel_gt_regs.h:119
pub const GEN8_CS_CHICKEN1: Reg = Reg::new(0x2580); // intel_gt_regs.h:284
pub const GEN9_PREEMPT_3D_OBJECT_LEVEL: u32 = 1 << 0; // intel_gt_regs.h:285
pub const GAM_ECOCHK: Reg = Reg::new(0x4090); // intel_gt_regs.h:303
pub const BDW_DISABLE_HDC_INVALIDATION: u32 = 1 << 25; // intel_gt_regs.h:304
pub const ECOCHK_DIS_TLB: u32 = 1 << 8; // intel_gt_regs.h:306
pub const GEN9_GAMT_ECO_REG_RW_IA: Reg = Reg::new(0x4ab0); // intel_gt_regs.h:381
pub const GAMT_ECO_ENABLE_IN_PLACE_DECOMPRESS: u32 = 1 << 18; // intel_gt_regs.h:382
pub const GAMT_CHKN_BIT_REG: Reg = Reg::new(0x4ab8); // intel_gt_regs.h:384
pub const GAMT_CHKN_DISABLE_DYNAMIC_CREDIT_SHARING: u32 = 1 << 28; // intel_gt_regs.h:386
pub const MMCD_MISC_CTRL: Reg = Reg::new(0x4ddc); // intel_gt_regs.h:406
pub const MMCD_PCLA: u32 = 1 << 31; // intel_gt_regs.h:407
pub const MMCD_HOTSPOT_EN: u32 = 1 << 27; // intel_gt_regs.h:408
pub const CACHE_MODE_1: Reg = Reg::new(0x7004); // intel_gt_regs.h:446
pub const GEN8_4X4_STC_OPTIMIZATION_DISABLE: u32 = bit(6); // intel_gt_regs.h:449 (Linux name `GEN8_4x4_STC_OPTIMIZATION_DISABLE`)
pub const GEN9_PARTIAL_RESOLVE_IN_VC_DISABLE: u32 = bit(1); // intel_gt_regs.h:450
pub const COMMON_SLICE_CHICKEN2: Reg = Reg::new(0x7014); // intel_gt_regs.h:461
pub const GEN9_PBE_COMPRESSED_HASH_SELECTION: u32 = 1 << 13; // intel_gt_regs.h:462
pub const GEN9_DISABLE_GATHER_AT_SET_SHADER_COMMON_SLICE: u32 = 1 << 12; // intel_gt_regs.h:463
pub const GEN8_SBE_DISABLE_REPLAY_BUF_OPTIMIZATION: u32 = 1 << 8; // intel_gt_regs.h:464
pub const HDC_CHICKEN0: Reg = Reg::new(0x7300); // intel_gt_regs.h:489
pub const HDC_FORCE_CSR_NON_COHERENT_OVR_DISABLE: u32 = 1 << 15; // intel_gt_regs.h:490
pub const HDC_FORCE_CONTEXT_SAVE_RESTORE_NON_COHERENT: u32 = 1 << 5; // intel_gt_regs.h:493
pub const HDC_FORCE_NON_COHERENT: u32 = 1 << 4; // intel_gt_regs.h:494
pub const GEN8_HDC_CHICKEN1: Reg = Reg::new(0x7304); // intel_gt_regs.h:500
pub const FBC_LLC_READ_CTRL: Reg = Reg::new(0x9044); // intel_gt_regs.h:574
pub const FBC_LLC_FULLY_OPEN: u32 = bit(30); // intel_gt_regs.h:575
pub const GEN6_UCGCTL1: Reg = Reg::new(0x9400); // intel_gt_regs.h:639
pub const GEN6_GAMUNIT_CLOCK_GATE_DISABLE: u32 = 1 << 22; // intel_gt_regs.h:640
pub const GEN7_UCGCTL4: Reg = Reg::new(0x940c); // intel_gt_regs.h:656
pub const GEN8_EU_GAUNIT_CLOCK_GATE_DISABLE: u32 = 1 << 14; // intel_gt_regs.h:658
pub const GEN6_GDRST: Reg = Reg::new(0x941c); // intel_gt_regs.h:663
pub const GEN6_GRDOM_FULL: u32 = 1 << 0; // intel_gt_regs.h:664
pub const GEN6_GRDOM_RENDER: u32 = 1 << 1; // intel_gt_regs.h:665
pub const GEN6_GRDOM_MEDIA: u32 = 1 << 2; // intel_gt_regs.h:666
pub const GEN6_GRDOM_BLT: u32 = 1 << 3; // intel_gt_regs.h:667
pub const GEN6_GRDOM_VECS: u32 = 1 << 4; // intel_gt_regs.h:668
pub const GEN9_GRDOM_GUC: u32 = 1 << 5; // intel_gt_regs.h:669
pub const GEN8_GRDOM_MEDIA2: u32 = 1 << 7; // intel_gt_regs.h:670
pub const GEN8_UCGCTL6: Reg = Reg::new(0x9430); // intel_gt_regs.h:713
pub const GEN8_SDEUNIT_CLOCK_GATE_DISABLE: u32 = 1 << 14; // intel_gt_regs.h:715
pub const GEN8_GARBCNTL: Reg = Reg::new(0xb004); // intel_gt_regs.h:940
pub const GEN9_GAPS_TSV_CREDIT_DISABLE: u32 = bit(7); // intel_gt_regs.h:943
pub const GEN9_SCRATCH_LNCF1: Reg = Reg::new(0xb008); // intel_gt_regs.h:947
pub const GEN9_LNCF_NONIA_COHERENT_ATOMICS_ENABLE: u32 = bit(0); // intel_gt_regs.h:948
pub const GEN8_L3SQCREG4: Reg = Reg::new(0xb118); // intel_gt_regs.h:992
pub const GEN8_LQSC_FLUSH_COHERENT_LINES: u32 = 1 << 21; // intel_gt_regs.h:995
pub const GEN8_LQSQ_NONIA_COHERENT_ATOMICS_ENABLE: u32 = bit(22); // intel_gt_regs.h:996
pub const GEN9_SCRATCH1: Reg = Reg::new(0xb11c); // intel_gt_regs.h:998
pub const EVICTION_PERF_FIX_ENABLE: u32 = bit(8); // intel_gt_regs.h:999
pub const BDW_SCRATCH1: Reg = Reg::new(0xb11c); // intel_gt_regs.h:1001
pub const GEN9_LBS_SLA_RETRY_TIMER_DECREMENT_ENABLE: u32 = 1 << 2; // intel_gt_regs.h:1002
pub const GEN8_HALF_SLICE_CHICKEN1: Reg = Reg::new(0xe100); // intel_gt_regs.h:1078
pub const GEN7_SBE_SS_CACHE_DISPATCH_PORT_SHARING_DISABLE: u32 = 1 << 4; // intel_gt_regs.h:1081
pub const HALF_SLICE_CHICKEN2: Reg = Reg::new(0xe180); // intel_gt_regs.h:1089
pub const GEN8_ST_PO_DISABLE: u32 = 1 << 13; // intel_gt_regs.h:1090
pub const GEN8_HALF_SLICE_CHICKEN3: Reg = Reg::new(0xe184); // intel_gt_regs.h:1093
pub const GEN8_SAMPLER_POWER_BYPASS_DIS: u32 = 1 << 1; // intel_gt_regs.h:1097
pub const GEN9_HALF_SLICE_CHICKEN5: Reg = Reg::new(0xe188); // intel_gt_regs.h:1099
pub const GEN9_CCS_TLB_PREFETCH_ENABLE: u32 = 1 << 3; // intel_gt_regs.h:1101
pub const GEN9_HALF_SLICE_CHICKEN7: Reg = Reg::new(0xe194); // intel_gt_regs.h:1110
pub const GEN9_SAMPLER_HASH_COMPRESSED_READ_ADDR: u32 = bit(8); // intel_gt_regs.h:1112
pub const GEN9_ENABLE_YV12_BUGFIX: u32 = bit(4); // intel_gt_regs.h:1113
pub const GEN9_ENABLE_GPGPU_PREEMPTION: u32 = bit(2); // intel_gt_regs.h:1114
pub const GEN8_ROW_CHICKEN: Reg = Reg::new(0xe4f0); // intel_gt_regs.h:1144
pub const FLOW_CONTROL_ENABLE: u32 = bit(15); // intel_gt_regs.h:1145
pub const PARTIAL_INSTRUCTION_SHOOTDOWN_DISABLE: u32 = bit(8); // intel_gt_regs.h:1148
pub const GEN9_RCS0_MOCS0: u32 = 0xc800; // intel_gt_regs.h:1023 (Linux name `__GEN9_RCS0_MOCS0`)
pub const GEN9_VCS0_MOCS0: u32 = 0xc900; // intel_gt_regs.h:1025 (Linux name `__GEN9_VCS0_MOCS0`)
pub const GEN9_VCS1_MOCS0: u32 = 0xca00; // intel_gt_regs.h:1027 (Linux name `__GEN9_VCS1_MOCS0`)
pub const GEN9_VECS0_MOCS0: u32 = 0xcb00; // intel_gt_regs.h:1029 (Linux name `__GEN9_VECS0_MOCS0`)
pub const GEN9_BCS0_MOCS0: u32 = 0xcc00; // intel_gt_regs.h:1031 (Linux name `__GEN9_BCS0_MOCS0`)
pub const LNCFCMOCS_REG_COUNT: u32 = 32; // intel_gt_regs.h:962

/// Every plain register (not the aliases) of this module, by name: checked for alignment and uniqueness by the tests.
pub const GT_WA_CATALOG: [(&str, Reg); 30] = [
    ("FF_SLICE_CHICKEN", FF_SLICE_CHICKEN),
    ("_3D_CHICKEN3", _3D_CHICKEN3),
    ("GEN8_MCR_SELECTOR", GEN8_MCR_SELECTOR),
    ("GEN9_CSFE_CHICKEN1_RCS", GEN9_CSFE_CHICKEN1_RCS),
    ("GEN7_FF_SLICE_CS_CHICKEN1", GEN7_FF_SLICE_CS_CHICKEN1),
    ("GEN9_CTX_PREEMPT_REG", GEN9_CTX_PREEMPT_REG),
    ("GEN8_CS_CHICKEN1", GEN8_CS_CHICKEN1),
    ("GAM_ECOCHK", GAM_ECOCHK),
    ("GEN9_GAMT_ECO_REG_RW_IA", GEN9_GAMT_ECO_REG_RW_IA),
    ("GAMT_CHKN_BIT_REG", GAMT_CHKN_BIT_REG),
    ("MMCD_MISC_CTRL", MMCD_MISC_CTRL),
    ("CACHE_MODE_1", CACHE_MODE_1),
    ("COMMON_SLICE_CHICKEN2", COMMON_SLICE_CHICKEN2),
    ("HDC_CHICKEN0", HDC_CHICKEN0),
    ("GEN8_HDC_CHICKEN1", GEN8_HDC_CHICKEN1),
    ("FBC_LLC_READ_CTRL", FBC_LLC_READ_CTRL),
    ("GEN6_UCGCTL1", GEN6_UCGCTL1),
    ("GEN7_UCGCTL4", GEN7_UCGCTL4),
    ("GEN6_GDRST", GEN6_GDRST),
    ("GEN8_UCGCTL6", GEN8_UCGCTL6),
    ("GEN8_GARBCNTL", GEN8_GARBCNTL),
    ("GEN9_SCRATCH_LNCF1", GEN9_SCRATCH_LNCF1),
    ("GEN8_L3SQCREG4", GEN8_L3SQCREG4),
    ("GEN9_SCRATCH1", GEN9_SCRATCH1),
    ("GEN8_HALF_SLICE_CHICKEN1", GEN8_HALF_SLICE_CHICKEN1),
    ("HALF_SLICE_CHICKEN2", HALF_SLICE_CHICKEN2),
    ("GEN8_HALF_SLICE_CHICKEN3", GEN8_HALF_SLICE_CHICKEN3),
    ("GEN9_HALF_SLICE_CHICKEN5", GEN9_HALF_SLICE_CHICKEN5),
    ("GEN9_HALF_SLICE_CHICKEN7", GEN9_HALF_SLICE_CHICKEN7),
    ("GEN8_ROW_CHICKEN", GEN8_ROW_CHICKEN),
];

// ---- Macros with arguments ---------------------------------------------------------------------------------------------

/// `GEN8_MCR_SLICE(slice)` (intel_gt_regs.h:71).
pub const fn gen8_mcr_slice(slice: u32) -> u32 {
    (slice & 3) << 26
}
/// `GEN8_MCR_SLICE_MASK` (intel_gt_regs.h:72).
pub const GEN8_MCR_SLICE_MASK: u32 = gen8_mcr_slice(3); // intel_gt_regs.h:72
/// `GEN8_MCR_SUBSLICE(subslice)` (intel_gt_regs.h:73).
pub const fn gen8_mcr_subslice(subslice: u32) -> u32 {
    (subslice & 3) << 24
}
/// `GEN8_MCR_SUBSLICE_MASK` (intel_gt_regs.h:74).
pub const GEN8_MCR_SUBSLICE_MASK: u32 = gen8_mcr_subslice(3); // intel_gt_regs.h:74

/// `GEN9_PREEMPT_GPGPU_LEVEL(hi, lo)` (intel_gt_regs.h:286).
pub const fn gen9_preempt_gpgpu_level(hi: u32, lo: u32) -> u32 {
    (hi << 2) | (lo << 1)
}
/// Zero-valued (intel_gt_regs.h:287): pinned by being distinct from its siblings.
pub const GEN9_PREEMPT_GPGPU_MID_THREAD_LEVEL: u32 = gen9_preempt_gpgpu_level(0, 0); // intel_gt_regs.h:287
pub const GEN9_PREEMPT_GPGPU_THREAD_GROUP_LEVEL: u32 = gen9_preempt_gpgpu_level(0, 1); // intel_gt_regs.h:288
pub const GEN9_PREEMPT_GPGPU_COMMAND_LEVEL: u32 = gen9_preempt_gpgpu_level(1, 0); // intel_gt_regs.h:289
pub const GEN9_PREEMPT_GPGPU_LEVEL_MASK: u32 = gen9_preempt_gpgpu_level(1, 1); // intel_gt_regs.h:290

/// `GEN9_LNCFCMOCS(i)` (intel_gt_regs.h:960): the L3 cache-control MOCS word `i` (two 16-bit entries per word).
pub const fn gen9_lncfcmocs(i: u32) -> Reg {
    Reg::nth(0xb020, i, 4)
}

/// Linux's second names for an offset another constant here already holds. `(second_name_reg, home_reg)`.
pub const WA_ALIASES: [(Reg, Reg); 1] = [(BDW_SCRATCH1, GEN9_SCRATCH1)];
