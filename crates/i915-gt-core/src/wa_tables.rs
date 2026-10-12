// SPDX-License-Identifier: GPL-2.0-only
//! The Gen9 / Kaby Lake workaround lists as DATA: one [`WaRow`] per `wa_*` call of gt/intel_workarounds.c, carrying the Linux
//! workaround name, the register macro it names and the `file:line` of the call. The lists a row set builds are
//! `gen9_ctx_workarounds_init` + `kbl_ctx_workarounds_init` (context), `gen9_gt_workarounds_init` + `kbl_gt_workarounds_init`
//! (GT), the Gen9 branches of `rcs_engine_wa_init` / `xcs_engine_wa_init` (engine) and `gen9_whitelist_build` +
//! `kbl_whitelist_build` (whitelist).
//!
//! NOT PORTED, by name: the Gen9-LP rows (`IS_GEN9_LP`: `GEN9_WM_CHICKEN3` :543, `GEN8_L3SQCREG1` :2487, Broxton's
//! `FF_SLICE_CS_CHICKEN2` :2469) - no LP platform row exists in i915-devinfo-core; the Skylake-only rows (`skl_gt_workarounds_init`
//! :1209-1224, `skl_tune_iz_hashing`, `skl_whitelist_build`) - Skylake is carried for fuse/ids but its workaround lists are not
//! this crate's job; Coffee/Comet Lake. A row's [`Cond`] evaluates the Linux predicate against a [`WaContext`].
use crate::engine::{EngineClass, EngineInfo};
use crate::error::GtError;
use crate::step::{is_graphics_step, Step};
use crate::wa::{Wa, WaList};
use i915_common_core::Reg;
use i915_devinfo_core::Platform;
use i915_gt_regs_core as r;

/// The platform facts the Linux predicates read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaContext {
    pub platform: Platform,
    /// `GRAPHICS_VER(i915)`.
    pub graphics_ver: u8,
    /// `HAS_LLC(i915)`.
    pub has_llc: bool,
    /// `INTEL_GRAPHICS_STEP(i915)`.
    pub step: Step,
}

/// A Linux `if (...)` guarding a `wa_*` call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cond {
    Always,
    /// `HAS_LLC(i915)`.
    HasLlc,
    /// `IS_SKYLAKE || IS_KABYLAKE || IS_COFFEELAKE || IS_COMETLAKE`.
    SklKblCflCml,
    /// `!IS_COFFEELAKE && !IS_COMETLAKE`: a constant true here (devinfo-core's `Platform` has no CFL / CML row).
    NotCflCml,
    /// `GRAPHICS_VER(i915) >= 9`.
    GenAtLeast9,
    /// `GRAPHICS_VER(i915) == 9`.
    GenIs9,
    /// `IS_KABYLAKE(i915) && IS_GRAPHICS_STEP(i915, since, until)`.
    KabylakeStep { since: Step, until: Step },
}

impl Cond {
    pub fn holds(&self, ctx: &WaContext) -> bool {
        match *self {
            Cond::Always => true,
            Cond::HasLlc => ctx.has_llc,
            Cond::SklKblCflCml => matches!(ctx.platform, Platform::Skylake | Platform::Kabylake),
            Cond::NotCflCml => true,
            Cond::GenAtLeast9 => ctx.graphics_ver >= 9,
            Cond::GenIs9 => ctx.graphics_ver == 9,
            Cond::KabylakeStep { since, until } => ctx.platform == Platform::Kabylake && is_graphics_step(ctx.step, since, until),
        }
    }
}

/// The register a row programs: absolute, or relative to the engine's `mmio_base` (`RING_SEMA_WAIT_POLL(engine->mmio_base)`).
#[derive(Clone, Copy)]
pub enum WaReg {
    Abs(Reg),
    Ring(fn(u32) -> Reg),
}

/// Which `wa_*` helper a row stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaOp {
    /// `wa_masked_en` / `wa_mcr_masked_en`.
    MaskedEn(u32),
    /// `wa_masked_dis` / `wa_mcr_masked_dis`.
    MaskedDis(u32),
    /// `wa_masked_field_set` / `wa_mcr_masked_field_set`.
    MaskedFieldSet { mask: u32, val: u32 },
    /// `wa_write_clr_set` / `wa_mcr_write_clr_set`.
    WriteClrSet { clr: u32, set: u32 },
    /// `wa_write`.
    Write(u32),
    /// `wa_write_or` / `wa_mcr_write_or`.
    WriteOr(u32),
    /// `whitelist_reg_ext(w, reg, flags)` / `whitelist_mcr_reg_ext`.
    Whitelist { flags: u32 },
}

/// One `wa_*` call.
#[derive(Clone, Copy)]
pub struct WaRow {
    /// Linux's workaround name (the comment above the call).
    pub name: &'static str,
    /// `intel_workarounds.c:<line of the call>`.
    pub src: &'static str,
    /// The register macro as Linux spells it (for the line checker and for readers).
    pub linux_reg: &'static str,
    pub reg: WaReg,
    pub mcr: bool,
    pub op: WaOp,
    pub cond: Cond,
}

const fn row(name: &'static str, src: &'static str, linux_reg: &'static str, reg: Reg, mcr: bool, op: WaOp, cond: Cond) -> WaRow {
    WaRow { name, src, linux_reg, reg: WaReg::Abs(reg), mcr, op, cond }
}

use WaOp::*;

/// `gen9_ctx_workarounds_init` (intel_workarounds.c:450-545), Gen9-BC rows in call order.
pub static GEN9_CTX_ROWS: [WaRow; 12] = [
    row("WaCompressedResourceSamplerPbeMediaNewHashMode:skl,kbl", "intel_workarounds.c:461", "COMMON_SLICE_CHICKEN2", r::COMMON_SLICE_CHICKEN2, false, MaskedEn(r::GEN9_PBE_COMPRESSED_HASH_SELECTION), Cond::HasLlc),
    row("WaCompressedResourceSamplerPbeMediaNewHashMode:skl,kbl", "intel_workarounds.c:463", "GEN9_HALF_SLICE_CHICKEN7", r::GEN9_HALF_SLICE_CHICKEN7, true, MaskedEn(r::GEN9_SAMPLER_HASH_COMPRESSED_READ_ADDR), Cond::HasLlc),
    row("WaClearFlowControlGpgpuContextSave + WaDisablePartialInstShootdown:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:469", "GEN8_ROW_CHICKEN", r::GEN8_ROW_CHICKEN, true, MaskedEn(r::FLOW_CONTROL_ENABLE | r::PARTIAL_INSTRUCTION_SHOOTDOWN_DISABLE), Cond::Always),
    row("WaEnableYV12BugFixInHalfSliceChicken7 + WaEnableSamplerGPGPUPreemptionSupport:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:475", "GEN9_HALF_SLICE_CHICKEN7", r::GEN9_HALF_SLICE_CHICKEN7, true, MaskedEn(r::GEN9_ENABLE_YV12_BUGFIX | r::GEN9_ENABLE_GPGPU_PREEMPTION), Cond::Always),
    row("Wa4x4STCOptimizationDisable + WaDisablePartialResolveInVc:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:481", "CACHE_MODE_1", r::CACHE_MODE_1, false, MaskedEn(r::GEN8_4X4_STC_OPTIMIZATION_DISABLE | r::GEN9_PARTIAL_RESOLVE_IN_VC_DISABLE), Cond::Always),
    row("WaCcsTlbPrefetchDisable:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:486", "GEN9_HALF_SLICE_CHICKEN5", r::GEN9_HALF_SLICE_CHICKEN5, true, MaskedDis(r::GEN9_CCS_TLB_PREFETCH_ENABLE), Cond::Always),
    row("WaForceContextSaveRestoreNonCoherent:skl,bxt,kbl,cfl", "intel_workarounds.c:490", "HDC_CHICKEN0", r::HDC_CHICKEN0, false, MaskedEn(r::HDC_FORCE_CONTEXT_SAVE_RESTORE_NON_COHERENT | r::HDC_FORCE_CSR_NON_COHERENT_OVR_DISABLE), Cond::Always),
    row("WaForceEnableNonCoherent:skl,bxt,kbl,cfl", "intel_workarounds.c:508", "HDC_CHICKEN0", r::HDC_CHICKEN0, false, MaskedEn(r::HDC_FORCE_NON_COHERENT), Cond::Always),
    row("WaDisableSamplerPowerBypassForSOPingPong:skl,bxt,kbl,cfl", "intel_workarounds.c:516", "GEN8_HALF_SLICE_CHICKEN3", r::GEN8_HALF_SLICE_CHICKEN3, true, MaskedEn(r::GEN8_SAMPLER_POWER_BYPASS_DIS), Cond::SklKblCflCml),
    row("WaDisableSTUnitPowerOptimization:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:520", "HALF_SLICE_CHICKEN2", r::HALF_SLICE_CHICKEN2, true, MaskedEn(r::GEN8_ST_PO_DISABLE), Cond::Always),
    row("WaDisable3DMidCmdPreemption:skl,bxt,glk,cfl,[cnl]", "intel_workarounds.c:534", "GEN8_CS_CHICKEN1", r::GEN8_CS_CHICKEN1, false, MaskedDis(r::GEN9_PREEMPT_3D_OBJECT_LEVEL), Cond::Always),
    row("WaDisableGPGPUMidCmdPreemption:skl,bxt,blk,cfl,[cnl]", "intel_workarounds.c:537", "GEN8_CS_CHICKEN1", r::GEN8_CS_CHICKEN1, false, MaskedFieldSet { mask: r::GEN9_PREEMPT_GPGPU_LEVEL_MASK, val: r::GEN9_PREEMPT_GPGPU_COMMAND_LEVEL }, Cond::Always),
];

/// `kbl_ctx_workarounds_init` (intel_workarounds.c:607-624), after `gen9_ctx_workarounds_init`.
pub static KBL_CTX_ROWS: [WaRow; 2] = [
    row("WaToEnableHwFixForPushConstHWBug:kbl", "intel_workarounds.c:616", "COMMON_SLICE_CHICKEN2", r::COMMON_SLICE_CHICKEN2, false, MaskedEn(r::GEN8_SBE_DISABLE_REPLAY_BUF_OPTIMIZATION), Cond::KabylakeStep { since: Step::C0, until: Step::FOREVER }),
    row("WaDisableSbeCacheDispatchPortSharing:kbl", "intel_workarounds.c:620", "GEN8_HALF_SLICE_CHICKEN1", r::GEN8_HALF_SLICE_CHICKEN1, true, MaskedEn(r::GEN7_SBE_SS_CACHE_DISPATCH_PORT_SHARING_DISABLE), Cond::Always),
];

/// `gen9_gt_workarounds_init` (intel_workarounds.c:1178-1206) after its `gen9_wa_init_mcr` call (:1183, built by
/// [`crate::sseu::gen9_mcr_steer`]), Gen9-BC rows in call order.
pub static GEN9_GT_ROWS: [WaRow; 3] = [
    row("WaDisableKillLogic:bxt,skl,kbl", "intel_workarounds.c:1187", "GAM_ECOCHK", r::GAM_ECOCHK, false, WriteOr(r::ECOCHK_DIS_TLB), Cond::NotCflCml),
    row("WaCompressedResourceSamplerPbeMediaNewHashMode:skl,kbl", "intel_workarounds.c:1197", "MMCD_MISC_CTRL", r::MMCD_MISC_CTRL, false, WriteOr(r::MMCD_PCLA | r::MMCD_HOTSPOT_EN), Cond::HasLlc),
    row("WaDisableHDCInvalidation:skl,bxt,kbl,cfl", "intel_workarounds.c:1203", "GAM_ECOCHK", r::GAM_ECOCHK, false, WriteOr(r::BDW_DISABLE_HDC_INVALIDATION), Cond::Always),
];

/// `kbl_gt_workarounds_init` (intel_workarounds.c:1226-1247), after `gen9_gt_workarounds_init`.
pub static KBL_GT_ROWS: [WaRow; 3] = [
    row("WaDisableDynamicCreditSharing:kbl", "intel_workarounds.c:1232", "GAMT_CHKN_BIT_REG", r::GAMT_CHKN_BIT_REG, false, WriteOr(r::GAMT_CHKN_DISABLE_DYNAMIC_CREDIT_SHARING), Cond::KabylakeStep { since: Step::NONE, until: Step::C0 }),
    row("WaDisableGafsUnitClkGating:kbl", "intel_workarounds.c:1237", "GEN7_UCGCTL4", r::GEN7_UCGCTL4, false, WriteOr(r::GEN8_EU_GAUNIT_CLOCK_GATE_DISABLE), Cond::Always),
    row("WaInPlaceDecompressionHang:kbl", "intel_workarounds.c:1242", "GEN9_GAMT_ECO_REG_RW_IA", r::GEN9_GAMT_ECO_REG_RW_IA, false, WriteOr(r::GAMT_ECO_ENABLE_IN_PLACE_DECOMPRESS), Cond::Always),
];

/// The Gen9 branches of `rcs_engine_wa_init` (intel_workarounds.c:2452-2505), RCS only, in call order.
pub static RCS_ENGINE_ROWS: [WaRow; 8] = [
    row("FtrPerCtxtPreemptionGranularityControl (Option 2, comment :2396-2451)", "intel_workarounds.c:2453", "GEN7_FF_SLICE_CS_CHICKEN1", r::GEN7_FF_SLICE_CS_CHICKEN1, false, MaskedEn(r::GEN9_FFSC_PERCTX_PREEMPT_CTRL), Cond::GenAtLeast9),
    row("WaEnableGapsTsvCreditFix:skl,kbl,cfl", "intel_workarounds.c:2462", "GEN8_GARBCNTL", r::GEN8_GARBCNTL, false, WriteOr(r::GEN9_GAPS_TSV_CREDIT_DISABLE), Cond::SklKblCflCml),
    row("WaContextSwitchWithConcurrentTLBInvalidate:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:2476", "GEN9_CSFE_CHICKEN1_RCS", r::GEN9_CSFE_CHICKEN1_RCS, false, MaskedEn(r::GEN9_PREEMPT_GPGPU_SYNC_SWITCH_DISABLE), Cond::GenIs9),
    row("WaEnableLbsSlaRetryTimerDecrement:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:2481", "BDW_SCRATCH1", r::BDW_SCRATCH1, true, WriteOr(r::GEN9_LBS_SLA_RETRY_TIMER_DECREMENT_ENABLE), Cond::GenIs9),
    row("WaOCLCoherentLineFlush:skl,bxt,kbl,cfl", "intel_workarounds.c:2494", "GEN8_L3SQCREG4", r::GEN8_L3SQCREG4, true, WriteOr(r::GEN8_LQSC_FLUSH_COHERENT_LINES), Cond::GenIs9),
    row("Disable atomics in L3 to prevent unrecoverable hangs", "intel_workarounds.c:2499", "GEN9_SCRATCH_LNCF1", r::GEN9_SCRATCH_LNCF1, false, WriteClrSet { clr: r::GEN9_LNCF_NONIA_COHERENT_ATOMICS_ENABLE, set: 0 }, Cond::GenIs9),
    row("Disable atomics in L3 to prevent unrecoverable hangs", "intel_workarounds.c:2501", "GEN8_L3SQCREG4", r::GEN8_L3SQCREG4, true, WriteClrSet { clr: r::GEN8_LQSQ_NONIA_COHERENT_ATOMICS_ENABLE, set: 0 }, Cond::GenIs9),
    row("Disable atomics in L3 to prevent unrecoverable hangs", "intel_workarounds.c:2503", "GEN9_SCRATCH1", r::GEN9_SCRATCH1, true, WriteClrSet { clr: r::EVICTION_PERF_FIX_ENABLE, set: 0 }, Cond::GenIs9),
];

/// `xcs_engine_wa_init` (intel_workarounds.c:2686-2695), the Kaby Lake row: every non-render, non-compute engine.
pub static XCS_ENGINE_ROWS: [WaRow; 1] = [WaRow {
    name: "WaKBLVECSSemaphoreWaitPoll:kbl",
    src: "intel_workarounds.c:2692",
    linux_reg: "RING_SEMA_WAIT_POLL",
    reg: WaReg::Ring(r::ring_sema_wait_poll),
    mcr: false,
    op: Write(1),
    cond: Cond::KabylakeStep { since: Step::A0, until: Step::F0 },
}];

/// `gen9_whitelist_build` (intel_workarounds.c:1907-1920) then `kbl_whitelist_build` (:1943-1954): RCS only; every entry is
/// `RING_FORCE_TO_NONPRIV_ACCESS_RW` (= 0).
pub static KBL_WHITELIST_ROWS: [WaRow; 5] = [
    row("WaVFEStateAfterPipeControlwithMediaStateClear:skl,bxt,glk,cfl", "intel_workarounds.c:1910", "GEN9_CTX_PREEMPT_REG", r::GEN9_CTX_PREEMPT_REG, false, Whitelist { flags: r::RING_FORCE_TO_NONPRIV_ACCESS_RW }, Cond::Always),
    row("WaEnablePreemptionGranularityControlByUMD:skl,bxt,kbl,cfl,[cnl]", "intel_workarounds.c:1913", "GEN8_CS_CHICKEN1", r::GEN8_CS_CHICKEN1, false, Whitelist { flags: r::RING_FORCE_TO_NONPRIV_ACCESS_RW }, Cond::Always),
    row("WaAllowUMDToModifyHDCChicken1:skl,bxt,kbl,glk,cfl", "intel_workarounds.c:1916", "GEN8_HDC_CHICKEN1", r::GEN8_HDC_CHICKEN1, false, Whitelist { flags: r::RING_FORCE_TO_NONPRIV_ACCESS_RW }, Cond::Always),
    row("WaSendPushConstantsFromMMIO:skl,bxt", "intel_workarounds.c:1919", "COMMON_SLICE_CHICKEN2", r::COMMON_SLICE_CHICKEN2, false, Whitelist { flags: r::RING_FORCE_TO_NONPRIV_ACCESS_RW }, Cond::Always),
    row("WaDisableLSQCROPERFforOCL:kbl", "intel_workarounds.c:1953", "GEN8_L3SQCREG4", r::GEN8_L3SQCREG4, true, Whitelist { flags: r::RING_FORCE_TO_NONPRIV_ACCESS_RW }, Cond::Always),
];

/// `is_nonpriv_flags_valid` (intel_workarounds.c:1846-1859).
pub const fn nonpriv_flags_valid(flags: u32) -> bool {
    if flags & !r::RING_FORCE_TO_NONPRIV_MASK_VALID != 0 {
        return false;
    }
    flags & r::RING_FORCE_TO_NONPRIV_ACCESS_MASK != r::RING_FORCE_TO_NONPRIV_ACCESS_INVALID
}

/// Apply `rows` whose condition holds, in order, to `list`. `mmio_base` resolves [`WaReg::Ring`] rows.
pub fn apply_rows<const N: usize>(list: &mut WaList<N>, rows: &[WaRow], ctx: &WaContext, mmio_base: u32) -> Result<(), GtError> {
    for row in rows {
        if !row.cond.holds(ctx) {
            continue;
        }
        let reg = match row.reg {
            WaReg::Abs(r) => r,
            WaReg::Ring(f) => f(mmio_base),
        };
        match row.op {
            MaskedEn(v) => list.masked_en(reg, row.mcr, v)?,
            MaskedDis(v) => list.masked_dis(reg, row.mcr, v)?,
            MaskedFieldSet { mask, val } => list.masked_field_set(reg, row.mcr, mask, val)?,
            WriteClrSet { clr, set } => list.write_clr_set(reg, row.mcr, clr, set)?,
            Write(v) => list.write(reg, row.mcr, v)?,
            WriteOr(v) => list.write_or(reg, row.mcr, v)?,
            Whitelist { flags } => whitelist_reg_ext(list, reg, row.mcr, flags)?,
        }
    }
    Ok(())
}

/// `whitelist_reg_ext` / `whitelist_mcr_reg_ext` (intel_workarounds.c:1861-1893): refuse past `RING_MAX_NONPRIV_SLOTS`, refuse
/// invalid flags, then `_wa_add` the register with the flags OR'd into its offset.
pub fn whitelist_reg_ext<const N: usize>(list: &mut WaList<N>, reg: Reg, mcr: bool, flags: u32) -> Result<(), GtError> {
    if list.count() >= r::RING_MAX_NONPRIV_SLOTS as usize {
        return Err(GtError::NonprivSlotsFull { limit: r::RING_MAX_NONPRIV_SLOTS });
    }
    if !nonpriv_flags_valid(flags) {
        return Err(GtError::NonprivFlagsInvalid { flags });
    }
    list.add(Wa { reg: Reg::new(reg.offset() | flags), clr: 0, set: 0, read: 0, masked: false, mcr })
}

/// `kbl_ctx_workarounds_init`'s list (`engine->ctx_wa_list`, "context"): RCS only.
pub fn kbl_ctx_workarounds(ctx: &WaContext, engine: &EngineInfo) -> Result<WaList<16>, GtError> {
    let mut wal = WaList::<16>::new("context", engine.name);
    if engine.class != EngineClass::Render {
        return Ok(wal); // intel_workarounds.c:954: `if (engine->class != RENDER_CLASS) goto done`
    }
    apply_rows(&mut wal, &GEN9_CTX_ROWS, ctx, engine.mmio_base)?;
    apply_rows(&mut wal, &KBL_CTX_ROWS, ctx, engine.mmio_base)?;
    Ok(wal)
}

/// `kbl_gt_workarounds_init`'s list (`gt->wa_list`, "GT"): the MCR steering write first (`gen9_wa_init_mcr`, :1183, from the
/// fuse-decoded topology `steer`), then the Gen9 rows, then the KBL rows.
pub fn kbl_gt_workarounds(ctx: &WaContext, steer: &crate::sseu::McrSteer) -> Result<WaList<16>, GtError> {
    let mut wal = WaList::<16>::new("GT", "global");
    // wa_write_clr_set(wal, GEN8_MCR_SELECTOR, mcr_mask, mcr) - intel_workarounds.c:1175
    wal.write_clr_set(r::GEN8_MCR_SELECTOR, false, steer.mask, steer.value)?;
    apply_rows(&mut wal, &GEN9_GT_ROWS, ctx, 0)?;
    apply_rows(&mut wal, &KBL_GT_ROWS, ctx, 0)?;
    Ok(wal)
}

/// `engine_init_workarounds` for one Kaby Lake engine (`engine->wa_list`, "engine", intel_workarounds.c:2887-2907): the RCS
/// list for the render engine, the `xcs` list for every other class. (`general_render_compute_wa_init` adds nothing before Gen11.)
pub fn kbl_engine_workarounds(ctx: &WaContext, engine: &EngineInfo) -> Result<WaList<16>, GtError> {
    let mut wal = WaList::<16>::new("engine", engine.name);
    match engine.class {
        EngineClass::Render => apply_rows(&mut wal, &RCS_ENGINE_ROWS, ctx, engine.mmio_base)?,
        _ => apply_rows(&mut wal, &XCS_ENGINE_ROWS, ctx, engine.mmio_base)?,
    }
    Ok(wal)
}

/// `intel_engine_init_whitelist` for Kaby Lake (`engine->whitelist`, "whitelist"): RCS only; the other engines get an empty list.
pub fn kbl_whitelist(ctx: &WaContext, engine: &EngineInfo) -> Result<WaList<16>, GtError> {
    let mut wal = WaList::<16>::new("whitelist", engine.name);
    if engine.class == EngineClass::Render {
        apply_rows(&mut wal, &KBL_WHITELIST_ROWS, ctx, engine.mmio_base)?;
    }
    Ok(wal)
}
