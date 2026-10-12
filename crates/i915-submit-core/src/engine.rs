// SPDX-License-Identifier: GPL-2.0-only
//! One execlists engine: its contexts (each with a ring, a timeline and a register-state image), the request table, and the
//! request construction path of `__i915_request_create` / `execlists_request_alloc` / `__i915_request_commit`
//! (i915_request.c:975-1010, :1787-1815; gt/intel_execlists_submission.c:2768-2802). Submission, CSB consumption, retirement and
//! hang detection are `impl Engine` blocks in [`crate::submit`].
//!
//! The engine owns the MODEL of every context's image and ring; the caller (the executor in `devices/i915`) owns the memory:
//! it passes the ring's CPU mapping to [`Engine::build_request`] and copies [`Engine::context_regs`] into the context object.
use crate::error::SubmitError;
use crate::execlists::Execlists;
use crate::lrc_reg::*;
use crate::lrc_state::{self, CtxParams, RegState};
use crate::request::{Request, TagAllocator, Table, Timeline};
use crate::ring::Ring;
use crate::wa_bb::WaCtx;
use i915_gt_core::emit::{
    emit_bb_start, emit_fini_breadcrumb_rcs, emit_fini_breadcrumb_xcs, emit_flush_rcs, emit_flush_xcs, emit_init_breadcrumb, fini_breadcrumb_len,
    flush_rcs_len, BreadcrumbEnds, CsBuf, FlushQuirks, EMIT_INVALIDATE,
};
use i915_gt_core::engine::{EngineClass, EngineInfo};

/// Per-engine configuration the executor supplies.
#[derive(Clone, Copy, Debug)]
pub struct EngineConfig {
    /// `i915_ggtt_offset(engine->status_page.vma)` (written to `RING_HWS_PGA`).
    pub status_page_ggtt: u32,
    /// The Gen9 flush workarounds (`FlushQuirks::for_context`).
    pub quirks: FlushQuirks,
    /// The workaround batch (`engine->wa_ctx`) with the GGTT offset of its object, for a render engine that has one.
    pub wa_ctx: Option<(u32, WaCtx)>,
    /// How long the oldest request may make no progress, or an ELSP submission stay unacknowledged, in the caller's clock
    /// units, before [`crate::submit`] declares the engine hung. Must be non-zero.
    pub stall_bound: u64,
}

/// Everything needed to create one context.
#[derive(Clone, Copy, Debug)]
pub struct ContextSpec<'a> {
    /// `i915_ggtt_offset(ce->state)`: the page-aligned GGTT address of the context image (the LRCA).
    pub lrca: u32,
    /// `ce->ring_size` (`SZ_4K` by default, intel_context.c:403).
    pub ring_size: u32,
    /// `i915_ggtt_offset(ring->vma)`.
    pub ring_ggtt: u32,
    /// The PML4's device address from `i915_memory_core::PpgttInit::pml4` (`px_dma(ppgtt->pd)`).
    pub pml4: u64,
    /// `intel_sseu_make_rpcs(...)` for a render engine (`i915_gt_core::sseu::make_rpcs_gen9`); ignored on the others.
    pub rpcs: u32,
    /// This context's timeline: GGTT address of its seqno dword, and whether it has an initial breadcrumb.
    pub hwsp_ggtt: u32,
    pub initial_breadcrumb: bool,
    /// `ce->default_state`: the golden context's second page, when the engine has one. Without it the context is created with
    /// `inhibit` (the first restore is inhibited and the page starts zeroed).
    pub default_state: Option<&'a RegState>,
}

/// What to put in the ring for one request's payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatchSpec {
    /// The batch's GPU address (`MI_BATCH_BUFFER_START` operand).
    pub offset: u64,
    /// `I915_DISPATCH_*` flags.
    pub flags: u32,
    /// `i915_request_has_nopreempt`.
    pub nopreempt: bool,
}

/// The identity of a built request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RequestRef {
    pub ctx: u32,
    pub seqno: u32,
}

/// `struct intel_context`, reduced to what the execlists path touches.
#[derive(Clone, Debug)]
pub(crate) struct Context {
    pub params: CtxParams,
    pub lrca: u32,
    pub regs: RegState,
    pub ring: Ring,
    pub tl: Timeline,
    /// `ce->lrc.lrca`: the low descriptor dword including the flags; `CTX_DESC_FORCE_RESTORE` is consumed at submission.
    pub desc_lo: u32,
    /// `ce->lrc.ccid`: the high descriptor dword, assigned when the context first goes in flight.
    pub ccid: u32,
    /// `ce->inflight`'s count: the number of ports (pending + in flight) naming this context.
    pub inflight: u32,
    /// The last value the hardware wrote to the timeline's HWSP seqno dword, as observed by the executor.
    pub hw_seqno: u32,
}

/// One engine of the GT.
#[derive(Clone, Debug)]
pub struct Engine<const C: usize, const R: usize> {
    pub(crate) info: EngineInfo,
    pub(crate) gen: u8,
    pub(crate) cfg: EngineConfig,
    pub(crate) execlists: Execlists,
    pub(crate) ctxs: [Option<Context>; C],
    pub(crate) reqs: Table<Request, R>,
    pub(crate) tags: TagAllocator,
    pub(crate) watch: crate::submit::Watch,
}

const NO_REQUEST: Request = Request { ctx: 0, seqno: 0, head: 0, infix: 0, postfix: 0, tail: 0, wa_tail: 0, submitted: false };

impl<const C: usize, const R: usize> Engine<C, R> {
    /// A fresh engine with the CSB pointers at their post-reset position. `gen` is `GRAPHICS_VER` (9 for Kaby Lake).
    pub fn new(info: &EngineInfo, gen: u8, cfg: EngineConfig) -> Result<Self, SubmitError> {
        if gen != 8 && gen != 9 {
            return Err(SubmitError::UnsupportedGeneration { gen });
        }
        if cfg.stall_bound == 0 {
            return Err(SubmitError::ZeroBound { what: "stall_bound" });
        }
        if cfg.status_page_ggtt & 0xfff != 0 {
            return Err(SubmitError::GgttUnaligned { what: "status page", addr: cfg.status_page_ggtt });
        }
        Ok(Engine {
            info: *info,
            gen,
            cfg,
            execlists: Execlists::new(),
            ctxs: core::array::from_fn(|_| None),
            reqs: Table::new(NO_REQUEST),
            tags: TagAllocator::new(),
            watch: crate::submit::Watch::new(),
        })
    }

    pub fn info(&self) -> &EngineInfo {
        &self.info
    }
    pub fn execlists(&self) -> &Execlists {
        &self.execlists
    }
    pub fn requests(&self) -> &[Request] {
        self.reqs.as_slice()
    }
    pub fn is_render(&self) -> bool {
        self.info.class == EngineClass::Render
    }

    pub(crate) fn ctx(&self, id: u32) -> Result<&Context, SubmitError> {
        self.ctxs.get(id as usize).and_then(|c| c.as_ref()).ok_or(SubmitError::NoSuchContext { id })
    }
    pub(crate) fn ctx_mut(&mut self, id: u32) -> Result<&mut Context, SubmitError> {
        self.ctxs.get_mut(id as usize).and_then(|c| c.as_mut()).ok_or(SubmitError::NoSuchContext { id })
    }

    /// The context's register-state page, which the executor mirrors into the context object (image offset
    /// [`LRC_STATE_OFFSET`]).
    pub fn context_regs(&self, id: u32) -> Result<&RegState, SubmitError> {
        Ok(&self.ctx(id)?.regs)
    }
    pub fn context_ring(&self, id: u32) -> Result<&Ring, SubmitError> {
        Ok(&self.ctx(id)?.ring)
    }
    pub fn context_timeline(&self, id: u32) -> Result<&Timeline, SubmitError> {
        Ok(&self.ctx(id)?.tl)
    }
    /// `ce->lrc.desc` as the ELSP would take it now: ccid in the upper dword (zero until the context first goes in flight).
    pub fn context_descriptor(&self, id: u32) -> Result<u64, SubmitError> {
        let c = self.ctx(id)?;
        Ok((c.ccid as u64) << GEN8_CTX_ID_SHIFT | c.desc_lo as u64)
    }
    /// The seqno the hardware last reported for this context's timeline.
    pub fn hw_seqno(&self, id: u32) -> Result<u32, SubmitError> {
        Ok(self.ctx(id)?.hw_seqno)
    }

    /// `engine->emit_fini_breadcrumb_dw`.
    fn fini_dw(&self) -> usize {
        fini_breadcrumb_len(self.is_render(), true)
    }

    /// `lrc_alloc` + `lrc_pin` + `lrc_init_state` (gt/intel_lrc.c:1129-1179, :1210-1222, :1016-1042): create the ring, the
    /// timeline and the register-state image, and leave the context ready for its first request.
    pub fn create_context(&mut self, spec: &ContextSpec) -> Result<u32, SubmitError> {
        let slot = self.ctxs.iter().position(|c| c.is_none()).ok_or(SubmitError::ContextTableFull { capacity: C as u32 })?;
        let ring = Ring::new(spec.ring_size)?;
        let tl = Timeline::new(spec.hwsp_ggtt, spec.initial_breadcrumb)?;
        let params = CtxParams {
            gen: self.gen,
            render: self.is_render(),
            mmio_base: self.info.mmio_base,
            pml4: spec.pml4,
            ring_ggtt: spec.ring_ggtt,
            rpcs: spec.rpcs,
            runtime_last: 0,
            wa_ctx: self.cfg.wa_ctx,
        };
        let mut regs = match spec.default_state {
            Some(s) => s.clone(),
            None => RegState::zeroed(),
        };
        lrc_state::init_regs(&mut regs, &params, spec.default_state.is_none())?;
        let desc_lo = lrc_state::update_regs(&mut regs, &params, spec.lrca, &ring, ring.tail())?;
        self.ctxs[slot] = Some(Context { params, lrca: spec.lrca, regs, ring, tl, desc_lo, ccid: 0, inflight: 0, hw_seqno: 0 });
        Ok(slot as u32)
    }

    /// `lrc_reset` (gt/intel_lrc.c:1181-1190): scrub the context image after a hang. The ring is collapsed to its current `emit`
    /// (everything before it is abandoned), the register state is rebuilt as for a context with no default state (restore
    /// inhibited) and the descriptor is recomputed. The context must not be in flight.
    pub fn reset_context(&mut self, id: u32) -> Result<(), SubmitError> {
        if self.ctx(id)?.inflight != 0 {
            return Err(SubmitError::ContextBusy { id });
        }
        let c = self.ctx_mut(id)?;
        let emit = c.ring.emit();
        c.ring.reset(emit);
        lrc_state::init_regs(&mut c.regs, &c.params, true)?;
        c.desc_lo = lrc_state::update_regs(&mut c.regs, &c.params, c.lrca, &c.ring, c.ring.tail())?;
        Ok(())
    }

    /// `i915_ggtt_offset(ce->state)`.
    pub fn context_lrca(&self, id: u32) -> Result<u32, SubmitError> {
        Ok(self.ctx(id)?.lrca)
    }

    /// Release a context that has no requests and is not in flight.
    pub fn destroy_context(&mut self, id: u32) -> Result<(), SubmitError> {
        let busy = self.ctx(id)?.inflight != 0 || self.reqs.as_slice().iter().any(|r| r.ctx == id);
        if busy {
            return Err(SubmitError::ContextBusy { id });
        }
        self.ctxs[id as usize] = None;
        Ok(())
    }

    /// Build one request on `ctx`'s ring: the invalidating flush (`execlists_request_alloc`), the initial breadcrumb, the
    /// batch start, and the fini breadcrumb committed into the reserved space (`__i915_request_commit`). `ring_mem` is the
    /// ring's CPU mapping (`ring_size / 4` dwords). On any refusal the ring, timeline and request table are unchanged
    /// (dwords beyond the ring's `emit` may have been scribbled, as in Linux).
    pub fn build_request(&mut self, ctx: u32, ring_mem: &mut [u32], batch: &BatchSpec) -> Result<RequestRef, SubmitError> {
        let render = self.is_render();
        let video = self.info.class == EngineClass::VideoDecode;
        let quirks = self.cfg.quirks;
        let fini_dw = self.fini_dw();
        let preempt_addr = self.cfg.status_page_ggtt + I915_GEM_HWS_PREEMPT * 4;
        if self.reqs.len() >= R {
            return Err(SubmitError::RequestTableFull { capacity: R as u32 });
        }
        let c = self.ctx(ctx)?;
        let mut ring = c.ring;
        let mut tl = c.tl;
        if ring_mem.len() as u32 * 4 != ring.size() {
            return Err(SubmitError::BadRingSize { size: ring_mem.len() as u32 * 4 });
        }
        let seqno = tl.advance()?;
        let hwsp = tl.hwsp_ggtt;
        // Space for the fini breadcrumb is reserved twice over so a wrap inside commit cannot fail (i915_request.c:993-994).
        let reserved = 2 * fini_dw as u32 * 4;
        let head = ring.emit();

        // execlists_request_alloc: reserved_space += EXECLISTS_REQUEST_SIZE; emit_flush(INVALIDATE); reserved_space -= ...
        let flush_dw = if render { flush_rcs_len(EMIT_INVALIDATE, quirks) } else { 4 };
        emit(&mut ring, ring_mem, flush_dw, reserved + EXECLISTS_REQUEST_SIZE, |cs| {
            if render {
                emit_flush_rcs(cs, EMIT_INVALIDATE, quirks)
            } else {
                emit_flush_xcs(cs, EMIT_INVALIDATE, video)
            }
        })?;
        // gen8_emit_init_breadcrumb: only a timeline with an initial breadcrumb (intel_timeline.c:90)
        if tl.initial_breadcrumb {
            emit(&mut ring, ring_mem, 6, reserved, |cs| emit_init_breadcrumb(cs, hwsp, seqno).map(|_| ()))?;
        }
        let infix = ring.emit();
        // engine->emit_bb_start
        let bb_dw = if batch.nopreempt { 4 } else { 6 };
        emit(&mut ring, ring_mem, bb_dw, reserved, |cs| emit_bb_start(cs, batch.offset, batch.flags, batch.nopreempt))?;
        // __i915_request_commit: reserved_space = 0, intel_ring_begin(fini), postfix = offset of the reserved span
        let mut ends = BreadcrumbEnds { tail: 0, wa_tail: 0 };
        let postfix = emit(&mut ring, ring_mem, fini_dw, 0, |cs| {
            ends = if render { emit_fini_breadcrumb_rcs(cs, seqno, hwsp, Some(preempt_addr))? } else { emit_fini_breadcrumb_xcs(cs, seqno, hwsp, Some(preempt_addr))? };
            Ok(())
        })?;
        let tail = ring.offset(postfix + ends.tail as u32 * 4);
        let wa_tail = ring.offset(postfix + ends.wa_tail as u32 * 4);
        // assert_ring_tail_valid(rq->tail) in gen8_emit_fini_breadcrumb_tail (gen8_engine_cs.c:653-654). The companion
        // `GEM_BUG_ON(intel_ring_direction(ring, rq->wa_tail, rq->head) <= 0)` (:607) cannot trip here: a request is a fixed
        // few hundred bytes and the smallest ring is 4 KiB, so wa_tail is always less than half a ring ahead of head.
        ring.check_tail(tail)?;
        let rq = Request { ctx, seqno, head, infix, postfix, tail, wa_tail, submitted: false };
        self.reqs.push(rq)?;
        let c = self.ctx_mut(ctx)?;
        c.ring = ring;
        c.tl = tl;
        Ok(RequestRef { ctx, seqno })
    }
}

/// `intel_ring_begin` + the emitter + `intel_ring_advance`'s length check, against the ring's CPU mapping. Returns the byte
/// offset of the packet. A wrap fills the tail of the ring with `MI_NOOP` (zero qwords) first.
fn emit(ring: &mut Ring, mem: &mut [u32], dwords: usize, reserved: u32, f: impl FnOnce(&mut CsBuf) -> Result<(), i915_gt_core::GtError>) -> Result<u32, SubmitError> {
    let b = ring.begin(dwords as u32, reserved)?;
    if let Some((off, bytes)) = b.wrap_fill {
        for d in &mut mem[(off / 4) as usize..((off + bytes) / 4) as usize] {
            *d = 0;
        }
    }
    let first = (b.at / 4) as usize;
    let win = &mut mem[first..first + dwords];
    let mut cs = CsBuf::new(win);
    f(&mut cs)?;
    if cs.len() != dwords {
        return Err(SubmitError::RingEmitMismatch { reserved: dwords as u32, wrote: cs.len() as u32 });
    }
    Ok(b.at)
}
