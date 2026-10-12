// SPDX-License-Identifier: GPL-2.0-only
//! The ring buffer's space, wrap and tail arithmetic: gt/intel_ring.c, gt/intel_ring.h and gt/intel_ring_types.h. A [`Ring`]
//! is a model of `struct intel_ring` without the memory: it hands the caller the byte offset (and any wrap fill) for each
//! reservation and the caller does the CPU stores into the ring's mapping.
//!
//! Where Linux's `intel_ring_begin` would sleep in `wait_for_space` for completed requests to free room, this port refuses
//! with [`SubmitError::RingFull`]: the caller retires what the hardware has completed ([`Ring::set_head`]) and retries.
use crate::error::SubmitError;
use crate::lrc_reg::CACHELINE_BYTES;
use i915_gt_regs_core::{RING_NR_PAGES, RING_VALID};

/// `PAGE_SIZE` for `RING_CTL_SIZE` (intel_engine_regs.h:21).
const PAGE_SIZE: u32 = 4096;

/// `RING_CTL_SIZE(size)` (intel_engine_regs.h:21): bytes -> pages-minus-one, already positioned in the RING_CTL size field.
pub const fn ring_ctl_size(size: u32) -> u32 {
    size.wrapping_sub(PAGE_SIZE)
}

/// What [`Ring::begin`] decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Begin {
    /// Byte offset of the first reserved dword (`cs - ring->vaddr`).
    pub at: u32,
    /// `Some((offset, bytes))` when the tail of the ring was filled with `MI_NOOP` (qword zeros) so the packet starts at 0:
    /// the caller zeroes that span of the ring mapping (`memset64(ring->vaddr + ring->emit, 0, need_wrap / 8)`).
    pub wrap_fill: Option<(u32, u32)>,
}

/// `struct intel_ring` (intel_ring_types.h:22-48) without the vma, pin count and mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ring {
    head: u32,
    tail: u32,
    emit: u32,
    space: u32,
    size: u32,
    wrap: u32,
    effective_size: u32,
}

impl Ring {
    /// `intel_engine_create_ring` (intel_ring.c:146-182), minus the allocation: `size` must be a power of two whose
    /// `RING_CTL_SIZE` fits `RING_NR_PAGES` (4 KiB..=2 MiB). `effective_size == size` on every Gen9 part (the i830/i845
    /// erratum that trims it does not apply).
    pub fn new(size: u32) -> Result<Ring, SubmitError> {
        if !size.is_power_of_two() || size < PAGE_SIZE || ring_ctl_size(size) & !RING_NR_PAGES != 0 {
            return Err(SubmitError::BadRingSize { size }); // GEM_BUG_ON(!is_power_of_2(size)); GEM_BUG_ON(RING_CTL_SIZE(size) & ~RING_NR_PAGES)
        }
        let mut r = Ring {
            head: 0,
            tail: 0,
            emit: 0,
            space: 0,
            size,
            wrap: 32 - size.trailing_zeros(), // BITS_PER_TYPE(ring->size) - ilog2(size)
            effective_size: size,
        };
        r.update_space();
        Ok(r)
    }
    pub const fn size(&self) -> u32 {
        self.size
    }
    pub const fn head(&self) -> u32 {
        self.head
    }
    pub const fn tail(&self) -> u32 {
        self.tail
    }
    pub const fn emit(&self) -> u32 {
        self.emit
    }
    /// The cached free space (`ring->space`): refreshed by [`Ring::update_space`], NOT on every head move, as in Linux.
    pub const fn space(&self) -> u32 {
        self.space
    }
    pub const fn wrap_shift(&self) -> u32 {
        self.wrap
    }

    /// `RING_CTL_SIZE(ring->size) | RING_VALID`, the value `lrc_update_regs` stores in `CTX_RING_CTL` (intel_lrc.c:1550).
    pub const fn ctl_value(&self) -> u32 {
        ring_ctl_size(self.size) | RING_VALID
    }

    /// `__intel_ring_space` (intel_ring.h:129-139): `(head - tail - CACHELINE_BYTES) & (size - 1)`; the cacheline of slack keeps
    /// the tail out of the head's cacheline.
    pub const fn space_between(head: u32, tail: u32, size: u32) -> u32 {
        head.wrapping_sub(tail).wrapping_sub(CACHELINE_BYTES) & (size - 1)
    }

    /// `intel_ring_update_space` (intel_ring.c:19-27).
    pub fn update_space(&mut self) -> u32 {
        self.space = Ring::space_between(self.head, self.emit, self.size);
        self.space
    }

    /// `intel_ring_wrap` (intel_ring.h:54-57).
    pub const fn wrap(&self, pos: u32) -> u32 {
        pos & (self.size - 1)
    }

    /// `intel_ring_direction` (intel_ring.h:59-65): positive when `next` is ahead of `prev` by less than half the ring.
    pub const fn direction(&self, next: u32, prev: u32) -> i32 {
        (next.wrapping_sub(prev) << self.wrap) as i32
    }

    /// `intel_ring_offset_valid` (intel_ring.h:67-78): strictly inside the ring and qword aligned.
    pub const fn offset_valid(&self, pos: u32) -> bool {
        pos & self.size.wrapping_neg() == 0 && pos % 8 == 0
    }

    /// `intel_ring_reset` (intel_ring.c:85-92).
    pub fn reset(&mut self, tail: u32) {
        let tail = self.wrap(tail);
        self.tail = tail;
        self.head = tail;
        self.emit = tail;
        self.update_space();
    }

    /// `rq->ring->head = rq->postfix` in `i915_request_retire` (i915_request.c:385): the last known GPU position.
    pub fn set_head(&mut self, head: u32) -> Result<(), SubmitError> {
        if !self.offset_valid(head) {
            return Err(SubmitError::RingOffsetInvalid { pos: head, size: self.size });
        }
        self.head = head;
        Ok(())
    }

    /// `assert_ring_tail_valid` (intel_ring.h:89-113): refuse a tail that is out of the ring, unaligned, or that would advance
    /// into the cacheline of the last known head from behind it.
    pub fn check_tail(&self, tail: u32) -> Result<(), SubmitError> {
        if !self.offset_valid(tail) {
            return Err(SubmitError::RingOffsetInvalid { pos: tail, size: self.size });
        }
        let cl = |a: u32| a & !(CACHELINE_BYTES - 1);
        if cl(tail) == cl(self.head) && tail < self.head {
            return Err(SubmitError::RingTailInvalid { tail, head: self.head });
        }
        Ok(())
    }

    /// `intel_ring_set_tail` (intel_ring.h:115-127): validate, then record the tail the hardware will be told.
    pub fn set_tail(&mut self, tail: u32) -> Result<u32, SubmitError> {
        self.check_tail(tail)?;
        self.tail = tail;
        Ok(tail)
    }

    /// `intel_ring_begin` (intel_ring.c:230-309): reserve `num_dwords` (even) plus the request's `reserved_space` bytes.
    /// Returns where the packet goes and whether the tail of the ring must be NOOP-filled first. Refuses with
    /// [`SubmitError::RingFull`] where Linux would `wait_for_space`.
    pub fn begin(&mut self, num_dwords: u32, reserved_space: u32) -> Result<Begin, SubmitError> {
        if num_dwords & 1 != 0 {
            return Err(SubmitError::RingOddDwords { dwords: num_dwords }); // GEM_BUG_ON(num_dwords & 1)
        }
        let remain_usable = self.effective_size.wrapping_sub(self.emit);
        let bytes = num_dwords * 4;
        let mut need_wrap = 0u32;
        let mut total_bytes = bytes + reserved_space;
        if total_bytes > self.effective_size {
            return Err(SubmitError::RingRequestTooLarge { need: total_bytes, effective: self.effective_size }); // GEM_BUG_ON(total_bytes > ring->effective_size)
        }
        if total_bytes > remain_usable {
            let remain_actual = self.size - self.emit;
            if bytes > remain_usable {
                // Not enough room for the basic request: flush out the remainder, then wait for base + reserved.
                total_bytes += remain_actual;
                need_wrap = remain_actual | 1;
            } else {
                // The base request fits but the reserved space falls off the end.
                total_bytes = reserved_space + remain_actual;
            }
        }
        if total_bytes > self.space {
            // wait_for_space (intel_ring.c:192-228) first refreshes `space` from the head, then waits for retirement.
            self.update_space();
            if total_bytes > self.space {
                return Err(SubmitError::RingFull { need: total_bytes, space: self.space });
            }
        }
        let mut wrap_fill = None;
        if need_wrap != 0 {
            let n = need_wrap & !1;
            wrap_fill = Some((self.emit, n));
            self.space -= n;
            self.emit = 0;
        }
        let at = self.emit;
        self.emit += bytes;
        self.space -= bytes;
        Ok(Begin { at, wrap_fill })
    }

    /// `intel_ring_offset` (intel_ring.h:80-87): the ring offset of `pos`, never `size` (it would hang some GPUs): size -> 0.
    pub const fn offset(&self, pos: u32) -> u32 {
        self.wrap(pos)
    }
}
