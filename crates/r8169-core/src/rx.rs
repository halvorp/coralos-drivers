// SPDX-License-Identifier: GPL-2.0-only
//! The receive path — deciding whether a descriptor's packet may be delivered, and how long it is.
//!
//! Ported from Linux `drivers/net/ethernet/realtek/r8169_main.c`:
//!   * the RxStatusDesc bits (:468-:471) and FirstFrag/LastFrag (:581-:582)
//!   * `rtl_rx`'s error handling (:4787-:4802) and length computation (:4804-:4806)
//!   * `rtl8169_fragmented_frame` (:4749-:4752)
//!
//! Copyright (c) a lot of people. See the Linux source for the full authorship of r8169.
//!
//! Every decision here is made from ONE descriptor status word. Get one of them wrong and the
//! driver either delivers corrupt frames to the stack or silently drops good ones, and in both
//! cases the NIC reports nothing at all.

/// `RxRWT` (:468) — receive watchdog timeout. The frame did not arrive completely.
pub const RX_RWT: u32 = 1 << 22;
/// `RxRES` (:469) — the descriptor carries an error of some kind. The other bits say which.
pub const RX_RES: u32 = 1 << 21;
/// `RxRUNT` (:470) — the frame was shorter than the minimum.
pub const RX_RUNT: u32 = 1 << 20;
/// `RxCRC` (:471) — the frame's checksum did not match.
pub const RX_CRC: u32 = 1 << 19;
/// `FirstFrag` (:581) / `LastFrag` (:582).
pub const FIRST_FRAG: u32 = 1 << 29;
pub const LAST_FRAG: u32 = 1 << 28;
/// `ETH_FCS_LEN` (include/uapi/linux/if_ether.h:38).
pub const ETH_FCS_LEN: u16 = 4;
/// `pkt_size = status & GENMASK(13, 0)` (:4804) — FOURTEEN bits, matching OPTS1_LEN_MASK.
pub const LEN_MASK: u32 = 0x3fff;

/// What the driver does with one received descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxVerdict {
    /// Deliver it to the stack.
    Deliver,
    /// Release the descriptor without delivering. Linux's `goto release_descriptor`.
    Drop,
}

/// Statistics one errored descriptor contributes (`rtl_rx`, :4791-:4795). A descriptor can raise
/// BOTH counters — a runt with a bad checksum is a length error and a CRC error, not a choice
/// between them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RxErrorCounts {
    pub length_errors: u32,
    pub crc_errors: u32,
}

/// The counters a descriptor bumps. `RxRWT` and `RxRUNT` are BOTH length errors (:4792).
pub fn error_counts(status: u32) -> RxErrorCounts {
    RxErrorCounts {
        length_errors: u32::from(status & (RX_RWT | RX_RUNT) != 0),
        crc_errors: u32::from(status & RX_CRC != 0),
    }
}

/// `rtl8169_fragmented_frame` (:4749) — a frame is fragmented unless it is BOTH the first and the
/// last segment.
///
/// The test is against the PAIR, not either bit: `(status & (First|Last)) != (First|Last)`. Testing
/// them separately, or with an `||`, accepts a descriptor carrying only one of them — a fragment
/// the driver has no way to reassemble, which Linux treats as a symptom of an over-MTU frame.
pub fn is_fragmented(status: u32) -> bool {
    status & (FIRST_FRAG | LAST_FRAG) != (FIRST_FRAG | LAST_FRAG)
}

/// The payload length (:4804-:4806).
///
/// Fourteen bits of the status word, LESS the four-byte frame check sequence unless the caller
/// asked to keep it (`NETIF_F_RXFCS`). Forgetting the subtraction hands the stack four bytes of
/// checksum as if they were payload — on every single packet.
pub fn packet_len(status: u32, keep_fcs: bool) -> u16 {
    let raw = (status & LEN_MASK) as u16;
    if keep_fcs {
        raw
    } else {
        raw.saturating_sub(ETH_FCS_LEN)
    }
}

/// Whether this descriptor's packet may be delivered (`rtl_rx`, :4787-:4802).
///
/// THE SECOND CONDITION IS A DOUBLE NEGATIVE AND IT IS THE WHOLE POINT. With `NETIF_F_RXALL` the
/// caller has asked for errored frames, and yet Linux STILL drops them when:
///   * `RxRWT` is set — the frame is incomplete, so there is nothing coherent to hand over; or
///   * NEITHER `RxRUNT` NOR `RxCRC` is set — the error is unclassified, and "something is wrong but
///     we cannot say what" is not a frame anyone asked for.
/// So a bad frame reaches the stack ONLY when the fault is specifically a runt or a bad checksum
/// and NOT a truncation. Inverting that last clause — the easy mistake, because it reads like a
/// guard rather than a permission — delivers exactly the frames Linux refuses to.
pub fn verdict(status: u32, rx_all: bool) -> RxVerdict {
    if status & RX_RES != 0 {
        if !rx_all {
            return RxVerdict::Drop;
        }
        if status & RX_RWT != 0 || status & (RX_RUNT | RX_CRC) == 0 {
            return RxVerdict::Drop;
        }
    }
    if is_fragmented(status) {
        return RxVerdict::Drop;
    }
    RxVerdict::Deliver
}
