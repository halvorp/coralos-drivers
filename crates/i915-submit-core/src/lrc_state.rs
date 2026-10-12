// SPDX-License-Identifier: GPL-2.0-only
//! The logical ring context's register state page and descriptor: `__lrc_init_regs`, `init_common_regs`, `init_ppgtt_regs`,
//! `init_wa_bb_regs`, `__reset_stop_ring`, `lrc_update_regs`, `lrc_descriptor`, `lrc_setup_indirect_ctx`, `lrc_init_state`
//! (gt/intel_lrc.c:772-1042, :1521-1574), Gen8/Gen9 and 4-level (48-bit) PPGTT only. The context image is
//! `EngineInfo::context_size` bytes (22 pages for a Gen9 render engine, 2 for the others): page 0 is the per-process HWSP, page 1
//! (this module's [`RegState`]) holds the registers the hardware reloads at a context switch, the rest is the engine's
//! default state which the caller copies in from the golden context.
use crate::error::SubmitError;
use crate::lrc_offsets::{self, lrc_ring_bb_offset, lrc_ring_indirect_offset, lrc_ring_indirect_ptr, lrc_ring_mi_mode, lrc_ring_wa_bb_per_ctx};
use crate::lrc_reg::*;
use crate::ring::Ring;
use crate::wa_bb::WaCtx;
use i915_common_core::reg::{masked_bit_disable, masked_bit_enable};
use i915_gt_regs_core as r;

/// Dwords in the state page.
pub const PAGE_DWORDS: usize = 1024;

/// `ce->lrc_reg_state`: the second page of the context image as dwords.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegState {
    dw: [u32; PAGE_DWORDS],
}

impl RegState {
    pub const fn zeroed() -> RegState {
        RegState { dw: [0; PAGE_DWORDS] }
    }
    /// Start from a copy of the default state's page 1 (`shmem_read(ce->default_state, ...)`, intel_lrc.c:1022-1026).
    pub fn from_default_state(page: &[u32; PAGE_DWORDS]) -> RegState {
        RegState { dw: *page }
    }
    pub fn as_slice(&self) -> &[u32] {
        &self.dw
    }
    pub fn get(&self, idx: usize) -> u32 {
        self.dw[idx]
    }
    /// Store one dword of the image (the executor mirrors this into the context object). Out-of-page is a named refusal.
    pub fn set(&mut self, idx: usize, v: u32) -> Result<(), SubmitError> {
        match self.dw.get_mut(idx) {
            Some(d) => {
                *d = v;
                Ok(())
            }
            None => Err(SubmitError::LrcOverrun { index: idx as u32, limit: PAGE_DWORDS as u32 }),
        }
    }
}

/// What the register state needs to know about its engine, address space and ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CtxParams {
    /// `GRAPHICS_VER`: 9 (Kaby Lake) or 8.
    pub gen: u8,
    /// `engine->class == RENDER_CLASS` (also selects the RCS table, `I915_ENGINE_HAS_RCS_REG_STATE`).
    pub render: bool,
    /// `engine->mmio_base`.
    pub mmio_base: u32,
    /// The PML4's device address (`px_dma(ppgtt->pd)`), from `i915_memory_core::PpgttInit::pml4`.
    pub pml4: u64,
    /// `i915_ggtt_offset(ring->vma)`.
    pub ring_ggtt: u32,
    /// `intel_sseu_make_rpcs(...)` for a render engine; ignored otherwise.
    pub rpcs: u32,
    /// `ce->stats.runtime.last`, the value of `CTX_TIMESTAMP` at the first restore.
    pub runtime_last: u32,
    /// `engine->wa_ctx` with `i915_ggtt_offset(wa_ctx->vma)`, when the engine has one.
    pub wa_ctx: Option<(u32, WaCtx)>,
}

fn require_gen(gen: u8) -> Result<(), SubmitError> {
    if gen == 8 || gen == 9 {
        Ok(())
    } else {
        Err(SubmitError::UnsupportedGeneration { gen })
    }
}

fn slot(regs: &mut RegState, idx: usize, v: u32) -> Result<(), SubmitError> {
    regs.set(idx, v)
}

fn indirect_ctx_offset_default(gen: u8) -> u32 {
    if gen >= 9 {
        GEN9_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT // intel_lrc.c:772-787
    } else {
        GEN8_CTX_RCS_INDIRECT_CTX_OFFSET_DEFAULT // intel_lrc.c:772-787
    }
}

/// `lrc_setup_indirect_ctx` (intel_lrc.c:802-816).
fn setup_indirect_ctx(regs: &mut RegState, p: &CtxParams, addr: u32, size: u32) -> Result<(), SubmitError> {
    if size == 0 || size % CACHELINE_BYTES != 0 {
        return Err(SubmitError::LrcTableMalformed { table: "indirect ctx size", at: size });
    }
    if addr % CACHELINE_BYTES != 0 {
        return Err(SubmitError::GgttUnaligned { what: "indirect ctx batch", addr });
    }
    let ptr = lrc_ring_indirect_ptr(p.gen, p.render).ok_or(SubmitError::UnsupportedGeneration { gen: p.gen })?;
    let off = lrc_ring_indirect_offset(p.gen, p.render).ok_or(SubmitError::UnsupportedGeneration { gen: p.gen })?;
    slot(regs, ptr + 1, addr | (size / CACHELINE_BYTES))?; // regs[lrc_ring_indirect_ptr + 1]
    slot(regs, off + 1, indirect_ctx_offset_default(p.gen) << 6)?; // regs[lrc_ring_indirect_offset + 1]
    Ok(())
}

/// `init_common_regs` (intel_lrc.c:841-866).
fn init_common_regs(regs: &mut RegState, p: &CtxParams, inhibit: bool) -> Result<(), SubmitError> {
    let mut ctl = masked_bit_enable(r::CTX_CTRL_INHIBIT_SYN_CTX_SWITCH);
    ctl |= masked_bit_disable(r::CTX_CTRL_ENGINE_CTX_RESTORE_INHIBIT);
    if inhibit {
        ctl |= r::CTX_CTRL_ENGINE_CTX_RESTORE_INHIBIT;
    }
    if p.gen < 11 {
        ctl |= masked_bit_disable(r::CTX_CTRL_ENGINE_CTX_SAVE_INHIBIT | r::CTX_CTRL_RS_CTX_ENABLE);
    }
    slot(regs, CTX_CONTEXT_CONTROL, ctl)?;
    slot(regs, CTX_TIMESTAMP, p.runtime_last)?;
    if let Some(loc) = lrc_ring_bb_offset(p.gen, p.render) {
        slot(regs, loc + 1, 0)?;
    }
    Ok(())
}

/// `init_wa_bb_regs` (intel_lrc.c:868-887).
fn init_wa_bb_regs(regs: &mut RegState, p: &CtxParams) -> Result<(), SubmitError> {
    if let Some((ggtt, wa)) = p.wa_ctx {
        if wa.per_ctx.size != 0 {
            let loc = lrc_ring_wa_bb_per_ctx(p.gen, p.render).ok_or(SubmitError::UnsupportedGeneration { gen: p.gen })?;
            slot(regs, loc + 1, (ggtt + wa.per_ctx.offset) | 0x01)?;
        }
        if wa.indirect_ctx.size != 0 {
            setup_indirect_ctx(regs, p, ggtt + wa.indirect_ctx.offset, wa.indirect_ctx.size)?;
        }
    }
    Ok(())
}

/// `init_ppgtt_regs` for a 4-level address space (intel_lrc.c:889-903): PDP0 carries the PML4's address.
fn init_ppgtt_regs(regs: &mut RegState, pml4: u64) -> Result<(), SubmitError> {
    slot(regs, CTX_PDP0_UDW, (pml4 >> 32) as u32)?;
    slot(regs, CTX_PDP0_LDW, pml4 as u32)
}

/// `__reset_stop_ring` (intel_lrc.c:913-922): clear `STOP_RING` in the saved MI_MODE (masked write: the high half selects it).
fn reset_stop_ring(regs: &mut RegState, p: &CtxParams) -> Result<(), SubmitError> {
    if let Some(x) = lrc_ring_mi_mode(p.gen, p.render) {
        let v = (regs.get(x + 1) & !r::STOP_RING) | (r::STOP_RING << 16);
        slot(regs, x + 1, v)?;
    }
    Ok(())
}

/// `__lrc_init_regs` (intel_lrc.c:924-951). `inhibit` is set for a context without a default state: the page is zeroed, the
/// offsets table is closed with `MI_BATCH_BUFFER_END`, and the first restore is inhibited.
pub fn init_regs(regs: &mut RegState, p: &CtxParams, inhibit: bool) -> Result<(), SubmitError> {
    require_gen(p.gen)?;
    if inhibit {
        *regs = RegState::zeroed(); // memset(regs, 0, PAGE_SIZE)
    }
    let table = lrc_offsets::reg_offsets(p.gen, p.render)?;
    lrc_offsets::set_offsets(&mut regs.dw, table.items, table.name, p.mmio_base, p.gen, inhibit)?;
    init_common_regs(regs, p, inhibit)?;
    init_ppgtt_regs(regs, p.pml4)?;
    init_wa_bb_regs(regs, p)?;
    reset_stop_ring(regs, p)
}

/// `lrc_descriptor` (intel_lrc.c:1521-1535) for a 4-level context: the low dword of the ELSP descriptor.
/// `lrca` is `i915_ggtt_offset(ce->state)` and must be page aligned.
pub fn descriptor(gen: u8, lrca: u32) -> Result<u32, SubmitError> {
    require_gen(gen)?;
    if lrca & !CTX_GTT_ADDRESS_MASK != 0 {
        return Err(SubmitError::LrcaUnaligned { lrca });
    }
    let mut desc = INTEL_LEGACY_64B_CONTEXT << GEN8_CTX_ADDRESSING_MODE_SHIFT;
    desc |= GEN8_CTX_VALID | GEN8_CTX_PRIVILEGE;
    if gen == 8 {
        desc |= GEN8_CTX_L3LLC_COHERENT;
    }
    Ok(lrca | desc as u32)
}

/// `lrc_update_regs` (intel_lrc.c:1537-1574): point the image at the ring and return the descriptor with
/// `CTX_DESC_FORCE_RESTORE` set (the first submission after a (re)pin must reload the whole context).
pub fn update_regs(regs: &mut RegState, p: &CtxParams, lrca: u32, ring: &Ring, head: u32) -> Result<u32, SubmitError> {
    require_gen(p.gen)?;
    if !ring.offset_valid(head) {
        return Err(SubmitError::RingOffsetInvalid { pos: head, size: ring.size() }); // GEM_BUG_ON(!intel_ring_offset_valid(ring, head))
    }
    if !ring.offset_valid(ring.tail()) {
        return Err(SubmitError::RingOffsetInvalid { pos: ring.tail(), size: ring.size() });
    }
    if p.ring_ggtt & 0xfff != 0 {
        return Err(SubmitError::GgttUnaligned { what: "ring", addr: p.ring_ggtt });
    }
    let desc = descriptor(p.gen, lrca)?;
    slot(regs, CTX_RING_START, p.ring_ggtt)?;
    slot(regs, CTX_RING_HEAD, head)?;
    slot(regs, CTX_RING_TAIL, ring.tail())?;
    slot(regs, CTX_RING_CTL, ring.ctl_value())?;
    if p.render {
        slot(regs, CTX_R_PWR_CLK_STATE, p.rpcs)?;
    }
    Ok(desc | CTX_DESC_FORCE_RESTORE as u32)
}
