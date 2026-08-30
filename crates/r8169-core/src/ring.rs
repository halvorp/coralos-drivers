// SPDX-License-Identifier: GPL-2.0-only
//! RX/TX descriptor ring bookkeeping, ported from Linux's r8169.
//!
//! The ring is memory the NIC walks by itself. Nothing here talks to a register: the whole protocol
//! is the `DescOwn` bit and the position of `RingEnd`, and getting either wrong produces a NIC that
//! reads the wrong memory rather than an error anyone can see.

use crate::desc::{self, DESC_OWN, RING_END};

/// Descriptors per ring. (r8169_main.c:73-74 — NUM_TX_DESC and NUM_RX_DESC, both 256.)
pub const NUM_TX_DESC: u32 = 256;
pub const NUM_RX_DESC: u32 = 256;

/// The ring index for a free-running counter: `tp->cur_rx % NUM_RX_DESC` (r8169_main.c:4770).
///
/// Linux keeps `cur_rx`/`cur_tx` free-running and takes them modulo the ring size at each use,
/// rather than wrapping the counter itself — so the difference between two counters is the number
/// of outstanding descriptors even across a wrap. Reproduced rather than "improved" for that reason.
pub fn rx_index(cur_rx: u32) -> u32 {
    cur_rx % NUM_RX_DESC
}
pub fn tx_index(cur_tx: u32) -> u32 {
    cur_tx % NUM_TX_DESC
}

/// Is this the LAST descriptor of the ring — the one that must carry `RingEnd`?
///
/// Linux marks it once at fill time: `tp->RxDescArray[NUM_RX_DESC - 1].opts1 |= cpu_to_le32(RingEnd)`
/// (r8169_main.c:4210). Without it the NIC walks past the end of the ring into memory it was never
/// given.
pub fn is_last_rx(index: u32) -> bool {
    index == NUM_RX_DESC - 1
}
pub fn is_last_tx(index: u32) -> bool {
    index == NUM_TX_DESC - 1
}

/// The `opts1` an RX descriptor is initialised with at ring-fill time: the ring-end flag on the last
/// entry and nothing on the others, before any buffer is handed over.
pub fn rx_initial_opts1(index: u32) -> u32 {
    if is_last_rx(index) { RING_END } else { 0 }
}

/// Hand an RX descriptor to the NIC with the driver's standard buffer size.
///
/// `rtl8169_mark_to_asic` (r8169_main.c:4144-4152) in full: read the existing `RingEnd` back, clear
/// `opts2`, barrier, then write `DescOwn | eor | R8169_RX_BUF_SIZE` as ONE word. The single word is
/// the handshake — the NIC may act the instant `DescOwn` appears, so length and flags must become
/// visible together with it, never in two steps.
///
/// The `opts2 = 0` and the `dma_wmb()` are the caller's to perform: this crate computes words, it
/// does not own the memory or the barrier. Named here because a port that silently drops the
/// barrier is a port that works until it does not.
pub fn rx_hand_to_nic_opts1(existing_opts1: u32) -> u32 {
    desc::rx_opts1_hand_to_nic(existing_opts1, desc::RX_BUF_SIZE)
}

/// How many descriptors are outstanding — issued to the NIC and not yet reclaimed.
/// `cur - dirty`, which is why both counters are free-running (see [`rx_index`]).
pub fn outstanding(cur: u32, dirty: u32) -> u32 {
    cur.wrapping_sub(dirty)
}

/// May the driver touch this descriptor? Only when the NIC has given it back.
pub fn driver_may_touch(opts1: u32) -> bool {
    opts1 & DESC_OWN == 0
}
