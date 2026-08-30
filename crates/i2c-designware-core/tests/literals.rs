// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the DesignWare I2C corpus. Expected values are LINUX literals, quoted with the FILE
//! and line they come from. Asserting against the ported constants would only prove they equal
//! themselves.

use i2c_designware_core::abort::{causes_in, undecoded, ABORT_CAUSES};
use i2c_designware_core::regs::{bits, off};

/// i2c-designware-core.h:61-92 — the offsets a transfer actually touches.
#[test]
fn the_register_offsets_match_linux() {
    assert_eq!(off::CON, 0x00);
    assert_eq!(off::TAR, 0x04);
    assert_eq!(off::SAR, 0x08);
    assert_eq!(off::DATA_CMD, 0x10);
    assert_eq!(off::SS_SCL_HCNT, 0x14);
    assert_eq!(off::SS_SCL_LCNT, 0x18);
    assert_eq!(off::FS_SCL_HCNT, 0x1c);
    assert_eq!(off::FS_SCL_LCNT, 0x20);
    assert_eq!(off::INTR_STAT, 0x2c);
    assert_eq!(off::INTR_MASK, 0x30);
    assert_eq!(off::TX_ABRT_SOURCE, 0x80);
}

/// i2c-designware-core.h:28-40. `BIT(n)` means exactly one bit — a mask that quietly carried two
/// would set a neighbouring control bit on every write.
#[test]
fn every_single_bit_definition_holds_exactly_one_bit() {
    for (name, v) in [
        ("CON_MASTER", bits::CON_MASTER),
        ("CON_10BITADDR_SLAVE", bits::CON_10BITADDR_SLAVE),
        ("CON_10BITADDR_MASTER", bits::CON_10BITADDR_MASTER),
        ("CON_RESTART_EN", bits::CON_RESTART_EN),
        ("CON_SLAVE_DISABLE", bits::CON_SLAVE_DISABLE),
        ("CON_STOP_DET_IFADDRESSED", bits::CON_STOP_DET_IFADDRESSED),
        ("CON_TX_EMPTY_CTRL", bits::CON_TX_EMPTY_CTRL),
        ("ENABLE_ENABLE", bits::ENABLE_ENABLE),
        ("ENABLE_ABORT", bits::ENABLE_ABORT),
    ] {
        assert_eq!(v.count_ones(), 1, "{name} = {v:#x} is not a single bit");
    }
    // And the literals themselves, from the header.
    assert_eq!(bits::CON_MASTER, 1 << 0);
    assert_eq!(bits::CON_RESTART_EN, 1 << 5);
    assert_eq!(bits::CON_SLAVE_DISABLE, 1 << 6);
    assert_eq!(bits::ENABLE_ENABLE, 1 << 0);
    assert_eq!(bits::ENABLE_ABORT, 1 << 1);
}

/// i2c-designware-core.h:29-32 — the speed field is TWO bits at position 1, and the three speeds
/// are values within it, NOT independent flags. Treating them as flags ORs 1 and 2 into 3.
#[test]
fn the_speed_field_is_a_value_not_a_set_of_flags() {
    assert_eq!(bits::CON_SPEED_STD, 1 << 1);
    assert_eq!(bits::CON_SPEED_FAST, 2 << 1);
    assert_eq!(bits::CON_SPEED_HIGH, 3 << 1);
    assert_eq!(bits::CON_SPEED_MASK, 0b110, "GENMASK(2, 1)");
    // FAST is not STD|something: it REPLACES it.
    assert_eq!(bits::CON_SPEED_STD | bits::CON_SPEED_FAST, bits::CON_SPEED_HIGH);
    for s in [bits::CON_SPEED_STD, bits::CON_SPEED_FAST, bits::CON_SPEED_HIGH] {
        assert_eq!(s & !bits::CON_SPEED_MASK, 0, "{s:#x} escapes the speed field");
    }
}

/// i2c-designware-core.h:117-124 — the composite interrupt masks are the OR of named bits, and the
/// master mask is the default plus TX_EMPTY.
#[test]
fn the_composite_interrupt_masks_contain_exactly_the_linux_bits() {
    assert_eq!(bits::INTR_DEFAULT_MASK,
               bits::INTR_RX_FULL | bits::INTR_TX_ABRT | bits::INTR_STOP_DET);
    assert_eq!(bits::INTR_MASTER_MASK, bits::INTR_DEFAULT_MASK | bits::INTR_TX_EMPTY);
    assert_eq!(bits::INTR_SLAVE_MASK,
               bits::INTR_DEFAULT_MASK | bits::INTR_RX_UNDER | bits::INTR_RD_REQ);
    // The master mask must NOT carry the slave-only bits — an over-broad mask enables interrupts
    // nobody handles.
    assert_eq!(bits::INTR_MASTER_MASK & bits::INTR_RD_REQ, 0);
}

/// i2c-designware-core.h:42+ — DATA_CMD carries the byte in bits 7:0 with command bits above it.
#[test]
fn the_data_cmd_fields_do_not_overlap_the_data_byte() {
    assert_eq!(bits::DATA_CMD_DAT, 0xff, "GENMASK(7, 0)");
    for (name, v) in [("READ", bits::DATA_CMD_READ),
                      ("STOP", bits::DATA_CMD_STOP),
                      ("RESTART", bits::DATA_CMD_RESTART)] {
        assert_eq!(v.count_ones(), 1, "{name} is not a single bit");
        assert_eq!(v & bits::DATA_CMD_DAT, 0, "{name} = {v:#x} overlaps the data byte");
    }
    // A read request with a stop is both command bits and NO data — the byte field stays clear.
    let last_read = bits::DATA_CMD_READ | bits::DATA_CMD_STOP;
    assert_eq!(last_read & bits::DATA_CMD_DAT, 0);
}

/// i2c-designware-common.c abort_sources[] has FOURTEEN entries. Bit 12 is ARB_LOST, named without
/// the `ABRT_` prefix the others carry — a prefix-based extraction yields 13 and silently loses
/// arbitration-loss, which would then decode as "unknown".
#[test]
fn all_fourteen_abort_causes_are_present_including_the_odd_one_out() {
    assert_eq!(ABORT_CAUSES.len(), 14);
    assert!(ABORT_CAUSES.iter().any(|c| c.name == "ARB_LOST" && c.bit == 12),
            "ARB_LOST (bit 12) is missing — the prefix trap");
    // Bits 6 and 8 are genuinely undefined in Linux; their absence is a fact, not an omission.
    let bits_present: Vec<u8> = ABORT_CAUSES.iter().map(|c| c.bit).collect();
    assert_eq!(bits_present, vec![0, 1, 2, 3, 4, 5, 7, 9, 10, 11, 12, 13, 14, 15]);
    // Every cause carries a message: a refusal must name what refused.
    assert!(ABORT_CAUSES.iter().all(|c| !c.message.is_empty()));
    // No two causes claim the same bit.
    let mut seen = 0u32;
    for c in ABORT_CAUSES {
        assert_eq!(seen & (1 << c.bit), 0, "bit {} claimed twice", c.bit);
        seen |= 1 << c.bit;
    }
}

/// Decoding names every set cause, in bit order.
#[test]
fn decoding_names_each_cause_that_is_set() {
    let word = (1 << 0) | (1 << 12); // 7B_ADDR_NOACK and ARB_LOST together
    let got: Vec<&str> = causes_in(word).map(|c| c.name).collect();
    assert_eq!(got, vec!["7B_ADDR_NOACK", "ARB_LOST"]);
    assert_eq!(causes_in(0).count(), 0);
}

/// An undefined bit must SURVIVE rather than vanish. "The controller reported something we have
/// never seen" turning into "nothing happened" is the failure this decoder exists to prevent.
#[test]
fn a_bit_no_cause_explains_is_reported_not_dropped() {
    assert_eq!(undecoded(1 << 6), 1 << 6, "bit 6 is undefined in Linux and must be surfaced");
    assert_eq!(undecoded(1 << 8), 1 << 8);
    assert_eq!(undecoded(1 << 31), 1 << 31);
    // A word of only known causes leaves no residue...
    assert_eq!(undecoded((1 << 0) | (1 << 12)), 0);
    // ...and a mixed word reports the cause AND keeps the stranger.
    let mixed = (1 << 3) | (1 << 6);
    assert_eq!(causes_in(mixed).map(|c| c.name).collect::<Vec<_>>(), vec!["TXDATA_NOACK"]);
    assert_eq!(undecoded(mixed), 1 << 6);
}

/// THE COVERAGE RECONCILIATION THAT WAS MISSING. The first extraction of this corpus silently
/// shipped 91 of the header's 109 `DW_IC_` defines: the 16 abort masks are written `BIT(ABRT_x)`
/// where `ABRT_x` is a bare position define, and two more (`COMP_TYPE_VALUE`, `SDA_HOLD_MIN_VERS`)
/// carry a trailing C comment that the value parser choked on. Nothing failed; the constants were
/// simply absent, and the errno mapping that needs them could not have been written.
///
/// This pins the count so a future re-extraction cannot quietly drop a subset again. 108 ported
/// plus 3 command bits Linux does not name (see regs.rs) = 111.
#[test]
fn the_corpus_covers_the_header_rather_than_a_subset_of_it() {
    // Spot-check one member of each family that the broken extraction lost, so this test fails for
    // a reason a reader can act on rather than only as a count mismatch.
    assert_eq!(bits::TX_ABRT_7B_ADDR_NOACK, 1 << 0, "BIT(ABRT_7B_ADDR_NOACK), core.h:181");
    assert_eq!(bits::TX_ARB_LOST, 1 << 12, "BIT(ARB_LOST), core.h:191");
    assert_eq!(bits::TX_ABRT_GCALL_READ, 1 << 5, "BIT(ABRT_GCALL_READ), core.h:186");
    assert_eq!(bits::COMP_TYPE_VALUE, 0x4457_0140, "core.h:100, \"DW\" + 0x0140");
    assert_eq!(bits::SDA_HOLD_MIN_VERS, 0x3131_312A, "core.h:98, \"111*\" == v1.11*");
    // The composite NOACK is the OR of the five NAK causes (core.h:196-:200).
    assert_eq!(
        bits::TX_ABRT_NOACK,
        bits::TX_ABRT_7B_ADDR_NOACK
            | bits::TX_ABRT_10ADDR1_NOACK
            | bits::TX_ABRT_10ADDR2_NOACK
            | bits::TX_ABRT_TXDATA_NOACK
            | bits::TX_ABRT_GCALL_NOACK
    );
}

/// i2c-designware-master.c:611-:618. The clear is a READ of CLR_TX_ABRT, and it destroys
/// TX_ABRT_SOURCE — so the source must be read FIRST. Linux says so in a comment; this asserts it.
#[test]
fn the_abort_source_is_captured_before_the_clear_that_destroys_it() {
    use i2c_designware_core::abort::CAPTURE_THEN_CLEAR;
    assert_eq!(CAPTURE_THEN_CLEAR, [off::TX_ABRT_SOURCE, off::CLR_TX_ABRT]);
    assert_eq!(CAPTURE_THEN_CLEAR[0], 0x80, "TX_ABRT_SOURCE, core.h:92");
    assert_eq!(CAPTURE_THEN_CLEAR[1], 0x54, "CLR_TX_ABRT, core.h:81");
    assert_ne!(CAPTURE_THEN_CLEAR[0], CAPTURE_THEN_CLEAR[1]);
}

/// i2c-designware-common.c:769-:785. The ORDER of the checks is the contract: NOACK is tested
/// first and returns immediately, so a word carrying BOTH a NAK and a lost arbitration reports
/// NoAck — not the RETRYABLE ArbitrationLost. Swapping them turns a permanent failure into an
/// infinite retry, or the reverse.
#[test]
fn the_verdict_order_decides_whether_a_failure_is_retryable() {
    use i2c_designware_core::abort::{verdict, AbortVerdict};
    assert_eq!(verdict(bits::TX_ABRT_7B_ADDR_NOACK), AbortVerdict::NoAck);
    assert_eq!(verdict(bits::TX_ARB_LOST), AbortVerdict::ArbitrationLost);
    assert_eq!(verdict(bits::TX_ABRT_GCALL_READ), AbortVerdict::BadRequest);
    assert_eq!(verdict(bits::TX_ABRT_MASTER_DIS), AbortVerdict::Io, "anything else is -EIO");
    assert_eq!(verdict(0), AbortVerdict::Io);
    // BOTH set: NOACK wins, because Linux returns before it ever looks at ARB_LOST.
    assert_eq!(
        verdict(bits::TX_ABRT_7B_ADDR_NOACK | bits::TX_ARB_LOST),
        AbortVerdict::NoAck,
        "NOACK is tested first and returns immediately (:769-:775)"
    );
}

/// :769-:772 logs NAKs at DEBUG, :776-:777 logs everything else at ERROR. Probing an absent
/// address NAKs on every scan; an error line per probe trains the reader to ignore the log.
#[test]
fn a_nak_is_expected_traffic_and_the_rest_is_not() {
    use i2c_designware_core::abort::is_expected_traffic;
    assert!(is_expected_traffic(bits::TX_ABRT_7B_ADDR_NOACK));
    assert!(!is_expected_traffic(bits::TX_ARB_LOST));
    assert!(!is_expected_traffic(bits::TX_ABRT_MASTER_DIS));
}
