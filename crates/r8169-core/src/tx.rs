// SPDX-License-Identifier: GPL-2.0-only
//! The transmit path — the order in which a packet becomes the NIC's to send.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * `rtl8169_start_xmit`'s publication sequence (:4590-:4606)
//!   * the descriptor ownership bits (:577-:582)
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! THIS MODULE IS ABOUT ORDER, NOT ARITHMETIC. Everything here would "work" in any sequence on a
//! single-threaded machine with no DMA. It is wrong in exactly the situations that are hardest to
//! reproduce and most damaging: a NIC reading a descriptor chain the instant it is released, and a
//! second CPU running the completion path against a tail pointer that moved too early.

/// `DescOwn` (:578) — the descriptor belongs to the NIC. Setting it is a RELEASE, not a flag.
pub const DESC_OWN: u32 = 1 << 31;
/// `RingEnd` (:579) — the last descriptor of the ring, so the NIC wraps.
pub const RING_END: u32 = 1 << 30;
/// `FirstFrag` (:580) / `LastFrag` (:581).
pub const FIRST_FRAG: u32 = 1 << 29;
pub const LAST_FRAG: u32 = 1 << 28;

/// One step of publishing a packet, in Linux's order (`rtl8169_start_xmit`, :4590-:4606).
///
/// Modelled as a sequence so the ORDER is assertable without a NIC or a second thread — the order
/// is the entire contract and it is invisible in any single value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishStep {
    /// `txd_last->opts1 |= LastFrag` (:4591). The chain gets its terminator BEFORE it is released.
    MarkLastFragment,
    /// `dma_wmb()` (:4597) — Linux: "Force memory writes to complete before releasing descriptor".
    /// This barrier orders the descriptor writes against THE DEVICE.
    BarrierBeforeRelease,
    /// `txd_first->opts1 |= DescOwn | FirstFrag` (:4601). ONE write, on the FIRST descriptor, and
    /// it is the release: from this instant the NIC may read the whole chain.
    ReleaseFirstDescriptor,
    /// `smp_wmb()` (:4604) — Linux: "rtl_tx needs to see descriptor changes before updated
    /// tp->cur_tx". A DIFFERENT barrier with a different job: it orders against ANOTHER CPU running
    /// the completion path, not against the device.
    BarrierBeforeTailUpdate,
    /// `WRITE_ONCE(tp->cur_tx, ...)` (:4606) — publish the new tail to the completion path.
    PublishTail,
    /// `rtl8169_doorbell` (:4612) — tell the NIC to look, if anything asked for it.
    Doorbell,
}

/// The publication sequence, in order.
///
/// WHY EACH EDGE MATTERS, since a reordering compiles and usually works:
///   * MarkLastFragment BEFORE ReleaseFirstDescriptor — release the head first and the NIC may walk
///     a chain whose terminator has not been written, transmitting past the end of the packet.
///   * BarrierBeforeRelease BETWEEN them — without it the release can become visible to the device
///     before the descriptor contents it depends on.
///   * PublishTail AFTER ReleaseFirstDescriptor, with its own barrier — the completion path reads
///     `cur_tx` to decide which descriptors to reclaim. A tail published early points the reaper at
///     a descriptor the NIC has not finished with.
///   * Doorbell LAST — it is an optimisation, not the release. The NIC already owns the chain.
pub const PUBLISH_SEQUENCE: [PublishStep; 6] = [
    PublishStep::MarkLastFragment,
    PublishStep::BarrierBeforeRelease,
    PublishStep::ReleaseFirstDescriptor,
    PublishStep::BarrierBeforeTailUpdate,
    PublishStep::PublishTail,
    PublishStep::Doorbell,
];

/// The opts1 word written to the FIRST descriptor when the packet is released (:4601).
///
/// `DescOwn` and `FirstFrag` go out TOGETHER in one OR. Splitting them into two writes opens a
/// window where the NIC owns a descriptor not yet marked as a packet's first fragment.
pub fn release_word(existing_opts1: u32) -> u32 {
    existing_opts1 | DESC_OWN | FIRST_FRAG
}

/// The opts1 word for the LAST descriptor of a chain (:4591) — `LastFrag`, and NOT `DescOwn`.
///
/// Only the FIRST descriptor is released. A port that sets DescOwn here too hands the NIC a
/// mid-chain descriptor independently of its head.
pub fn last_fragment_word(existing_opts1: u32) -> u32 {
    existing_opts1 | LAST_FRAG
}

/// Whether a descriptor is currently the NIC's.
///
/// The driver may not touch a descriptor while this holds; the RX path's own comment (:4782) says
/// the same thing from the other side — no field may be read until DescOwn has been checked.
pub fn is_owned_by_nic(opts1: u32) -> bool {
    opts1 & DESC_OWN != 0
}

/// A single-descriptor packet is both the first and the last fragment.
///
/// It is one descriptor, so the two markings collapse into the release word — but they must BOTH be
/// present, or the NIC sees a chain that never ends.
pub fn single_descriptor_word(existing_opts1: u32) -> u32 {
    release_word(existing_opts1) | LAST_FRAG
}
