// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the ported RX/TX ring bookkeeping.
//!
//! The ring has no registers — the whole protocol is the `DescOwn` bit and where `RingEnd` sits, so
//! these assert the ARITHMETIC and the OWNERSHIP transitions against the Linux source.

use r8169_core::desc::{self, DESC_OWN, RING_END};
use r8169_core::ring;

/// NUM_TX_DESC / NUM_RX_DESC, r8169_main.c:73-74.
#[test]
fn the_ring_sizes_match_linux() {
    assert_eq!(ring::NUM_TX_DESC, 256);
    assert_eq!(ring::NUM_RX_DESC, 256);
}

/// `tp->cur_rx % NUM_RX_DESC` (r8169_main.c:4770). The counter is free-running; only its USE is
/// wrapped.
#[test]
fn the_index_is_the_free_running_counter_modulo_the_ring() {
    assert_eq!(ring::rx_index(0), 0);
    assert_eq!(ring::rx_index(255), 255);
    assert_eq!(ring::rx_index(256), 0, "wraps at the ring size");
    assert_eq!(ring::rx_index(257), 1);
    assert_eq!(ring::rx_index(1_000_000), 1_000_000 % 256);
    assert_eq!(ring::tx_index(256), 0);
}

/// Keeping the counters free-running is what makes `cur - dirty` the outstanding count even ACROSS a
/// wrap. Wrapping the counters themselves would break this, which is why the port reproduces
/// Linux's scheme instead of "simplifying" it.
#[test]
fn outstanding_survives_a_counter_wrap() {
    assert_eq!(ring::outstanding(10, 4), 6);
    assert_eq!(ring::outstanding(258, 254), 4, "spans the ring boundary");
    assert_eq!(ring::outstanding(0, u32::MAX - 3), 4, "spans the u32 wrap");
}

/// Only the LAST descriptor carries RingEnd (r8169_main.c:4210). Every other entry must not.
#[test]
fn only_the_final_descriptor_is_marked_as_the_ring_end() {
    assert!(ring::is_last_rx(255));
    assert!(!ring::is_last_rx(254));
    assert!(!ring::is_last_rx(0));
    assert_eq!(ring::rx_initial_opts1(255), RING_END);
    assert_eq!(ring::rx_initial_opts1(254), 0);
    assert_eq!(ring::rx_initial_opts1(0), 0);
    // Exactly one entry in a full ring is the end.
    let ends = (0..ring::NUM_RX_DESC).filter(|i| ring::is_last_rx(*i)).count();
    assert_eq!(ends, 1, "exactly one RingEnd in the ring");
}

/// The handshake: one word carrying DescOwn, the preserved RingEnd, and the buffer size together.
/// (rtl8169_mark_to_asic, r8169_main.c:4144-4152.) The NIC may act the instant DescOwn appears, so
/// these must become visible together — never in two steps.
#[test]
fn handing_over_writes_own_ringend_and_length_as_one_word() {
    let last = ring::rx_hand_to_nic_opts1(RING_END);
    assert_eq!(last & DESC_OWN, DESC_OWN, "handed to the NIC");
    assert_eq!(last & RING_END, RING_END, "RingEnd preserved on the final descriptor");
    assert_eq!(last & desc::OPTS1_LEN_MASK, desc::RX_BUF_SIZE, "full 16383-byte buffer advertised");

    let middle = ring::rx_hand_to_nic_opts1(0);
    assert_eq!(middle & RING_END, 0, "a middle descriptor must not gain RingEnd");
    assert_eq!(middle & desc::OPTS1_LEN_MASK, desc::RX_BUF_SIZE);
}

/// The ownership rule the whole ring rests on: the driver must not touch a descriptor the NIC owns.
#[test]
fn the_driver_may_only_touch_descriptors_the_nic_has_returned() {
    assert!(!ring::driver_may_touch(DESC_OWN), "NIC-owned: hands off");
    assert!(!ring::driver_may_touch(DESC_OWN | RING_END | 1500));
    assert!(ring::driver_may_touch(0), "returned by the NIC");
    assert!(ring::driver_may_touch(RING_END | 1500), "returned, with status bits set");
}

/// A full lap: fill, hand every descriptor over, and confirm the ring is still well-formed — the
/// last entry still the only RingEnd, every entry owned by the NIC, every one advertising the full
/// buffer. A truncating length mask or a lost RingEnd shows up here as a whole-ring property.
#[test]
fn a_full_ring_stays_well_formed_after_every_descriptor_is_handed_over() {
    let mut ring_mem: Vec<u32> =
        (0..ring::NUM_RX_DESC).map(ring::rx_initial_opts1).collect();
    for slot in ring_mem.iter_mut() {
        *slot = ring::rx_hand_to_nic_opts1(*slot);
    }
    assert!(ring_mem.iter().all(|o| o & DESC_OWN != 0), "all owned by the NIC");
    let ends: Vec<usize> = ring_mem
        .iter()
        .enumerate()
        .filter(|(_, o)| *o & RING_END != 0)
        .map(|(i, _)| i)
        .collect();
    assert_eq!(ends, vec![255], "exactly one RingEnd, and it is the last entry");
    assert!(
        ring_mem.iter().all(|o| o & desc::OPTS1_LEN_MASK == desc::RX_BUF_SIZE),
        "every descriptor advertises the full buffer — a narrow length mask fails here"
    );
}
