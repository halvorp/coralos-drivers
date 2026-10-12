// SPDX-License-Identifier: GPL-2.0-only
//! Command emitters as pure dword builders: the Gen8/Gen9 ring commands of gt/gen8_engine_cs.c and gen8_engine_cs.h. Each
//! emitter first RESERVES its documented length in the caller's buffer (Linux's `intel_ring_begin(rq, len)`) and refuses with
//! [`GtError::CsOverflow`] when the room is not there, so a refused emission writes nothing. The buffer is the caller's ring
//! window; offsets the caller needs (`rq->tail`, `rq->wa_tail`) come back as DWORD counts from the start of the emission.
use crate::error::GtError;
use crate::wa_tables::WaContext;
use crate::step::{is_graphics_step, Step};
use i915_devinfo_core::Platform;
use i915_gpu_commands_core as c;

/// `EMIT_INVALIDATE` / `EMIT_FLUSH` / `EMIT_BARRIER` (gt/intel_engine_types.h:534-536).
pub const EMIT_INVALIDATE: u32 = 1 << 0; // intel_engine_types.h:534
pub const EMIT_FLUSH: u32 = 1 << 1; // intel_engine_types.h:535
pub const EMIT_BARRIER: u32 = EMIT_INVALIDATE | EMIT_FLUSH; // intel_engine_types.h:536
/// `I915_DISPATCH_SECURE` (intel_engine_types.h:540).
pub const I915_DISPATCH_SECURE: u32 = 1 << 0; // intel_engine_types.h:540

/// `LRC_PPHWSP_SCRATCH_ADDR` = `0x34 * sizeof(u32)` (gt/intel_lrc.h:30-31): the scratch dword in the per-process HWSP.
pub const LRC_PPHWSP_SCRATCH_ADDR: u32 = 0x34 * 4; // intel_lrc.h:31
/// `I915_GEM_HWS_PREEMPT_ADDR` = `0x32 * sizeof(u32)` (gt/intel_engine.h:188-189): the preempt semaphore in the engine HWSP.
pub const I915_GEM_HWS_PREEMPT_ADDR: u32 = 0x32 * 4; // intel_engine.h:189

/// A bounded dword sink over the caller's ring window.
pub struct CsBuf<'a> {
    buf: &'a mut [u32],
    len: usize,
}

impl<'a> CsBuf<'a> {
    pub fn new(buf: &'a mut [u32]) -> CsBuf<'a> {
        CsBuf { buf, len: 0 }
    }
    /// Dwords written so far.
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn written(&self) -> &[u32] {
        &self.buf[..self.len]
    }
    /// `intel_ring_begin(rq, n)`: refuse unless `n` more dwords fit.
    pub fn reserve(&self, n: usize) -> Result<(), GtError> {
        let have = self.buf.len() - self.len;
        if n > have {
            return Err(GtError::CsOverflow { need: n as u32, have: have as u32 });
        }
        Ok(())
    }
    /// Push one dword. Callers `reserve` first; a push past the end is still a named refusal, never a panic.
    pub fn push(&mut self, v: u32) -> Result<(), GtError> {
        self.reserve(1)?;
        self.buf[self.len] = v;
        self.len += 1;
        Ok(())
    }
    fn extend(&mut self, vs: &[u32]) -> Result<(), GtError> {
        for v in vs {
            self.push(*v)?;
        }
        Ok(())
    }
}

/// `__gen8_emit_pipe_control` (gen8_engine_cs.h:52-62): six dwords `{ GFX_OP_PIPE_CONTROL(6) | bg0, bg1, offset, 0, 0, 0 }`.
pub fn emit_pipe_control(cs: &mut CsBuf, bit_group_0: u32, bit_group_1: u32, offset: u32) -> Result<(), GtError> {
    cs.reserve(6)?;
    cs.extend(&[c::gfx_op_pipe_control(6) | bit_group_0, bit_group_1, offset, 0, 0, 0])
}

/// `__gen8_emit_write_rcs` (gen8_engine_cs.h:78-90): the qword-write pipe control that stores `value` at `offset`.
fn emit_write_rcs(cs: &mut CsBuf, value: u32, offset: u32, flags0: u32, flags1: u32) -> Result<(), GtError> {
    cs.reserve(6)?;
    cs.extend(&[c::gfx_op_pipe_control(6) | flags0, flags1 | c::PIPE_CONTROL_QW_WRITE, offset, 0, value, 0])
}

/// `gen8_emit_ggtt_write_rcs` (gen8_engine_cs.h:91-101): offset must be 8-byte aligned (`GEM_BUG_ON`, :95).
pub fn emit_ggtt_write_rcs(cs: &mut CsBuf, value: u32, gtt_offset: u32, flags: u32) -> Result<(), GtError> {
    if gtt_offset % 8 != 0 {
        return Err(GtError::BadGgttOffset { offset: gtt_offset, why: "qword write needs an 8-byte aligned offset" });
    }
    emit_write_rcs(cs, value, gtt_offset, 0, flags | c::PIPE_CONTROL_GLOBAL_GTT_IVB)
}

/// `__gen8_emit_flush_dw` (gen8_engine_cs.h:117-127): `{ (MI_FLUSH_DW + 1) | flags, gtt_offset, 0, value }`.
fn emit_flush_dw(cs: &mut CsBuf, value: u32, gtt_offset: u32, flags: u32) -> Result<(), GtError> {
    cs.reserve(4)?;
    cs.extend(&[(c::MI_FLUSH_DW + 1) | flags, gtt_offset, 0, value])
}

/// `gen8_emit_ggtt_write` (gen8_engine_cs.h:128-141): bit 5 of the address must be zero (a workaround) and the offset 8-byte
/// aligned (`GEM_BUG_ON`s, :132,134).
pub fn emit_ggtt_write(cs: &mut CsBuf, value: u32, gtt_offset: u32, flags: u32) -> Result<(), GtError> {
    if gtt_offset & (1 << 5) != 0 {
        return Err(GtError::BadGgttOffset { offset: gtt_offset, why: "MI_FLUSH_DW address bit 5 must be zero" });
    }
    if gtt_offset % 8 != 0 {
        return Err(GtError::BadGgttOffset { offset: gtt_offset, why: "write needs an 8-byte aligned offset" });
    }
    emit_flush_dw(cs, value, gtt_offset | c::MI_FLUSH_DW_USE_GTT, flags | c::MI_FLUSH_DW_OP_STOREDW)
}

/// The two Gen9 quirks `gen8_emit_flush_rcs` consults (gen8_engine_cs.c:38-47).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlushQuirks {
    /// `GRAPHICS_VER == 9`: a NULL pipe control before `VF_CACHE_INVALIDATE`.
    pub vf_flush_wa: bool,
    /// `WaForGAMHang:kbl`: `IS_KABYLAKE && IS_GRAPHICS_STEP(0, STEP_C0)`.
    pub dc_flush_wa_kbl: bool,
}

impl FlushQuirks {
    pub fn for_context(ctx: &WaContext) -> FlushQuirks {
        FlushQuirks {
            vf_flush_wa: ctx.graphics_ver == 9,
            dc_flush_wa_kbl: ctx.platform == Platform::Kabylake && is_graphics_step(ctx.step, Step::NONE, Step::C0),
        }
    }
}

/// The dword count `gen8_emit_flush_rcs` reserves for `mode` (gen8_engine_cs.c:49-56): 6, +6 for the VF flush, +12 for the DC
/// flush. Only an INVALIDATE turns the two workarounds on.
pub const fn flush_rcs_len(mode: u32, q: FlushQuirks) -> usize {
    let mut len = 6;
    if mode & EMIT_INVALIDATE != 0 {
        if q.vf_flush_wa {
            len += 6;
        }
        if q.dc_flush_wa_kbl {
            len += 12;
        }
    }
    len
}

/// `gen8_emit_flush_rcs` (gt/gen8_engine_cs.c:15-79).
pub fn emit_flush_rcs(cs: &mut CsBuf, mode: u32, q: FlushQuirks) -> Result<(), GtError> {
    let mut flags = c::PIPE_CONTROL_CS_STALL;
    let (mut vf_flush_wa, mut dc_flush_wa) = (false, false);
    if mode & EMIT_FLUSH != 0 {
        flags |= c::PIPE_CONTROL_RENDER_TARGET_CACHE_FLUSH;
        flags |= c::PIPE_CONTROL_DEPTH_CACHE_FLUSH;
        flags |= c::PIPE_CONTROL_DC_FLUSH_ENABLE;
        flags |= c::PIPE_CONTROL_FLUSH_ENABLE;
    }
    if mode & EMIT_INVALIDATE != 0 {
        flags |= c::PIPE_CONTROL_TLB_INVALIDATE;
        flags |= c::PIPE_CONTROL_INSTRUCTION_CACHE_INVALIDATE;
        flags |= c::PIPE_CONTROL_TEXTURE_CACHE_INVALIDATE;
        flags |= c::PIPE_CONTROL_VF_CACHE_INVALIDATE;
        flags |= c::PIPE_CONTROL_CONST_CACHE_INVALIDATE;
        flags |= c::PIPE_CONTROL_STATE_CACHE_INVALIDATE;
        flags |= c::PIPE_CONTROL_QW_WRITE;
        flags |= c::PIPE_CONTROL_STORE_DATA_INDEX;
        vf_flush_wa = q.vf_flush_wa; // gen8_engine_cs.c:43
        dc_flush_wa = q.dc_flush_wa_kbl; // gen8_engine_cs.c:47
    }
    cs.reserve(flush_rcs_len(mode, q))?;
    if vf_flush_wa {
        emit_pipe_control(cs, 0, 0, 0)?;
    }
    if dc_flush_wa {
        emit_pipe_control(cs, 0, c::PIPE_CONTROL_DC_FLUSH_ENABLE, 0)?;
    }
    emit_pipe_control(cs, 0, flags, LRC_PPHWSP_SCRATCH_ADDR)?;
    if dc_flush_wa {
        emit_pipe_control(cs, 0, c::PIPE_CONTROL_CS_STALL, 0)?;
    }
    Ok(())
}

/// `gen8_emit_flush_xcs` (gen8_engine_cs.c:81-112): four dwords for the BCS / VCS / VECS.
pub fn emit_flush_xcs(cs: &mut CsBuf, mode: u32, video_decode: bool) -> Result<(), GtError> {
    cs.reserve(4)?;
    let mut cmd = c::MI_FLUSH_DW + 1;
    cmd |= c::MI_FLUSH_DW_STORE_INDEX | c::MI_FLUSH_DW_OP_STOREDW;
    if mode & EMIT_INVALIDATE != 0 {
        cmd |= c::MI_INVALIDATE_TLB;
        if video_decode {
            cmd |= c::MI_INVALIDATE_BSD;
        }
    }
    cs.extend(&[cmd, LRC_PPHWSP_SCRATCH_ADDR, 0, 0])
}

/// `gen8_emit_bb_start_noarb` (gen8_engine_cs.c:538-572): arbitration disabled for the batch; four dwords.
pub fn emit_bb_start_noarb(cs: &mut CsBuf, offset: u64, flags: u32) -> Result<(), GtError> {
    cs.reserve(4)?;
    cs.extend(&[
        c::MI_ARB_ON_OFF | c::MI_ARB_DISABLE,
        c::MI_BATCH_BUFFER_START_GEN8 | if flags & I915_DISPATCH_SECURE != 0 { 0 } else { 1 << 8 },
        offset as u32,
        (offset >> 32) as u32,
    ])
}

/// `gen8_emit_bb_start` (gen8_engine_cs.c:574-600): six dwords, arbitration on around the batch; a no-preempt request takes the
/// `noarb` form (`i915_request_has_nopreempt`).
pub fn emit_bb_start(cs: &mut CsBuf, offset: u64, flags: u32, nopreempt: bool) -> Result<(), GtError> {
    if nopreempt {
        return emit_bb_start_noarb(cs, offset, flags);
    }
    cs.reserve(6)?;
    cs.extend(&[
        c::MI_ARB_ON_OFF | c::MI_ARB_ENABLE,
        c::MI_BATCH_BUFFER_START_GEN8 | if flags & I915_DISPATCH_SECURE != 0 { 0 } else { 1 << 8 },
        offset as u32,
        (offset >> 32) as u32,
        c::MI_ARB_ON_OFF | c::MI_ARB_DISABLE,
        c::MI_NOOP,
    ])
}

/// `gen8_emit_init_breadcrumb` (gen8_engine_cs.c:436-481) for a timeline with an initial breadcrumb: six dwords storing
/// `seqno - 1` at the request's HWSP slot, then the preemption point. Returns `infix` (dwords written).
pub fn emit_init_breadcrumb(cs: &mut CsBuf, hwsp_offset: u32, seqno: u32) -> Result<usize, GtError> {
    cs.reserve(6)?;
    cs.extend(&[c::MI_STORE_DWORD_IMM_GEN4 | c::MI_USE_GGTT, hwsp_offset, 0, seqno.wrapping_sub(1), c::MI_NOOP, c::MI_ARB_CHECK])?;
    Ok(6)
}

/// Where the request's ring offsets fall, as dword counts from the start of the fini breadcrumb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreadcrumbEnds {
    /// `rq->tail`: after the interrupt / busywait, before the workaround NOOPs.
    pub tail: usize,
    /// `rq->wa_tail`: after the two WaIdleLiteRestore NOOPs.
    pub wa_tail: usize,
}

/// `emit_preempt_busywait` (gen8_engine_cs.c:628-642): six dwords.
fn emit_preempt_busywait(cs: &mut CsBuf, preempt_address: u32) -> Result<(), GtError> {
    cs.extend(&[
        c::MI_ARB_CHECK,
        c::MI_SEMAPHORE_WAIT | c::MI_SEMAPHORE_GLOBAL_GTT | c::MI_SEMAPHORE_POLL | c::MI_SEMAPHORE_SAD_EQ_SDD,
        0,
        preempt_address,
        0,
        c::MI_NOOP,
    ])
}

/// `gen8_emit_fini_breadcrumb_tail` (gen8_engine_cs.c:644-657) + `gen8_emit_wa_tail` (:615-626). `start` is the dword count
/// already written by the caller's part of the breadcrumb; the returned ends are relative to the breadcrumb's first dword.
fn emit_fini_tail(cs: &mut CsBuf, start: usize, semaphores: Option<u32>) -> Result<BreadcrumbEnds, GtError> {
    cs.push(c::MI_USER_INTERRUPT)?;
    cs.push(c::MI_ARB_ON_OFF | c::MI_ARB_ENABLE)?;
    if let Some(preempt_address) = semaphores {
        cs.reserve(6)?;
        emit_preempt_busywait(cs, preempt_address)?;
    }
    let tail = cs.len() - start;
    cs.push(c::MI_ARB_CHECK)?;
    cs.push(c::MI_NOOP)?;
    Ok(BreadcrumbEnds { tail, wa_tail: cs.len() - start })
}

/// The dword count of `gen8_emit_fini_breadcrumb_xcs` / `_rcs`, so a caller can size the request's reserved tail
/// (`engine->emit_fini_breadcrumb_dw`): xcs = 4 + 2 + (6 if semaphores) + 2; rcs = 6 + 6 + 2 + (6 if semaphores) + 2.
pub const fn fini_breadcrumb_len(render: bool, semaphores: bool) -> usize {
    (if render { 12 } else { 4 }) + 2 + (if semaphores { 6 } else { 0 }) + 2
}

/// `gen8_emit_fini_breadcrumb_xcs` (gen8_engine_cs.c:664-667): `emit_xcs_breadcrumb` (a GGTT flush-dw write of `seqno` to the
/// request's HWSP slot) then the common tail. `semaphores` is `Some(preempt_address)` when the engine has semaphores and GuC
/// submission is not in use (`intel_engine_has_semaphores && !intel_uc_uses_guc_submission`, :649-651).
pub fn emit_fini_breadcrumb_xcs(cs: &mut CsBuf, seqno: u32, hwsp_offset: u32, semaphores: Option<u32>) -> Result<BreadcrumbEnds, GtError> {
    cs.reserve(fini_breadcrumb_len(false, semaphores.is_some()))?;
    let start = cs.len();
    emit_ggtt_write(cs, seqno, hwsp_offset, 0)?;
    emit_fini_tail(cs, start, semaphores)
}

/// `gen8_emit_fini_breadcrumb_rcs` (gen8_engine_cs.c:669-687): a flushing pipe control, the seqno qword write
/// (`FLUSH_ENABLE | CS_STALL`), then the common tail.
pub fn emit_fini_breadcrumb_rcs(cs: &mut CsBuf, seqno: u32, hwsp_offset: u32, semaphores: Option<u32>) -> Result<BreadcrumbEnds, GtError> {
    cs.reserve(fini_breadcrumb_len(true, semaphores.is_some()))?;
    let start = cs.len();
    emit_pipe_control(
        cs,
        0,
        c::PIPE_CONTROL_CS_STALL
            | c::PIPE_CONTROL_TLB_INVALIDATE
            | c::PIPE_CONTROL_RENDER_TARGET_CACHE_FLUSH
            | c::PIPE_CONTROL_DEPTH_CACHE_FLUSH
            | c::PIPE_CONTROL_DC_FLUSH_ENABLE,
        0,
    )?;
    emit_ggtt_write_rcs(cs, seqno, hwsp_offset, c::PIPE_CONTROL_FLUSH_ENABLE | c::PIPE_CONTROL_CS_STALL)?;
    emit_fini_tail(cs, start, semaphores)
}

/// `intel_engine_emit_ctx_wa` payload (intel_workarounds.c:1023-1038): `MI_LOAD_REGISTER_IMM(n)`, `n` (offset, value) pairs,
/// `MI_NOOP`. `read` supplies the register's current value for an entry that needs one; it is called ONLY for entries that are
/// neither masked nor all-ones (`wa->masked_reg || (wa->clr | wa->set) == U32_MAX` skips the read, :1028). Returns the dwords
/// written (`2 * count + 2`, the length Linux reserves, :1010-1012).
pub fn emit_ctx_wa<const N: usize>(
    cs: &mut CsBuf,
    wal: &crate::wa::WaList<N>,
    mut read: impl FnMut(i915_common_core::Reg) -> Option<u32>,
) -> Result<usize, GtError> {
    let count = wal.count();
    if count == 0 {
        return Ok(0);
    }
    let len = count * 2 + 2;
    cs.reserve(len)?;
    // Resolve every value BEFORE writing a dword, so a missing read refuses without leaving a half-emitted LRI.
    let mut vals = [0u32; N];
    for (i, wa) in wal.entries().iter().enumerate() {
        vals[i] = if wa.masked || (wa.clr | wa.set) == u32::MAX {
            wa.set
        } else {
            let cur = read(wa.reg).ok_or(GtError::CtxWaNeedsRead { reg: wa.reg })?;
            (cur & !wa.clr) | wa.set
        };
    }
    cs.push(c::mi_load_register_imm(count as u32))?;
    for (i, wa) in wal.entries().iter().enumerate() {
        cs.push(wa.reg.offset())?;
        cs.push(vals[i])?;
    }
    cs.push(c::MI_NOOP)?;
    Ok(len)
}
