// SPDX-License-Identifier: GPL-2.0-only
//! Ring-state vectors from Linux `drivers/usb/host/xhci-ring.c`.
//! Copyright (C) 2008 Intel Corp., Sarah Sharp, and the Linux xHCI authors.

use xhci_ring_core::ring::*;
use xhci_ring_core::trb::{encode_type, Trb, TrbType, CHAIN, CYCLE, LINK_TOGGLE};

const fn pos(segment: usize, trb: usize) -> Position {
    Position { segment, trb }
}

/// xhci-ring.c:112-:121, :165-:179. Slot 255 is Link and follows to the next segment, wrapping at
/// the final segment. Ordinary slots increment without changing segment.
#[test]
fn position_math_uses_slot_255_as_the_link_slot() {
    assert!(!last_trb_on_segment(pos(0, 254)));
    assert!(last_trb_on_segment(pos(0, 255)));
    assert_eq!(last_trb_on_ring(pos(0, 255), 2), Ok(false));
    assert_eq!(last_trb_on_ring(pos(1, 255), 2), Ok(true));
    assert_eq!(next_trb(pos(0, 17), 2), Ok(pos(0, 18)));
    assert_eq!(next_trb(pos(0, 255), 2), Ok(pos(1, 0)));
    assert_eq!(next_trb(pos(1, 255), 2), Ok(pos(0, 0)));
}

/// xhci-ring.c:185-:203. Event rings have no Link TRBs: CCS remains stable at segment boundaries
/// and toggles only when the final segment wraps to the first.
#[test]
fn event_dequeue_cycle_toggles_only_on_whole_ring_wrap() {
    assert_eq!(advance_event_dequeue(pos(0, 17), 2, false), Ok((pos(0, 18), false)));
    assert_eq!(advance_event_dequeue(pos(0, 255), 2, false), Ok((pos(1, 0), false)));
    assert_eq!(advance_event_dequeue(pos(1, 255), 2, false), Ok((pos(0, 0), true)));
    assert_eq!(advance_event_dequeue(pos(1, 255), 2, true), Ok((pos(0, 0), false)));
}

/// THE LOAD-BEARING LINK RULE, xhci-ring.c:247-:261. Linux sets/clears CHAIN as requested, flips
/// the Link's CYCLE bit to hand it to hardware, and then toggles PCS iff LINK_TOGGLE is set. The
/// toggle decision therefore observes the control word after cycle flip, although the bits differ.
#[test]
fn link_advance_flips_link_cycle_and_toggle_flips_producer_cycle() {
    let link = encode_type(TrbType::Link as u8) | LINK_TOGGLE;
    let got = advance_enqueue_link(link, pos(1, 255), 2, false, true, false).unwrap();
    assert_eq!(got.link_control, 0x1813, "type 6 | TC 0x2 | CHAIN 0x10 | CYCLE 0x1");
    assert_eq!(got.next, pos(0, 0));
    assert!(got.cycle_state, "TC toggles PCS at the last ring Link");

    // Following that same Link on its next producer lap flips only Link CYCLE back to zero and
    // toggles PCS back. Confusing Link CYCLE with TC would fail this vector.
    let got = advance_enqueue_link(got.link_control, pos(1, 255), 2, true, false, false).unwrap();
    assert_eq!(got.link_control, 0x1802, "CHAIN cleared, Link CYCLE flipped 1 -> 0, TC retained");
    assert!(!got.cycle_state);
}

/// xhci-ring.c:256-:258. A Link without TC still has its own cycle bit flipped, but MUST NOT change
/// ring PCS. This pins the distinction that has caused prior Link TRB failures.
#[test]
fn link_cycle_flip_without_toggle_bit_does_not_flip_ring_cycle() {
    let link = encode_type(TrbType::Link as u8);
    let got = advance_enqueue_link(link, pos(0, 255), 2, false, false, false).unwrap();
    assert_eq!(got.link_control, 0x1801);
    assert_eq!(got.cycle_state, false);
    assert_eq!(got.next, pos(1, 0));
}

/// xhci-ring.c:239-:249. Old 0.95 controllers preserve the initialized Link-chain bit; all others
/// copy the preceding ordinary TRB's chain state.
#[test]
fn link_chain_quirk_preserves_chain_while_normal_path_replaces_it() {
    let chained = encode_type(TrbType::Link as u8) | CHAIN;
    assert_eq!(
        advance_enqueue_link(chained, pos(0, 255), 2, false, false, true).unwrap().link_control,
        0x1811,
        "quirk preserves CHAIN and producer flips CYCLE"
    );
    assert_eq!(
        advance_enqueue_link(chained, pos(0, 255), 2, false, false, false).unwrap().link_control,
        0x1801,
        "normal path clears CHAIN when TD ended"
    );
}

/// xhci-ring.c:283-:304. After an ordinary TRB, enqueue may remain on Link when a TD ended and no
/// more TRBs are promised. CHAIN or `more_trbs_coming` forces immediate Link traversal.
#[test]
fn ordinary_enqueue_advance_only_crosses_link_when_linux_requires_it() {
    let link = encode_type(TrbType::Link as u8) | LINK_TOGGLE;
    assert_eq!(
        advance_enqueue(pos(0, 10), 0, None, 2, false, false, false).unwrap(),
        EnqueueAdvance { next: pos(0, 11), cycle_state: false, passed_link_control: None }
    );
    assert_eq!(
        advance_enqueue(pos(0, 254), 0, None, 2, false, false, false).unwrap(),
        EnqueueAdvance { next: pos(0, 255), cycle_state: false, passed_link_control: None },
        "ended TD may leave enqueue on Link"
    );
    assert_eq!(
        advance_enqueue(pos(0, 254), 0, Some(link), 2, false, true, false).unwrap(),
        EnqueueAdvance { next: pos(1, 0), cycle_state: true, passed_link_control: Some(0x1803) },
        "more TRBs crosses Link, flips Link CYCLE, and TC flips PCS"
    );
    assert_eq!(
        advance_enqueue(pos(0, 254), CHAIN, Some(link), 2, false, false, false).unwrap(),
        EnqueueAdvance { next: pos(1, 0), cycle_state: true, passed_link_control: Some(0x1813) },
        "middle of TD propagates CHAIN onto Link"
    );
    assert_eq!(
        advance_enqueue(pos(0, 254), CHAIN, None, 2, false, false, false),
        Err(RingError::MissingLinkControl { segment: 0, link_trb: 255 })
    );
}

/// xhci-ring.c:148-:160. An ordinary no-op clears words 0..2 and all control except cycle/type. A
/// Link remains a Link and can only lose CHAIN.
#[test]
fn noop_conversion_preserves_only_the_linux_allowed_fields() {
    let ordinary = Trb { words: [1, 2, 3, 0xffff_ffff] };
    assert_eq!(
        to_noop(ordinary, TrbType::TransferNoop as u8, false).words,
        [0, 0, 0, 0x2001],
        "CYCLE 0x1 | TRB_TYPE(8) 0x2000"
    );
    let link = Trb { words: [0x1234, 0, 0, 0x1813] };
    assert_eq!(to_noop(link, 8, true).words, [0x1234, 0, 0, 0x1803]);
    assert_eq!(to_noop(link, 8, false).words, [0x1234, 0, 0, 0x1813]);
}

/// xhci-ring.c:338-:370. Equal pointers mean EMPTY, not full, and every segment contributes 255
/// normal TRBs because slot 255 is Link. A Link-stuck enqueue normalizes to next segment slot zero.
#[test]
fn free_count_excludes_links_handles_wrap_and_empty_special_case() {
    assert_eq!(num_trbs_free(pos(0, 0), pos(0, 0), 2), Ok(510));
    assert_eq!(num_trbs_free(pos(0, 10), pos(0, 20), 2), Ok(10));
    assert_eq!(num_trbs_free(pos(1, 250), pos(0, 5), 2), Ok(10));
    assert_eq!(num_trbs_free(pos(0, 255), pos(1, 7), 2), Ok(7));
}

/// xhci-ring.c:19-:29. Full means enqueue++ equals dequeue. Equal current pointers are explicitly
/// empty and retain all 255 normal slots in a one-segment ring.
#[test]
fn ring_full_is_next_enqueue_equals_dequeue_not_current_equality() {
    assert_eq!(ring_full(pos(0, 10), pos(0, 11), 1), Ok(true));
    assert_eq!(ring_full(pos(0, 254), pos(0, 0), 1), Ok(true), "advance skips Link slot 255");
    assert_eq!(ring_full(pos(0, 10), pos(0, 10), 1), Ok(false), "equal means empty");
    assert_eq!(ring_full(pos(0, 10), pos(0, 12), 1), Ok(false));
}

/// xhci-ring.c:387-:405. Filling the current segment exactly counts as crossing one segment; each
/// additional group of 255 normal TRBs crosses another.
#[test]
fn expansion_math_counts_exact_segment_fill_as_crossing() {
    assert_eq!(segments_crossed(10, 244), Ok(0));
    assert_eq!(segments_crossed(10, 245), Ok(1), "trbs_past_seg == 0 still expands");
    assert_eq!(segments_crossed(10, 500), Ok(2));
    assert_eq!(segments_crossed(255, 0), Ok(1), "enqueue stuck on Link special geometry");
}

/// Named refusals: xhci-ring.c:204-:220 and :265-:267 warn rather than accepting malformed rings.
#[test]
fn malformed_positions_and_non_links_are_named_refusals() {
    assert_eq!(next_trb(pos(0, 0), 0), Err(RingError::SegmentCountZero));
    assert_eq!(
        next_trb(pos(2, 0), 2),
        Err(RingError::SegmentOutOfRange { segment: 2, segment_count: 2 })
    );
    assert_eq!(
        next_trb(pos(0, 256), 2),
        Err(RingError::TrbOutOfRange { trb: 256, trbs_per_segment: 256 })
    );
    assert_eq!(
        advance_enqueue_link(0x2000 | CYCLE, pos(0, 255), 1, false, false, false),
        Err(RingError::ExpectedLinkTrb { control: 0x2001, decoded_type: 8 })
    );
    assert_eq!(
        advance_enqueue_link(0x1800, pos(0, 254), 1, false, false, false),
        Err(RingError::ExpectedNormalTrb { trb: 254, link_trb: 255 })
    );
}
