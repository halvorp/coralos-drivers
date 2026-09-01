// SPDX-License-Identifier: GPL-2.0-only
//! Pure ring position, fullness, and Link TRB transitions, ported from Linux
//! `drivers/usb/host/xhci-ring.c:10-:40, :112-:125, :165-:306, :338-:412`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

use crate::trb::{self, Trb, CHAIN, CYCLE, LINK_TOGGLE, TRBS_PER_SEGMENT, USABLE_TRBS_PER_SEGMENT};

/// A position in a ring whose segments all have Linux's fixed 256-TRB geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub segment: usize,
    pub trb: usize,
}

/// State changed by following one Link TRB as a producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkAdvance {
    pub link_control: u32,
    pub next: Position,
    pub cycle_state: bool,
}

/// State after advancing past one ordinary enqueue TRB (xhci-ring.c:283-:304).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnqueueAdvance {
    pub next: Position,
    pub cycle_state: bool,
    /// Updated Link control when this call traversed one; `None` when enqueue stayed on the Link or
    /// moved to another ordinary TRB.
    pub passed_link_control: Option<u32>,
}

/// A ring arithmetic refusal that names the offending value and its bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    SegmentCountZero,
    SegmentOutOfRange { segment: usize, segment_count: usize },
    TrbOutOfRange { trb: usize, trbs_per_segment: usize },
    ExpectedLinkTrb { control: u32, decoded_type: u8 },
    ExpectedNormalTrb { trb: usize, link_trb: usize },
    MissingLinkControl { segment: usize, link_trb: usize },
    ArithmeticOverflow { operation: &'static str },
}

fn validate(position: Position, segment_count: usize) -> Result<(), RingError> {
    if segment_count == 0 {
        return Err(RingError::SegmentCountZero);
    }
    if position.segment >= segment_count {
        return Err(RingError::SegmentOutOfRange {
            segment: position.segment,
            segment_count,
        });
    }
    if position.trb >= TRBS_PER_SEGMENT {
        return Err(RingError::TrbOutOfRange {
            trb: position.trb,
            trbs_per_segment: TRBS_PER_SEGMENT,
        });
    }
    Ok(())
}

/// Whether this is the segment's last, Link-TRB slot (xhci-ring.c:112-:115).
pub const fn last_trb_on_segment(position: Position) -> bool {
    position.trb == TRBS_PER_SEGMENT - 1
}

/// Whether this is the Link TRB that returns to the first segment (xhci-ring.c:117-:121).
pub fn last_trb_on_ring(position: Position, segment_count: usize) -> Result<bool, RingError> {
    validate(position, segment_count)?;
    Ok(last_trb_on_segment(position) && position.segment + 1 == segment_count)
}

/// Position immediately after a TRB without skipping Link TRBs. At a Link/last slot this follows
/// the segment link (xhci-ring.c:165-:179).
pub fn next_trb(position: Position, segment_count: usize) -> Result<Position, RingError> {
    validate(position, segment_count)?;
    if last_trb_on_segment(position) {
        Ok(Position { segment: (position.segment + 1) % segment_count, trb: 0 })
    } else {
        Ok(Position { trb: position.trb + 1, ..position })
    }
}

/// Consumer event-ring advance. Event rings contain no Link TRBs; CCS toggles only when wrapping
/// from the last TRB of the last segment (xhci-ring.c:185-:203).
pub fn advance_event_dequeue(
    position: Position,
    segment_count: usize,
    cycle_state: bool,
) -> Result<(Position, bool), RingError> {
    let wraps = last_trb_on_ring(position, segment_count)?;
    Ok((next_trb(position, segment_count)?, cycle_state ^ wraps))
}

/// Update a Link TRB as Linux's producer does, then move to the next segment. The Link cycle bit is
/// flipped FIRST; producer cycle state changes iff the Link's toggle bit is set AFTER that flip
/// (xhci-ring.c:229-:263). `preserve_chain` models the 0.95 Link-chain quirk.
pub fn advance_enqueue_link(
    link_control: u32,
    position: Position,
    segment_count: usize,
    cycle_state: bool,
    chain: bool,
    preserve_chain: bool,
) -> Result<LinkAdvance, RingError> {
    validate(position, segment_count)?;
    if !last_trb_on_segment(position) {
        return Err(RingError::ExpectedNormalTrb {
            trb: position.trb,
            link_trb: TRBS_PER_SEGMENT - 1,
        });
    }
    if !trb::is_link(link_control) {
        return Err(RingError::ExpectedLinkTrb {
            control: link_control,
            decoded_type: trb::decode_type(link_control),
        });
    }

    let mut control = link_control;
    if !preserve_chain {
        control = (control & !CHAIN) | if chain { CHAIN } else { 0 };
    }
    control ^= CYCLE;
    let next_cycle = cycle_state ^ (control & LINK_TOGGLE != 0);

    Ok(LinkAdvance {
        link_control: control,
        next: Position { segment: (position.segment + 1) % segment_count, trb: 0 },
        cycle_state: next_cycle,
    })
}

/// Advance after queueing one ordinary TRB. Linux moves once, then traverses a following Link only
/// when the queued TRB had CHAIN or the caller says more TRBs are coming (xhci-ring.c:283-:304).
/// `following_link_control` is required only when that traversal occurs.
pub fn advance_enqueue(
    position: Position,
    queued_control: u32,
    following_link_control: Option<u32>,
    segment_count: usize,
    cycle_state: bool,
    more_trbs_coming: bool,
    preserve_link_chain: bool,
) -> Result<EnqueueAdvance, RingError> {
    validate(position, segment_count)?;
    if last_trb_on_segment(position) {
        return Err(RingError::ExpectedNormalTrb {
            trb: position.trb,
            link_trb: TRBS_PER_SEGMENT - 1,
        });
    }

    let next = Position { trb: position.trb + 1, ..position };
    let chain = queued_control & CHAIN != 0;
    if last_trb_on_segment(next) && (chain || more_trbs_coming) {
        let link_control = following_link_control.ok_or(RingError::MissingLinkControl {
            segment: next.segment,
            link_trb: next.trb,
        })?;
        let advanced = advance_enqueue_link(
            link_control,
            next,
            segment_count,
            cycle_state,
            chain,
            preserve_link_chain,
        )?;
        Ok(EnqueueAdvance {
            next: advanced.next,
            cycle_state: advanced.cycle_state,
            passed_link_control: Some(advanced.link_control),
        })
    } else {
        Ok(EnqueueAdvance { next, cycle_state, passed_link_control: None })
    }
}

/// Convert one ordinary TRB to a no-op exactly as `trb_to_noop`: clear words 0..2 and preserve
/// only cycle before inserting the requested no-op type (xhci-ring.c:148-:160). Link TRBs are not
/// rewritten; they are optionally unchained.
pub fn to_noop(mut trb_word: Trb, noop_type: u8, unchain_links: bool) -> Trb {
    if trb::is_link(trb_word.words[3]) {
        if unchain_links {
            trb_word.words[3] &= !CHAIN;
        }
    } else {
        trb_word.words[0] = 0;
        trb_word.words[1] = 0;
        trb_word.words[2] = 0;
        trb_word.words[3] = (trb_word.words[3] & CYCLE) | trb::encode_type(noop_type);
    }
    trb_word
}

/// Free normal TRBs between enqueue and dequeue, excluding every segment's Link slot. This is the
/// arithmetic form of `xhci_num_trbs_free` (xhci-ring.c:338-:370). As in Linux, equal positions
/// represent an empty ring and therefore all normal slots are free.
pub fn num_trbs_free(
    enqueue: Position,
    dequeue: Position,
    segment_count: usize,
) -> Result<usize, RingError> {
    validate(enqueue, segment_count)?;
    validate(dequeue, segment_count)?;

    let enqueue = normalize_link(enqueue, segment_count);
    if last_trb_on_segment(dequeue) {
        return Err(RingError::ExpectedNormalTrb {
            trb: dequeue.trb,
            link_trb: TRBS_PER_SEGMENT - 1,
        });
    }
    if enqueue == dequeue {
        return segment_count
            .checked_mul(USABLE_TRBS_PER_SEGMENT)
            .ok_or(RingError::ArithmeticOverflow { operation: "segment_count * usable_trbs_per_segment" });
    }

    let capacity = segment_count
        .checked_mul(USABLE_TRBS_PER_SEGMENT)
        .ok_or(RingError::ArithmeticOverflow { operation: "segment_count * usable_trbs_per_segment" })?;
    let enq = enqueue.segment * USABLE_TRBS_PER_SEGMENT + enqueue.trb;
    let deq = dequeue.segment * USABLE_TRBS_PER_SEGMENT + dequeue.trb;
    Ok(if deq >= enq { deq - enq } else { capacity - enq + deq })
}

/// Linux's ring-full rule: advancing enqueue once (skipping the Link slot) reaches dequeue
/// (xhci-ring.c:19-:29).
pub fn ring_full(
    enqueue: Position,
    dequeue: Position,
    segment_count: usize,
) -> Result<bool, RingError> {
    Ok(num_trbs_free(enqueue, dequeue, segment_count)? == 1)
}

/// Number of segments a queue operation would require beyond the current segment, before checking
/// whether those segments collide with dequeue (xhci-ring.c:374-:412). Filling the current segment
/// exactly already returns one, matching Linux's `trbs_past_seg == 0` special rule.
pub fn segments_crossed(enqueue_trb: usize, num_trbs: usize) -> Result<usize, RingError> {
    if enqueue_trb >= TRBS_PER_SEGMENT {
        return Err(RingError::TrbOutOfRange {
            trb: enqueue_trb,
            trbs_per_segment: TRBS_PER_SEGMENT,
        });
    }
    let end = enqueue_trb
        .checked_add(num_trbs)
        .ok_or(RingError::ArithmeticOverflow { operation: "enqueue_trb + num_trbs" })?;
    if end < USABLE_TRBS_PER_SEGMENT {
        Ok(0)
    } else {
        Ok(1 + (end - USABLE_TRBS_PER_SEGMENT) / USABLE_TRBS_PER_SEGMENT)
    }
}

fn normalize_link(position: Position, segment_count: usize) -> Position {
    if last_trb_on_segment(position) {
        Position { segment: (position.segment + 1) % segment_count, trb: 0 }
    } else {
        position
    }
}
