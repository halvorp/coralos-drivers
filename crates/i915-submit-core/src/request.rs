// SPDX-License-Identifier: GPL-2.0-only
//! Requests, timelines and seqnos: `i915_seqno_passed` (i915_request.h:480-483), the completion tests built on it, the
//! timeline seqno advance (gt/intel_timeline.c:300-338), and the hardware context-tag bitmap
//! (`engine->context_tag`, gt/intel_execlists_submission.c:3579, :494-514, :619-625).
use crate::error::SubmitError;
use crate::lrc_reg::GEN11_SW_CTX_ID_SHIFT;

/// `i915_seqno_passed` (i915_request.h:480-483): has `seq1` reached `seq2`, modulo 2^32.
pub const fn seqno_passed(seq1: u32, seq2: u32) -> bool {
    (seq1.wrapping_sub(seq2) as i32) >= 0
}

/// `struct intel_timeline`'s seqno half: where the hardware writes this timeline's breadcrumbs and what number is next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeline {
    /// `tl->hwsp_offset`: the GGTT byte address of the seqno dword. Must be qword aligned with bit 5 clear: the breadcrumb is
    /// an `MI_FLUSH_DW` qword write ("w/a: bit 5 needs to be zero for MI_FLUSH_DW address", intel_timeline.c:314).
    pub hwsp_ggtt: u32,
    /// `tl->has_initial_breadcrumb`: a per-context timeline stores `seqno - 1` when the request starts, so seqnos advance by 2.
    /// The kernel context's timeline lives in the engine status page and has none (intel_timeline.c:90, :98).
    pub initial_breadcrumb: bool,
    seqno: u32,
}

impl Timeline {
    pub fn new(hwsp_ggtt: u32, initial_breadcrumb: bool) -> Result<Timeline, SubmitError> {
        if hwsp_ggtt % 8 != 0 {
            return Err(SubmitError::SeqnoSlotInvalid { addr: hwsp_ggtt, why: "the breadcrumb is a qword write: 8-byte alignment" });
        }
        if hwsp_ggtt & (1 << 5) != 0 {
            return Err(SubmitError::SeqnoSlotInvalid { addr: hwsp_ggtt, why: "MI_FLUSH_DW address bit 5 must be zero" });
        }
        Ok(Timeline { hwsp_ggtt, initial_breadcrumb, seqno: 0 })
    }
    /// Re-attach to a HWSP slot that already holds `seqno` (`intel_timeline_reset_seqno` writes `tl->seqno` back into the slot,
    /// intel_timeline.c:224-233). `seqno` must be even for a timeline with an initial breadcrumb (`GEM_BUG_ON(tl->seqno &
    /// tl->has_initial_breadcrumb)`, :303).
    pub fn resume(hwsp_ggtt: u32, initial_breadcrumb: bool, seqno: u32) -> Result<Timeline, SubmitError> {
        let mut tl = Timeline::new(hwsp_ggtt, initial_breadcrumb)?;
        if initial_breadcrumb && seqno & 1 != 0 {
            return Err(SubmitError::SeqnoSlotInvalid { addr: hwsp_ggtt, why: "a timeline with an initial breadcrumb holds even seqnos" });
        }
        tl.seqno = seqno;
        Ok(tl)
    }
    /// The last seqno handed out (0 for a fresh timeline).
    pub const fn seqno(&self) -> u32 {
        self.seqno
    }
    /// What the next request would be numbered, without taking it. `timeline_advance` (intel_timeline.c:300-306) adds
    /// `1 + has_initial_breadcrumb`; a wrap to zero (Linux rotates the HWSP slot, :308-325) is refused by name.
    pub fn peek_next(&self) -> Result<u32, SubmitError> {
        let step = 1 + self.initial_breadcrumb as u32;
        match self.seqno.checked_add(step) {
            Some(next) => Ok(next),
            None => Err(SubmitError::SeqnoWrap { seqno: self.seqno }),
        }
    }
    /// Take the next seqno.
    pub fn advance(&mut self) -> Result<u32, SubmitError> {
        let next = self.peek_next()?;
        self.seqno = next;
        Ok(next)
    }
}

/// `struct i915_request` reduced to the ring bookkeeping and the completion state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request {
    /// The owning context's table index.
    pub ctx: u32,
    /// `rq->fence.seqno`.
    pub seqno: u32,
    /// `rq->head`: the ring's `emit` when the request was allocated.
    pub head: u32,
    /// `rq->infix`: the end of the header, start of the user payload.
    pub infix: u32,
    /// `rq->postfix`: the start of the fini breadcrumb; the ring head moves here when the request retires.
    pub postfix: u32,
    /// `rq->tail`: submitted to the hardware as `RING_TAIL`; after the first submission it is set to `wa_tail`.
    pub tail: u32,
    /// `rq->wa_tail`: after the two `WaIdleLiteRestore` NOOPs.
    pub wa_tail: u32,
    /// `__i915_request_submit` has been applied: the request has been handed to a port.
    pub submitted: bool,
}

/// A fixed-capacity ordered table with removal, so the crate allocates nothing and a full table is a named refusal.
#[derive(Clone, Copy, Debug)]
pub struct Table<T: Copy, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy, const N: usize> Table<T, N> {
    pub const fn new(fill: T) -> Self {
        Table { items: [fill; N], len: 0 }
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn push(&mut self, v: T) -> Result<(), SubmitError> {
        if self.len == N {
            return Err(SubmitError::RequestTableFull { capacity: N as u32 });
        }
        self.items[self.len] = v;
        self.len += 1;
        Ok(())
    }
    pub fn as_slice(&self) -> &[T] {
        &self.items[..self.len]
    }
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..self.len]
    }
    /// Remove index `i`, keeping the order of the rest.
    pub fn remove(&mut self, i: usize) -> T {
        let v = self.items[i];
        let mut j = i;
        while j + 1 < self.len {
            self.items[j] = self.items[j + 1];
            j += 1;
        }
        self.len -= 1;
        v
    }
}

/// `engine->context_tag` (gt/intel_execlists_submission.c:3579): 63 usable tags, `GENMASK(BITS_PER_LONG - 2, 0)`; a context
/// takes the lowest free tag when it first goes in flight and returns it when it last leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TagAllocator {
    free: u64,
}

impl Default for TagAllocator {
    fn default() -> Self {
        TagAllocator::new()
    }
}

impl TagAllocator {
    pub const TAGS: u32 = 63;
    pub const fn new() -> TagAllocator {
        TagAllocator { free: (1u64 << 63) - 1 }
    }
    pub const fn free_tags(&self) -> u32 {
        self.free.count_ones()
    }
    /// `__ffs(engine->context_tag)` + `__clear_bit` (:505-508).
    pub fn alloc(&mut self) -> Result<u32, SubmitError> {
        if self.free == 0 {
            return Err(SubmitError::TagsExhausted);
        }
        let tag = self.free.trailing_zeros();
        self.free &= !(1u64 << tag);
        Ok(tag)
    }
    /// `__set_bit(ccid - 1, &engine->context_tag)` (:625).
    pub fn release(&mut self, tag: u32) {
        if tag < Self::TAGS {
            self.free |= 1u64 << tag;
        }
    }
}

/// The upper descriptor dword for a tag on a Gen8/Gen9 part: `(1 + tag) << (GEN11_SW_CTX_ID_SHIFT - 32)` (:509). The Gen8/Gen9
/// engines add no class / instance bits (`execlists->ccid` stays 0, :3585-3589).
pub const fn ccid_for_tag(tag: u32) -> u32 {
    (1 + tag) << (GEN11_SW_CTX_ID_SHIFT - 32)
}

/// The inverse (`__execlists_schedule_out`, :610-625): the tag a ccid was built from, or `None` for a fixed `ce->tag`.
pub const fn tag_for_ccid(ccid: u32) -> Option<u32> {
    let id = (ccid >> (GEN11_SW_CTX_ID_SHIFT - 32)) & crate::lrc_reg::GEN12_MAX_CONTEXT_HW_ID;
    if id >= 1 && id <= TagAllocator::TAGS {
        Some(id - 1)
    } else {
        None
    }
}
