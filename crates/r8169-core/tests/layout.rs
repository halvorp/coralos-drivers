// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the ported r8169 register map and descriptor format.
//!
//! These assert the PORT against the values in the Linux source, because that is the only thing a
//! port can be wrong about at this stage. sdhci-core's history in this repo is the warning worth
//! heeding: its first six recovery vectors ASSERTED NOTHING and had to be rewritten. A vector that
//! cannot fail is not a vector.
//!
//! Each expectation below is the literal from
//! references/linux-ref/drivers/net/ethernet/realtek/r8169_main.c, with its line.

use r8169_core::{desc, regs};

/// enum rtl_registers, r8169_main.c:253-297. Written out as a table so a transposition is visible
/// rather than buried in a series of separate asserts.
#[test]
fn the_register_offsets_match_the_linux_enum() {
    let expected: &[(&str, u32, u32)] = &[
        // (name, ported, linux literal)
        ("MAC0", regs::MAC0, 0),
        ("MAC4", regs::MAC4, 4),
        ("MAR0", regs::MAR0, 8),
        ("CounterAddrLow", regs::COUNTER_ADDR_LOW, 0x10),
        ("TxDescStartAddrLow", regs::TX_DESC_START_ADDR_LOW, 0x20),
        ("TxDescStartAddrHigh", regs::TX_DESC_START_ADDR_HIGH, 0x24),
        ("ChipCmd", regs::CHIP_CMD, 0x37),
        ("TxPoll", regs::TX_POLL, 0x38),
        ("IntrMask", regs::INTR_MASK, 0x3c),
        ("IntrStatus", regs::INTR_STATUS, 0x3e),
        ("TxConfig", regs::TX_CONFIG, 0x40),
        ("RxConfig", regs::RX_CONFIG, 0x44),
        ("Cfg9346", regs::CFG_9346, 0x50),
        ("Config0", regs::CONFIG0, 0x51),
        ("Config1", regs::CONFIG1, 0x52),
        ("Config2", regs::CONFIG2, 0x53),
        ("Config3", regs::CONFIG3, 0x54),
        ("Config4", regs::CONFIG4, 0x55),
        ("Config5", regs::CONFIG5, 0x56),
        ("PHYAR", regs::PHYAR, 0x60),
        ("PHYstatus", regs::PHY_STATUS, 0x6c),
    ];
    for (name, ported, linux) in expected {
        assert_eq!(ported, linux, "{name}: ported {ported:#x} != Linux {linux:#x}");
    }
}

/// IntrMask and IntrStatus are both 16-bit and two bytes apart — the pair most likely to be
/// transposed, and a transposition arms nothing and acknowledges nothing. Called out on its own so
/// the failure names the hazard rather than just a number.
#[test]
fn the_interrupt_pair_is_not_transposed() {
    assert_eq!(regs::INTR_MASK, 0x3c, "IntrMask");
    assert_eq!(regs::INTR_STATUS, 0x3e, "IntrStatus");
    assert!(regs::INTR_MASK < regs::INTR_STATUS, "mask precedes status");
    assert_eq!(regs::INTR_STATUS - regs::INTR_MASK, 2);
}

/// struct TxDesc/RxDesc: __le32 opts1; __le32 opts2; __le64 addr. (r8169_main.c:646-656)
#[test]
fn the_descriptor_is_sixteen_bytes_laid_out_as_linux_declares_it() {
    assert_eq!(desc::DESC_BYTES, 16);
    assert_eq!(desc::OFF_OPTS1, 0);
    assert_eq!(desc::OFF_OPTS2, 4);
    assert_eq!(desc::OFF_ADDR, 8, "the 64-bit addr must be 8-byte aligned within the descriptor");
    assert_eq!(desc::OFF_ADDR + 8, desc::DESC_BYTES, "addr is the last field");
}

/// Generic descriptor bits, r8169_main.c:579-582.
#[test]
fn the_descriptor_flag_bits_match_linux() {
    assert_eq!(desc::DESC_OWN, 1 << 31);
    assert_eq!(desc::RING_END, 1 << 30);
    assert_eq!(desc::FIRST_FRAG, 1 << 29);
    assert_eq!(desc::LAST_FRAG, 1 << 28);
    // They must not overlap each other, nor the length field.
    let flags = [desc::DESC_OWN, desc::RING_END, desc::FIRST_FRAG, desc::LAST_FRAG];
    for (i, a) in flags.iter().enumerate() {
        for b in &flags[i + 1..] {
            assert_eq!(a & b, 0, "flag bits overlap");
        }
        assert_eq!(a & desc::OPTS1_LEN_MASK, 0, "a flag bit collides with the length field");
    }
}

/// rtl8169_mark_to_asic (r8169_main.c:4146-4151) preserves the existing RingEnd bit when handing a
/// descriptor back. THIS IS THE BEHAVIOURAL ONE: losing that bit on the LAST descriptor makes the
/// NIC walk past the end of the ring, and nothing in the layout constants would catch it.
#[test]
fn handing_a_descriptor_to_the_nic_preserves_the_ring_end_bit() {
    let last = desc::RING_END; // the final descriptor, previously marked
    let opts1 = desc::rx_opts1_hand_to_nic(last, 1536);
    assert!(desc::is_owned_by_nic(opts1), "must be handed to the NIC");
    assert_eq!(opts1 & desc::RING_END, desc::RING_END, "RingEnd must SURVIVE the recycle");
    assert_eq!(opts1 & desc::OPTS1_LEN_MASK, 1536, "length must be carried");

    // …and a middle descriptor must NOT acquire it.
    let middle = desc::rx_opts1_hand_to_nic(0, 1536);
    assert_eq!(middle & desc::RING_END, 0, "a non-final descriptor must not gain RingEnd");
}

/// A length larger than the field must not corrupt the flags — the mask is what stops a caller's
/// bad length from setting DescOwn or RingEnd by accident.
#[test]
fn an_oversized_length_cannot_spill_into_the_flag_bits() {
    let opts1 = desc::rx_opts1_hand_to_nic(0, 0xffff_ffff);
    assert_eq!(opts1 & desc::RING_END, 0, "a huge length must not set RingEnd");
    assert_eq!(opts1 & desc::FIRST_FRAG, 0);
    assert_eq!(opts1 & desc::LAST_FRAG, 0);
    assert_eq!(opts1 & desc::OPTS1_LEN_MASK, desc::OPTS1_LEN_MASK);
}
