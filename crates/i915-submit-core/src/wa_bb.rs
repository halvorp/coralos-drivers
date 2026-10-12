// SPDX-License-Identifier: GPL-2.0-only
//! The workaround batch buffers executed at every context switch: `gen8_init_indirectctx_bb` and `gen9_init_indirectctx_bb`
//! with `gen8_emit_flush_coherentl3_wa` and `emit_lri` (gt/intel_lrc.c:1638-1803), and the page layout of `lrc_init_wa_ctx`
//! (:1838-1930). There is no per-context batch on Gen8/Gen9 (`wa_bb_fn[1] = NULL`, :1856, :1859), so `per_ctx.size` stays 0.
//!
//! The batch is built into the caller's page-sized buffer; the caller writes it to the wa object (one page, GGTT-pinned) and
//! hands `ggtt_offset + indirect_ctx.offset` to the context image ([`crate::lrc_state`]).
use crate::error::SubmitError;
use crate::lrc_reg::{CACHELINE_BYTES, LRC_PPHWSP_SCRATCH};
use i915_common_core::reg::masked_field;
use i915_gpu_commands_core as c;
use i915_gt_core::emit::{emit_pipe_control, CsBuf};
use i915_gt_regs_core as r;

/// `CTX_WA_BB_SIZE` (intel_lrc.c:1805): one page.
pub const CTX_WA_BB_SIZE: u32 = 4096; // intel_lrc.c:1805
/// `INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA` (intel_gt_types.h:319): the byte offset of the Coherent-L3 workaround dword in the
/// GT's scratch object. "No one else is allowed to scribble over scratch + 256!" (intel_lrc.c:1641).
pub const INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA: u32 = 256; // intel_gt_types.h:319

/// `struct i915_wa_ctx_bb` (intel_engine_types.h): where one batch sits in the wa object, in BYTES.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaBb {
    pub offset: u32,
    pub size: u32,
}

/// `struct i915_ctx_workarounds`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WaCtx {
    pub indirect_ctx: WaBb,
    pub per_ctx: WaBb,
}

/// The platform facts the batch consults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaBbParams {
    pub gen: u8,
    /// `IS_BROADWELL` (the Gen8 batch adds the coherent-L3 flush for BDW only, intel_lrc.c:1687).
    pub is_broadwell: bool,
    /// `HAS_POOLED_EU` (BXT/GLK; false on Kaby Lake, intel_lrc.c:1774).
    pub has_pooled_eu: bool,
    /// `i915_ggtt_offset(gt->scratch)`.
    pub scratch_ggtt: u32,
}

fn coherentl3_scratch(p: &WaBbParams) -> Result<u32, SubmitError> {
    p.scratch_ggtt.checked_add(INTEL_GT_SCRATCH_FIELD_COHERENTL3_WA).ok_or(SubmitError::ScratchOverflow { scratch: p.scratch_ggtt })
}

/// `gen8_emit_flush_coherentl3_wa` (intel_lrc.c:1638-1668): store the L3SQCREG4 value, force the flush-coherent-lines bit, a
/// CS-stall + DC-flush pipe control, then load the register back. 4 + 3 + 6 + 4 = 17 dwords.
fn emit_flush_coherentl3_wa(cs: &mut CsBuf, p: &WaBbParams) -> Result<(), SubmitError> {
    let scratch = coherentl3_scratch(p)?;
    let l3sqcreg4 = r::GEN8_L3SQCREG4.offset();
    cs.push(c::MI_STORE_REGISTER_MEM_GEN8 | c::MI_SRM_LRM_GLOBAL_GTT)?;
    cs.push(l3sqcreg4)?;
    cs.push(scratch)?;
    cs.push(0)?;
    cs.push(c::mi_load_register_imm(1))?;
    cs.push(l3sqcreg4)?;
    cs.push(0x4040_0000 | r::GEN8_LQSC_FLUSH_COHERENT_LINES)?;
    emit_pipe_control(cs, 0, c::PIPE_CONTROL_CS_STALL | c::PIPE_CONTROL_DC_FLUSH_ENABLE, 0)?;
    cs.push(c::MI_LOAD_REGISTER_MEM_GEN8 | c::MI_SRM_LRM_GLOBAL_GTT)?;
    cs.push(l3sqcreg4)?;
    cs.push(scratch)?;
    cs.push(0)?;
    Ok(())
}

/// `while ((unsigned long)batch % CACHELINE_BYTES) *batch++ = MI_NOOP;` (intel_lrc.c:1701-1703, :1798-1800).
fn pad_to_cacheline(cs: &mut CsBuf) -> Result<(), SubmitError> {
    while (cs.len() as u32 * 4) % CACHELINE_BYTES != 0 {
        cs.push(c::MI_NOOP)?;
    }
    Ok(())
}

/// `WaClearSlmSpaceAtContextSwitch`: the L3 flush + store-data-index qword write to the PPHWSP scratch dword
/// (intel_lrc.c:1690-1697, :1763-1769).
fn emit_clear_slm(cs: &mut CsBuf) -> Result<(), SubmitError> {
    emit_pipe_control(
        cs,
        0,
        c::PIPE_CONTROL_FLUSH_L3 | c::PIPE_CONTROL_STORE_DATA_INDEX | c::PIPE_CONTROL_CS_STALL | c::PIPE_CONTROL_QW_WRITE,
        LRC_PPHWSP_SCRATCH * 4,
    )?;
    Ok(())
}

/// `gen8_init_indirectctx_bb` (intel_lrc.c:1681-1717).
fn gen8_init_indirectctx_bb(cs: &mut CsBuf, p: &WaBbParams) -> Result<(), SubmitError> {
    cs.push(c::MI_ARB_ON_OFF | c::MI_ARB_DISABLE)?; // WaDisableCtxRestoreArbitration:bdw,chv
    if p.is_broadwell {
        emit_flush_coherentl3_wa(cs, p)?; // WaFlushCoherentL3CacheLinesAtContextSwitch:bdw
    }
    emit_clear_slm(cs)?; // WaClearSlmSpaceAtContextSwitch:bdw,chv
    cs.push(c::MI_ARB_ON_OFF | c::MI_ARB_ENABLE)?;
    pad_to_cacheline(cs)
}

/// `emit_lri` (intel_lrc.c:1719-1731): `MI_LOAD_REGISTER_IMM(count)`, `count` (reg, value) pairs, `MI_NOOP`.
fn emit_lri(cs: &mut CsBuf, lri: &[(u32, u32)]) -> Result<(), SubmitError> {
    cs.push(c::mi_load_register_imm(lri.len() as u32))?;
    for (reg, value) in lri {
        cs.push(*reg)?;
        cs.push(*value)?;
    }
    cs.push(c::MI_NOOP)?;
    Ok(())
}

/// `gen9_init_indirectctx_bb` (intel_lrc.c:1733-1803). The coherent-L3 flush is emitted for every Gen9 part (the Linux
/// comment names skl,bxt,glk but the call is unconditional, :1761).
fn gen9_init_indirectctx_bb(cs: &mut CsBuf, p: &WaBbParams) -> Result<(), SubmitError> {
    let lri: [(u32, u32); 3] = [
        // WaDisableGatherAtSetShaderCommonSlice:skl,bxt,kbl,glk
        (r::COMMON_SLICE_CHICKEN2.offset(), masked_field(r::GEN9_DISABLE_GATHER_AT_SET_SHADER_COMMON_SLICE, 0)),
        // BSpec: 11391
        (r::FF_SLICE_CHICKEN.offset(), masked_field(r::FF_SLICE_CHICKEN_CL_PROVOKING_VERTEX_FIX, r::FF_SLICE_CHICKEN_CL_PROVOKING_VERTEX_FIX)),
        // BSpec: 11299
        (r::_3D_CHICKEN3.offset(), masked_field(r::_3D_CHICKEN_SF_PROVOKING_VERTEX_FIX, r::_3D_CHICKEN_SF_PROVOKING_VERTEX_FIX)),
    ];
    cs.push(c::MI_ARB_ON_OFF | c::MI_ARB_DISABLE)?;
    emit_flush_coherentl3_wa(cs, p)?; // WaFlushCoherentL3CacheLinesAtContextSwitch:skl,bxt,glk
    emit_clear_slm(cs)?; // WaClearSlmSpaceAtContextSwitch:skl,bxt,kbl,glk,cfl
    emit_lri(cs, &lri)?;
    if p.has_pooled_eu {
        // WaMediaPoolStateCmdInWABB:bxt,glk (intel_lrc.c:1774-1795)
        for dw in [c::GEN9_MEDIA_POOL_STATE, c::GEN9_MEDIA_POOL_ENABLE, 0x0077_7000, 0, 0, 0] {
            cs.push(dw)?;
        }
    }
    cs.push(c::MI_ARB_ON_OFF | c::MI_ARB_ENABLE)?;
    pad_to_cacheline(cs)
}

/// `lrc_init_wa_ctx` (intel_lrc.c:1838-1930) for a render engine: emit the indirect-context batch at offset 0 of `page`
/// (CTX_WA_BB_SIZE bytes) and record offset/size in bytes. The per-context slot follows it with size 0. Gen11+ and non-render
/// engines get no batch (:1850-1852), which the caller expresses by not calling this.
pub fn build_wa_ctx(p: &WaBbParams, page: &mut [u32]) -> Result<WaCtx, SubmitError> {
    if page.len() as u32 * 4 < CTX_WA_BB_SIZE {
        return Err(SubmitError::Gt(i915_gt_core::GtError::CsOverflow { need: CTX_WA_BB_SIZE / 4, have: page.len() as u32 }));
    }
    let limit = (CTX_WA_BB_SIZE / 4) as usize;
    let mut cs = CsBuf::new(&mut page[..limit]);
    match p.gen {
        9 => gen9_init_indirectctx_bb(&mut cs, p)?,
        8 => gen8_init_indirectctx_bb(&mut cs, p)?,
        gen => return Err(SubmitError::UnsupportedGeneration { gen }),
    }
    let indirect = WaBb { offset: 0, size: cs.len() as u32 * 4 };
    Ok(WaCtx { indirect_ctx: indirect, per_ctx: WaBb { offset: indirect.size, size: 0 } })
}
