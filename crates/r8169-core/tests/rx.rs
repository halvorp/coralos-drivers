// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the receive path. Expected values are LINUX literals with their lines.

use r8169_core::rx::{
    error_counts, is_fragmented, packet_len, verdict, RxErrorCounts, RxVerdict, ETH_FCS_LEN,
    FIRST_FRAG, LAST_FRAG, LEN_MASK, RX_CRC, RX_RES, RX_RUNT, RX_RWT,
};

/// A whole, clean frame: both fragment bits, no error, some length.
const GOOD: u32 = FIRST_FRAG | LAST_FRAG | 100;

/// r8169_main.c:468-:471, :581-:582, and if_ether.h:38.
#[test]
fn the_status_bits_match_linux() {
    assert_eq!(RX_RWT, 1 << 22);
    assert_eq!(RX_RES, 1 << 21);
    assert_eq!(RX_RUNT, 1 << 20);
    assert_eq!(RX_CRC, 1 << 19);
    assert_eq!(FIRST_FRAG, 1 << 29);
    assert_eq!(LAST_FRAG, 1 << 28);
    assert_eq!(ETH_FCS_LEN, 4);
    assert_eq!(LEN_MASK, 0x3fff, "GENMASK(13, 0) — FOURTEEN bits");
}

/// :4749-:4752 — fragmented unless BOTH bits are present. The test is against the PAIR.
#[test]
fn a_frame_is_fragmented_unless_it_is_both_first_and_last() {
    assert!(!is_fragmented(FIRST_FRAG | LAST_FRAG));
    assert!(is_fragmented(FIRST_FRAG), "first without last is a fragment");
    assert!(is_fragmented(LAST_FRAG), "last without first is a fragment");
    assert!(is_fragmented(0), "neither is a fragment");
}

/// :4804-:4806 — fourteen bits, less the FCS unless the caller keeps it.
#[test]
fn the_length_excludes_the_frame_check_sequence_by_default() {
    assert_eq!(packet_len(1514, false), 1510, "1514 on the wire is 1510 of payload");
    assert_eq!(packet_len(1514, true), 1514, "NETIF_F_RXFCS keeps it");
    // Only the low fourteen bits are length; the flag bits above must not leak in.
    assert_eq!(packet_len(FIRST_FRAG | LAST_FRAG | 64, false), 60);
    assert_eq!(packet_len(0x3fff, true), 0x3fff, "the mask's maximum");
    // A length shorter than the FCS must not wrap to 65533.
    assert_eq!(packet_len(2, false), 0);
}

/// :4791-:4795 — a descriptor can bump BOTH counters. RWT and RUNT are both LENGTH errors.
#[test]
fn one_descriptor_can_be_both_a_length_and_a_crc_error() {
    assert_eq!(error_counts(RX_RUNT | RX_CRC),
               RxErrorCounts { length_errors: 1, crc_errors: 1 });
    assert_eq!(error_counts(RX_RWT), RxErrorCounts { length_errors: 1, crc_errors: 0 });
    assert_eq!(error_counts(RX_RUNT), RxErrorCounts { length_errors: 1, crc_errors: 0 });
    assert_eq!(error_counts(RX_CRC), RxErrorCounts { length_errors: 0, crc_errors: 1 });
    assert_eq!(error_counts(0), RxErrorCounts::default());
}

/// :4798-:4799 — without RXALL, ANY flagged error drops the frame.
#[test]
fn without_rxall_any_error_drops_the_frame() {
    assert_eq!(verdict(GOOD, false), RxVerdict::Deliver);
    for bit in [RX_RUNT, RX_CRC, RX_RWT, 0] {
        assert_eq!(verdict(GOOD | RX_RES | bit, false), RxVerdict::Drop,
                   "RxRES with extra bit {bit:#x} must drop when RXALL is off");
    }
}

/// THE DOUBLE NEGATIVE, :4800. Under RXALL a bad frame is delivered ONLY when the fault is
/// specifically a runt or a bad checksum AND NOT a truncation. Inverting this clause — the easy
/// mistake, because it reads like a guard rather than a permission — delivers exactly the frames
/// Linux refuses to.
#[test]
fn under_rxall_only_a_classified_non_truncated_error_is_delivered() {
    assert_eq!(verdict(GOOD | RX_RES | RX_RUNT, true), RxVerdict::Deliver, "a runt is classified");
    assert_eq!(verdict(GOOD | RX_RES | RX_CRC, true), RxVerdict::Deliver, "a CRC error too");
    assert_eq!(verdict(GOOD | RX_RES | RX_RUNT | RX_CRC, true), RxVerdict::Deliver);
    // A watchdog timeout is INCOMPLETE — dropped even alongside a classified fault.
    assert_eq!(verdict(GOOD | RX_RES | RX_RWT, true), RxVerdict::Drop);
    assert_eq!(verdict(GOOD | RX_RES | RX_RWT | RX_CRC, true), RxVerdict::Drop,
               "RWT drops it even though the CRC bit would otherwise permit delivery");
    // An UNCLASSIFIED error: RxRES with nothing saying what is wrong.
    assert_eq!(verdict(GOOD | RX_RES, true), RxVerdict::Drop,
               "'something is wrong but we cannot say what' is not a frame anyone asked for");
}

/// :4810 — a fragment is dropped regardless of RXALL and regardless of any error bits, because the
/// driver has no way to reassemble it.
#[test]
fn a_fragment_is_dropped_whatever_else_is_true() {
    assert_eq!(verdict(FIRST_FRAG | 100, false), RxVerdict::Drop);
    assert_eq!(verdict(FIRST_FRAG | 100, true), RxVerdict::Drop);
    assert_eq!(verdict(LAST_FRAG | RX_RES | RX_CRC | 100, true), RxVerdict::Drop,
               "even the delivery-permitting error case cannot rescue a fragment");
}
