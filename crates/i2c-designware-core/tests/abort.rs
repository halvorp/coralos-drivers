// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for abort decoding. Expected values are LINUX literals with file and line.
//!
//! This module was the only one in the crate with no tests, which matters more here than it would
//! elsewhere: `abort.rs` is pure extracted data plus a decision order, and the crate's own header
//! makes the point that "a mistyped offset does not fail to compile — it drives a different
//! register on real silicon and reports nothing". The same is true of a dropped cause or a
//! reordered verdict check. These vectors are what makes either of those fail loudly.

use i2c_designware_core::abort::{
    causes_in, is_expected_traffic, undecoded, verdict, AbortVerdict, ABORT_CAUSES,
    CAPTURE_THEN_CLEAR,
};
use i2c_designware_core::regs::{bits, off};

/// Every cause in `abort_sources[]`, in Linux's own order, written out LITERALLY.
///
/// DELIBERATELY NOT DERIVED FROM `ABORT_CAUSES`. A list generated from the production table cannot
/// detect a deletion from that table — the test case disappears together with the thing it was
/// meant to guard. Verified against references/linux-ref/drivers/i2c/busses/i2c-designware-common.c
/// on 2026-08-31: that array has exactly these fourteen entries, in this order.
const LINUX_ABORT_SOURCE_NAMES: [&str; 14] = [
    "7B_ADDR_NOACK",
    "10ADDR1_NOACK",
    "10ADDR2_NOACK",
    "TXDATA_NOACK",
    "GCALL_NOACK",
    "GCALL_READ",
    "SBYTE_ACKDET",
    "SBYTE_NORSTRT",
    "10B_RD_NORSTRT",
    "MASTER_DIS",
    "ARB_LOST",
    "SLAVE_FLUSH_TXFIFO",
    "SLAVE_ARBLOST",
    "SLAVE_RD_INTX",
];

/// The extraction carries every cause Linux defines — checked by NAME, not merely by count.
///
/// A count-only assertion passes when one cause is swapped for a duplicate of another, which is
/// exactly the shape a copy-paste extraction error takes.
#[test]
fn every_linux_abort_source_is_present_by_name() {
    let ours: Vec<&str> = ABORT_CAUSES.iter().map(|c| c.name).collect();
    let missing: Vec<&&str> = LINUX_ABORT_SOURCE_NAMES
        .iter()
        .filter(|n| !ours.contains(n))
        .collect();
    assert!(missing.is_empty(), "causes missing from ABORT_CAUSES: {missing:?}");
    assert_eq!(
        ours, LINUX_ABORT_SOURCE_NAMES,
        "ABORT_CAUSES must carry Linux's causes in Linux's order"
    );
}

/// The specific near-miss the module header records.
///
/// `ARB_LOST` is the ONLY cause Linux names without the `ABRT_` prefix. An extraction keyed off
/// that prefix yields thirteen causes and silently loses arbitration loss, which then decodes as
/// an unknown bit. This test fails if that ever happens again.
#[test]
fn arb_lost_survives_despite_lacking_the_abrt_prefix() {
    let arb = ABORT_CAUSES
        .iter()
        .find(|c| c.name == "ARB_LOST")
        .expect("ARB_LOST dropped — a prefix-keyed extraction yields 13 causes and loses it");
    assert_eq!(arb.bit, 12, "core.h:176 puts ARB_LOST at bit 12");
    assert_eq!(1u32 << arb.bit, bits::TX_ARB_LOST, "bit must agree with regs::bits::TX_ARB_LOST");
}

/// Bits ascend and never repeat.
///
/// A duplicated bit would make one cause unreachable through `causes_in` while the table still
/// looked complete by count.
#[test]
fn bits_are_ascending_and_unique() {
    let bits_seq: Vec<u8> = ABORT_CAUSES.iter().map(|c| c.bit).collect();
    let mut sorted = bits_seq.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(bits_seq, sorted, "ABORT_CAUSES must be in strict ascending bit order");
    assert!(bits_seq.iter().all(|&b| b < 32), "TX_ABRT_SOURCE is a u32");
}

/// Bits 6 and 8 are absent BECAUSE Linux leaves them undefined, not by oversight.
///
/// Pinned so a future "fill in the gaps" edit has to justify itself against this comment.
#[test]
fn bits_six_and_eight_are_deliberately_undefined() {
    for gap in [6u8, 8u8] {
        assert!(
            !ABORT_CAUSES.iter().any(|c| c.bit == gap),
            "bit {gap} is undefined in Linux; it must not acquire a cause"
        );
        assert_eq!(
            undecoded(1 << gap),
            1 << gap,
            "an undefined bit must survive as undecoded rather than vanish"
        );
    }
}

/// `causes_in` reports exactly the causes whose bits are set, in bit order.
#[test]
fn causes_in_decodes_exactly_the_set_bits() {
    let word = (1 << 0) | (1 << 12) | (1 << 15);
    let names: Vec<&str> = causes_in(word).map(|c| c.name).collect();
    assert_eq!(names, ["7B_ADDR_NOACK", "ARB_LOST", "SLAVE_RD_INTX"]);

    assert_eq!(causes_in(0).count(), 0, "a zero word decodes to no causes");
    let all: u32 = ABORT_CAUSES.iter().map(|c| 1u32 << c.bit).fold(0, |a, b| a | b);
    assert_eq!(causes_in(all).count(), ABORT_CAUSES.len(), "every cause is reachable");
}

/// An unknown bit is preserved rather than dropped.
///
/// The module's stated contract: turning "the controller reported something we have never seen"
/// into "nothing happened" is worse than a crash, because it is silent.
#[test]
fn undecoded_preserves_bits_no_cause_explains() {
    assert_eq!(undecoded(1 << 31), 1 << 31, "an unknown high bit must survive");
    assert_eq!(undecoded(bits::TX_ARB_LOST), 0, "a known cause is fully explained");
    // A word mixing a known cause with two undefined bits keeps ONLY the undefined ones.
    let mixed = bits::TX_ARB_LOST | (1 << 6) | (1 << 8);
    assert_eq!(undecoded(mixed), (1 << 6) | (1 << 8));
}

/// THE ORDER CONTRACT, and the reason this file exists.
///
/// i2c-designware-common.c:769-:775 tests NOACK first and returns immediately, so a word carrying
/// BOTH a NAK and a lost arbitration is `NoAck` — the PERMANENT failure — and not the retryable
/// `ArbitrationLost`. Swapping the two checks turns a permanent failure into an infinite retry
/// loop. Nothing else in the crate would notice; this assertion is the whole guard.
#[test]
fn noack_is_reported_ahead_of_arbitration_lost() {
    let both = bits::TX_ABRT_NOACK | bits::TX_ARB_LOST;
    assert_eq!(
        verdict(both),
        AbortVerdict::NoAck,
        "NOACK is tested first (:769); reordering makes a permanent failure look retryable"
    );
}

/// Each verdict class maps to the errno Linux returns.
#[test]
fn verdict_maps_each_class() {
    // -EREMOTEIO (i2c-designware-common.c:775). Every named NOACK bit alone yields NoAck.
    for cause in [
        bits::TX_ABRT_7B_ADDR_NOACK,
        bits::TX_ABRT_10ADDR1_NOACK,
        bits::TX_ABRT_10ADDR2_NOACK,
        bits::TX_ABRT_TXDATA_NOACK,
        bits::TX_ABRT_GCALL_NOACK,
    ] {
        assert_eq!(verdict(cause), AbortVerdict::NoAck, "named cause {cause:#x} is NOACK");
    }
    assert_eq!(verdict(bits::TX_ARB_LOST), AbortVerdict::ArbitrationLost); // i2c-designware-common.c:780
    assert_eq!(verdict(bits::TX_ABRT_GCALL_READ), AbortVerdict::BadRequest); // i2c-designware-common.c:782
    assert_eq!(verdict(bits::RX_ABRT_SLAVE_RD_INTX), AbortVerdict::Io); // i2c-designware-common.c:784
    assert_eq!(verdict(0), AbortVerdict::Io, "no known cause set falls through to -EIO");
}

/// `TX_ABRT_NOACK` is the five-bit NAK mask, not a single bit.
///
/// core.h:196 ORs the 7-bit, both 10-bit address, TXDATA and GCALL NAKs. If it ever narrows to one
/// bit, four NAK classes start reporting as generic I/O errors.
#[test]
fn noack_mask_covers_all_five_nak_causes() {
    assert_eq!(bits::TX_ABRT_7B_ADDR_NOACK, 0x1); // i2c-designware-core.h:181
    assert_eq!(bits::TX_ABRT_10ADDR1_NOACK, 0x2); // i2c-designware-core.h:182
    assert_eq!(bits::TX_ABRT_10ADDR2_NOACK, 0x4); // i2c-designware-core.h:183
    assert_eq!(bits::TX_ABRT_TXDATA_NOACK, 0x8); // i2c-designware-core.h:184
    assert_eq!(bits::TX_ABRT_GCALL_NOACK, 0x10); // i2c-designware-core.h:185
    assert_eq!(bits::TX_ABRT_NOACK, 0x1f); // i2c-designware-core.h:196
    assert_eq!(
        bits::TX_ABRT_NOACK,
        bits::TX_ABRT_7B_ADDR_NOACK
            | bits::TX_ABRT_10ADDR1_NOACK
            | bits::TX_ABRT_10ADDR2_NOACK
            | bits::TX_ABRT_TXDATA_NOACK
            | bits::TX_ABRT_GCALL_NOACK
    ); // i2c-designware-core.h:196-200
}

/// A NAK is expected traffic; everything else is not.
///
/// Probing an absent address NAKs on every scan, so logging that at error level trains the reader
/// to ignore the log.
#[test]
fn only_naks_are_expected_traffic() {
    assert!(is_expected_traffic(bits::TX_ABRT_NOACK));
    assert!(
        is_expected_traffic(bits::TX_ABRT_7B_ADDR_NOACK),
        "the named 7-bit address NAK is expected"
    );
    assert!(!is_expected_traffic(bits::TX_ARB_LOST), "lost arbitration is a real fault");
    assert!(!is_expected_traffic(1 << 15));
    assert!(!is_expected_traffic(0));
}

/// The abort registers are touched in capture-then-clear order.
///
/// i2c-designware-master.c:611-:618 — reading CLR_TX_ABRT clears TX_ABRT_SOURCE, so reading the
/// clear register first destroys all fourteen causes and the transfer fails with nothing to say.
#[test]
fn capture_precedes_clear() {
    assert_eq!(
        CAPTURE_THEN_CLEAR,
        [off::TX_ABRT_SOURCE, off::CLR_TX_ABRT],
        "TX_ABRT_SOURCE must be read BEFORE CLR_TX_ABRT, which clears it"
    );
    assert_ne!(
        CAPTURE_THEN_CLEAR[0], CAPTURE_THEN_CLEAR[1],
        "two distinct registers; a collapsed pair would read the clear twice"
    );
}
