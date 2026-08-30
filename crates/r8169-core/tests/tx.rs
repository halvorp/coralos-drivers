// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the transmit publication order. Expected values are LINUX literals with their lines.

use r8169_core::tx::{
    is_owned_by_nic, last_fragment_word, release_word, single_descriptor_word, PublishStep,
    DESC_OWN, FIRST_FRAG, LAST_FRAG, PUBLISH_SEQUENCE, RING_END,
};

/// r8169_main.c:578-:581.
#[test]
fn the_descriptor_ownership_bits_match_linux() {
    assert_eq!(DESC_OWN, 1 << 31);
    assert_eq!(RING_END, 1 << 30);
    assert_eq!(FIRST_FRAG, 1 << 29);
    assert_eq!(LAST_FRAG, 1 << 28);
    // Four distinct bits in the top nibble — none may alias another.
    let all = [DESC_OWN, RING_END, FIRST_FRAG, LAST_FRAG];
    for (i, a) in all.iter().enumerate() {
        assert_eq!(a.count_ones(), 1);
        for b in &all[i + 1..] {
            assert_eq!(a & b, 0, "{a:#x} and {b:#x} overlap");
        }
    }
}

/// THE ORDER IS THE CONTRACT (:4590-:4612). Every edge below is invisible in any single value and
/// compiles fine reordered.
#[test]
fn the_publication_sequence_is_linuxs_order() {
    use PublishStep::*;
    assert_eq!(
        PUBLISH_SEQUENCE,
        [
            MarkLastFragment,
            BarrierBeforeRelease,
            ReleaseFirstDescriptor,
            BarrierBeforeTailUpdate,
            PublishTail,
            Doorbell,
        ]
    );
}

/// The terminator is written BEFORE the release (:4591 before :4601). Release the head first and
/// the NIC may walk a chain whose end has not been marked, transmitting past the packet.
#[test]
fn the_chain_is_terminated_before_it_is_released() {
    let last = PUBLISH_SEQUENCE.iter().position(|s| *s == PublishStep::MarkLastFragment).unwrap();
    let rel = PUBLISH_SEQUENCE.iter().position(|s| *s == PublishStep::ReleaseFirstDescriptor).unwrap();
    assert!(last < rel, "LastFrag must be written before DescOwn");
}

/// TWO barriers, TWO jobs. `dma_wmb` (:4597) orders the descriptor writes against THE DEVICE;
/// `smp_wmb` (:4604) orders them against ANOTHER CPU in the completion path. Each sits immediately
/// before the publication it protects, and collapsing them into one loses a guarantee.
#[test]
fn each_barrier_immediately_precedes_the_publication_it_protects() {
    let at = |s| PUBLISH_SEQUENCE.iter().position(|x| *x == s).unwrap();
    assert_eq!(at(PublishStep::BarrierBeforeRelease) + 1, at(PublishStep::ReleaseFirstDescriptor));
    assert_eq!(at(PublishStep::BarrierBeforeTailUpdate) + 1, at(PublishStep::PublishTail));
    // And they are DISTINCT steps — one barrier cannot serve both, they order against different
    // observers.
    assert_ne!(PublishStep::BarrierBeforeRelease, PublishStep::BarrierBeforeTailUpdate);
}

/// The tail is published AFTER the release (:4606 after :4601). The completion path reads it to
/// decide what to reclaim; a tail published early points the reaper at a live descriptor.
#[test]
fn the_tail_is_published_after_the_release_and_the_doorbell_is_last() {
    let at = |s| PUBLISH_SEQUENCE.iter().position(|x| *x == s).unwrap();
    assert!(at(PublishStep::ReleaseFirstDescriptor) < at(PublishStep::PublishTail));
    assert_eq!(at(PublishStep::Doorbell), PUBLISH_SEQUENCE.len() - 1,
               "the doorbell is an optimisation, not the release — the NIC already owns the chain");
}

/// :4601 — DescOwn and FirstFrag go out TOGETHER in one OR. Splitting them opens a window where the
/// NIC owns a descriptor not yet marked as a packet's first fragment.
#[test]
fn the_release_word_carries_ownership_and_first_fragment_together() {
    let w = release_word(0x1234);
    assert_eq!(w & DESC_OWN, DESC_OWN);
    assert_eq!(w & FIRST_FRAG, FIRST_FRAG);
    assert_eq!(w & 0xffff, 0x1234, "the existing opts1 survives");
    assert_eq!(w & LAST_FRAG, 0, "a multi-descriptor head is not the last fragment");
}

/// :4591 — the LAST descriptor gets LastFrag and NOT DescOwn. Only the FIRST is released; setting
/// ownership here hands the NIC a mid-chain descriptor independently of its head.
#[test]
fn only_the_first_descriptor_is_released_to_the_nic() {
    let w = last_fragment_word(0x99);
    assert_eq!(w & LAST_FRAG, LAST_FRAG);
    assert_eq!(w & DESC_OWN, 0, "the tail must NOT be released on its own");
    assert!(!is_owned_by_nic(w));
    assert!(is_owned_by_nic(release_word(0)));
}

/// A one-descriptor packet is both ends of its own chain — both markings must be present, or the
/// NIC sees a chain that never terminates.
#[test]
fn a_single_descriptor_packet_is_both_first_and_last() {
    let w = single_descriptor_word(0);
    assert_eq!(w & (DESC_OWN | FIRST_FRAG | LAST_FRAG), DESC_OWN | FIRST_FRAG | LAST_FRAG);
}
