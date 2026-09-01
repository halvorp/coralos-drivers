// SPDX-License-Identifier: GPL-2.0-only
//! Register-corpus vectors ported from Linux `drivers/i2c/busses/i2c-designware-core.h` and
//! `drivers/i2c/busses/i2c-designware-master.c`.
//!
//! Copyright (c) Synopsys Inc., Intel Corporation, and the Linux i2c-designware authors.
//!
//! Every expected value below is a Linux literal. The actual side always passes through the named
//! production constant so changing any constant changes a test result.

use i2c_designware_core::abort::causes_in;
use i2c_designware_core::regs::{bits, off};
use i2c_designware_core::xfer::{rx_fifo_depth, tx_fifo_depth};

/// Every register offset in i2c-designware-core.h:61-99, including registers not used by the pure
/// state machines yet. An unused clear register is still part of the hardware ABI.
#[test]
fn every_register_offset_matches_linux() {
    assert_eq!(off::CON, 0x00); // i2c-designware-core.h:61
    assert_eq!(off::TAR, 0x04); // i2c-designware-core.h:62
    assert_eq!(off::SAR, 0x08); // i2c-designware-core.h:63
    assert_eq!(off::DATA_CMD, 0x10); // i2c-designware-core.h:64
    assert_eq!(off::SS_SCL_HCNT, 0x14); // i2c-designware-core.h:65
    assert_eq!(off::SS_SCL_LCNT, 0x18); // i2c-designware-core.h:66
    assert_eq!(off::FS_SCL_HCNT, 0x1c); // i2c-designware-core.h:67
    assert_eq!(off::FS_SCL_LCNT, 0x20); // i2c-designware-core.h:68
    assert_eq!(off::HS_SCL_HCNT, 0x24); // i2c-designware-core.h:69
    assert_eq!(off::HS_SCL_LCNT, 0x28); // i2c-designware-core.h:70
    assert_eq!(off::INTR_STAT, 0x2c); // i2c-designware-core.h:71
    assert_eq!(off::INTR_MASK, 0x30); // i2c-designware-core.h:72
    assert_eq!(off::RAW_INTR_STAT, 0x34); // i2c-designware-core.h:73
    assert_eq!(off::RX_TL, 0x38); // i2c-designware-core.h:74
    assert_eq!(off::TX_TL, 0x3c); // i2c-designware-core.h:75
    assert_eq!(off::CLR_INTR, 0x40); // i2c-designware-core.h:76
    assert_eq!(off::CLR_RX_UNDER, 0x44); // i2c-designware-core.h:77
    assert_eq!(off::CLR_RX_OVER, 0x48); // i2c-designware-core.h:78
    assert_eq!(off::CLR_TX_OVER, 0x4c); // i2c-designware-core.h:79
    assert_eq!(off::CLR_RD_REQ, 0x50); // i2c-designware-core.h:80
    assert_eq!(off::CLR_TX_ABRT, 0x54); // i2c-designware-core.h:81
    assert_eq!(off::CLR_RX_DONE, 0x58); // i2c-designware-core.h:82
    assert_eq!(off::CLR_ACTIVITY, 0x5c); // i2c-designware-core.h:83
    assert_eq!(off::CLR_STOP_DET, 0x60); // i2c-designware-core.h:84
    assert_eq!(off::CLR_START_DET, 0x64); // i2c-designware-core.h:85
    assert_eq!(off::CLR_GEN_CALL, 0x68); // i2c-designware-core.h:86
    assert_eq!(off::ENABLE, 0x6c); // i2c-designware-core.h:87
    assert_eq!(off::STATUS, 0x70); // i2c-designware-core.h:88
    assert_eq!(off::TXFLR, 0x74); // i2c-designware-core.h:89
    assert_eq!(off::RXFLR, 0x78); // i2c-designware-core.h:90
    assert_eq!(off::SDA_HOLD, 0x7c); // i2c-designware-core.h:91
    assert_eq!(off::TX_ABRT_SOURCE, 0x80); // i2c-designware-core.h:92
    assert_eq!(off::ENABLE_STATUS, 0x9c); // i2c-designware-core.h:93
    assert_eq!(off::CLR_RESTART_DET, 0xa8); // i2c-designware-core.h:94
    assert_eq!(off::SMBUS_INTR_MASK, 0xcc); // i2c-designware-core.h:95
    assert_eq!(off::COMP_PARAM_1, 0xf4); // i2c-designware-core.h:96
    assert_eq!(off::COMP_VERSION, 0xf8); // i2c-designware-core.h:97
    assert_eq!(off::COMP_TYPE, 0xfc); // i2c-designware-core.h:99
    assert_eq!(off::ERR_TX_ABRT, 0x01); // i2c-designware-core.h:139
}

/// Control and DATA_CMD definitions from i2c-designware-core.h:28-49. These assertions pin every
/// family member independently; the relationship assertions below do not substitute for literals.
#[test]
fn every_control_and_command_definition_matches_linux() {
    assert_eq!(bits::CON_MASTER, 0x1); // i2c-designware-core.h:28
    assert_eq!(bits::CON_SPEED_STD, 0x2); // i2c-designware-core.h:29
    assert_eq!(bits::CON_SPEED_FAST, 0x4); // i2c-designware-core.h:30
    assert_eq!(bits::CON_SPEED_HIGH, 0x6); // i2c-designware-core.h:31
    assert_eq!(bits::CON_SPEED_MASK, 0x6); // i2c-designware-core.h:32
    assert_eq!(bits::CON_10BITADDR_SLAVE, 0x8); // i2c-designware-core.h:33
    assert_eq!(bits::CON_10BITADDR_MASTER, 0x10); // i2c-designware-core.h:34
    assert_eq!(bits::CON_RESTART_EN, 0x20); // i2c-designware-core.h:35
    assert_eq!(bits::CON_SLAVE_DISABLE, 0x40); // i2c-designware-core.h:36
    assert_eq!(bits::CON_STOP_DET_IFADDRESSED, 0x80); // i2c-designware-core.h:37
    assert_eq!(bits::CON_TX_EMPTY_CTRL, 0x100); // i2c-designware-core.h:38
    assert_eq!(bits::CON_RX_FIFO_FULL_HLD_CTRL, 0x200); // i2c-designware-core.h:39
    assert_eq!(bits::CON_BUS_CLEAR_CTRL, 0x800); // i2c-designware-core.h:40
    assert_eq!(bits::DATA_CMD_DAT, 0xff); // i2c-designware-core.h:42
    assert_eq!(bits::DATA_CMD_FIRST_DATA_BYTE, 0x800); // i2c-designware-core.h:43
    assert_eq!(bits::REG_STEP_BYTES, 0x2); // i2c-designware-core.h:48
    assert_eq!(bits::REG_WORD_SHIFT, 0x10); // i2c-designware-core.h:49

    assert_eq!(bits::DATA_CMD_READ, 0x100); // i2c-designware-master.c:442
    assert_eq!(bits::DATA_CMD_STOP, 0x200); // i2c-designware-master.c:429
    assert_eq!(bits::DATA_CMD_RESTART, 0x400); // i2c-designware-master.c:432
}

fn speed_name(con: u32) -> &'static str {
    match con & bits::CON_SPEED_MASK {
        value if value == bits::CON_SPEED_STD => "standard",
        value if value == bits::CON_SPEED_FAST => "fast",
        value if value == bits::CON_SPEED_HIGH => "high",
        _ => "reserved",
    }
}

/// Drive each value of the two-bit speed field through a decode, rather than assuming one covered
/// member protects its siblings.
#[test]
fn every_control_speed_value_decodes_to_its_linux_class() {
    assert_eq!(speed_name(0x0), "reserved"); // i2c-designware-core.h:29-32
    assert_eq!(speed_name(0x2), "standard"); // i2c-designware-core.h:29
    assert_eq!(speed_name(0x4), "fast"); // i2c-designware-core.h:30
    assert_eq!(speed_name(0x6), "high"); // i2c-designware-core.h:31
    assert_eq!(speed_name(0xffff_fffc), "fast"); // i2c-designware-core.h:32, GENMASK(2, 1)
}

/// FIFO geometry, identifying values, and version gates from i2c-designware-core.h:54-100.
#[test]
fn every_geometry_and_identity_definition_matches_linux() {
    assert_eq!(bits::FIFO_TX_FIELD, 0xff0000); // i2c-designware-core.h:54
    assert_eq!(bits::FIFO_RX_FIELD, 0xff00); // i2c-designware-core.h:55
    assert_eq!(bits::FIFO_MIN_DEPTH, 0x2); // i2c-designware-core.h:56
    assert_eq!(bits::SDA_HOLD_MIN_VERS, 0x3131_312a); // i2c-designware-core.h:98
    assert_eq!(bits::COMP_TYPE_VALUE, 0x4457_0140); // i2c-designware-core.h:100
}

/// Use real COMP_PARAM_1 words to select every FIFO field and prove neighbouring bits are ignored.
#[test]
fn both_fifo_fields_decode_independently() {
    assert_eq!(tx_fifo_depth(0x001f_0700), 0x20); // i2c-designware-core.h:54
    assert_eq!(rx_fifo_depth(0x001f_0700), 0x08); // i2c-designware-core.h:55
    assert_eq!(tx_fifo_depth(0x00ff_0001), 0x100); // i2c-designware-core.h:54
    assert_eq!(rx_fifo_depth(0x0100_ff00), 0x100); // i2c-designware-core.h:55
}

fn interrupt_names(raw: u32) -> Vec<&'static str> {
    [
        (bits::INTR_RX_UNDER, "RX_UNDER"),
        (bits::INTR_RX_OVER, "RX_OVER"),
        (bits::INTR_RX_FULL, "RX_FULL"),
        (bits::INTR_TX_OVER, "TX_OVER"),
        (bits::INTR_TX_EMPTY, "TX_EMPTY"),
        (bits::INTR_RD_REQ, "RD_REQ"),
        (bits::INTR_TX_ABRT, "TX_ABRT"),
        (bits::INTR_RX_DONE, "RX_DONE"),
        (bits::INTR_ACTIVITY, "ACTIVITY"),
        (bits::INTR_STOP_DET, "STOP_DET"),
        (bits::INTR_START_DET, "START_DET"),
        (bits::INTR_GEN_CALL, "GEN_CALL"),
        (bits::INTR_RESTART_DET, "RESTART_DET"),
        (bits::INTR_MST_ON_HOLD, "MST_ON_HOLD"),
    ]
    .into_iter()
    .filter_map(|(mask, name)| (raw & mask != 0).then_some(name))
    .collect()
}

/// Pin each interrupt source by Linux literal before testing any composite masks.
#[test]
fn every_interrupt_bit_matches_linux() {
    assert_eq!(bits::INTR_RX_UNDER, 0x1); // i2c-designware-core.h:102
    assert_eq!(bits::INTR_RX_OVER, 0x2); // i2c-designware-core.h:103
    assert_eq!(bits::INTR_RX_FULL, 0x4); // i2c-designware-core.h:104
    assert_eq!(bits::INTR_TX_OVER, 0x8); // i2c-designware-core.h:105
    assert_eq!(bits::INTR_TX_EMPTY, 0x10); // i2c-designware-core.h:106
    assert_eq!(bits::INTR_RD_REQ, 0x20); // i2c-designware-core.h:107
    assert_eq!(bits::INTR_TX_ABRT, 0x40); // i2c-designware-core.h:108
    assert_eq!(bits::INTR_RX_DONE, 0x80); // i2c-designware-core.h:109
    assert_eq!(bits::INTR_ACTIVITY, 0x100); // i2c-designware-core.h:110
    assert_eq!(bits::INTR_STOP_DET, 0x200); // i2c-designware-core.h:111
    assert_eq!(bits::INTR_START_DET, 0x400); // i2c-designware-core.h:112
    assert_eq!(bits::INTR_GEN_CALL, 0x800); // i2c-designware-core.h:113
    assert_eq!(bits::INTR_RESTART_DET, 0x1000); // i2c-designware-core.h:114
    assert_eq!(bits::INTR_MST_ON_HOLD, 0x2000); // i2c-designware-core.h:115
}

/// Drive one raw interrupt sample for every family member through the same decode. An incorrectly
/// colliding bit produces two names; a missing/wrong bit produces none.
#[test]
fn every_interrupt_source_decodes_to_its_linux_name() {
    let vectors = [
        (0x1, "RX_UNDER"), // i2c-designware-core.h:102
        (0x2, "RX_OVER"), // i2c-designware-core.h:103
        (0x4, "RX_FULL"), // i2c-designware-core.h:104
        (0x8, "TX_OVER"), // i2c-designware-core.h:105
        (0x10, "TX_EMPTY"), // i2c-designware-core.h:106
        (0x20, "RD_REQ"), // i2c-designware-core.h:107
        (0x40, "TX_ABRT"), // i2c-designware-core.h:108
        (0x80, "RX_DONE"), // i2c-designware-core.h:109
        (0x100, "ACTIVITY"), // i2c-designware-core.h:110
        (0x200, "STOP_DET"), // i2c-designware-core.h:111
        (0x400, "START_DET"), // i2c-designware-core.h:112
        (0x800, "GEN_CALL"), // i2c-designware-core.h:113
        (0x1000, "RESTART_DET"), // i2c-designware-core.h:114
        (0x2000, "MST_ON_HOLD"), // i2c-designware-core.h:115
    ];
    for (raw, expected) in vectors {
        assert_eq!(interrupt_names(raw), [expected], "raw interrupt {raw:#x}");
    }
    assert!(interrupt_names(0).is_empty());
}

/// The three interrupt masks are composites: both the Linux literal and their named components
/// are load-bearing and therefore asserted separately.
#[test]
fn interrupt_composites_equal_their_named_members_and_linux_literals() {
    assert_eq!(bits::INTR_DEFAULT_MASK, 0x244); // i2c-designware-core.h:117
    assert_eq!(bits::INTR_MASTER_MASK, 0x254); // i2c-designware-core.h:120
    assert_eq!(bits::INTR_SLAVE_MASK, 0x265); // i2c-designware-core.h:122

    assert_eq!(
        bits::INTR_DEFAULT_MASK,
        bits::INTR_RX_FULL | bits::INTR_TX_ABRT | bits::INTR_STOP_DET
    ); // i2c-designware-core.h:117-119
    assert_eq!(
        bits::INTR_MASTER_MASK,
        bits::INTR_RX_FULL | bits::INTR_TX_EMPTY | bits::INTR_TX_ABRT | bits::INTR_STOP_DET
    ); // i2c-designware-core.h:120-121
    assert_eq!(
        bits::INTR_SLAVE_MASK,
        bits::INTR_RX_UNDER
            | bits::INTR_RX_FULL
            | bits::INTR_RD_REQ
            | bits::INTR_TX_ABRT
            | bits::INTR_STOP_DET
    ); // i2c-designware-core.h:122-124
}

fn status_names(raw: u32) -> Vec<&'static str> {
    [
        (bits::STATUS_ACTIVITY, "ACTIVITY"),
        (bits::STATUS_TFE, "TFE"),
        (bits::STATUS_RFNE, "RFNE"),
        (bits::STATUS_MASTER_ACTIVITY, "MASTER_ACTIVITY"),
        (bits::STATUS_SLAVE_ACTIVITY, "SLAVE_ACTIVITY"),
        (bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY, "MASTER_HOLD_TX_FIFO_EMPTY"),
    ]
    .into_iter()
    .filter_map(|(mask, name)| (raw & mask != 0).then_some(name))
    .collect()
}

/// Enable and status fields from i2c-designware-core.h:126-137.
#[test]
fn every_enable_status_and_sda_hold_definition_matches_linux() {
    assert_eq!(bits::ENABLE_ENABLE, 0x1); // i2c-designware-core.h:126
    assert_eq!(bits::ENABLE_ABORT, 0x2); // i2c-designware-core.h:127
    assert_eq!(bits::STATUS_ACTIVITY, 0x1); // i2c-designware-core.h:129
    assert_eq!(bits::STATUS_TFE, 0x4); // i2c-designware-core.h:130
    assert_eq!(bits::STATUS_RFNE, 0x8); // i2c-designware-core.h:131
    assert_eq!(bits::STATUS_MASTER_ACTIVITY, 0x20); // i2c-designware-core.h:132
    assert_eq!(bits::STATUS_SLAVE_ACTIVITY, 0x40); // i2c-designware-core.h:133
    assert_eq!(bits::STATUS_MASTER_HOLD_TX_FIFO_EMPTY, 0x80); // i2c-designware-core.h:134
    assert_eq!(bits::SDA_HOLD_RX_SHIFT, 0x10); // i2c-designware-core.h:136
    assert_eq!(bits::SDA_HOLD_RX_MASK, 0xff0000); // i2c-designware-core.h:137
}

/// Drive every status family member independently. STATUS_TFE and STATUS_RFNE are deliberately not
/// inferred from a combined "ready" sample: each controls a different FIFO wait.
#[test]
fn every_status_bit_decodes_to_its_linux_name() {
    let vectors = [
        (0x1, "ACTIVITY"), // i2c-designware-core.h:129
        (0x4, "TFE"), // i2c-designware-core.h:130
        (0x8, "RFNE"), // i2c-designware-core.h:131
        (0x20, "MASTER_ACTIVITY"), // i2c-designware-core.h:132
        (0x40, "SLAVE_ACTIVITY"), // i2c-designware-core.h:133
        (0x80, "MASTER_HOLD_TX_FIFO_EMPTY"), // i2c-designware-core.h:134
    ];
    for (raw, expected) in vectors {
        assert_eq!(status_names(raw), [expected], "raw status {raw:#x}");
    }
    assert!(status_names(0x12).is_empty(), "reserved status bits have no classification");
}

fn high_speed_capable(comp_param_1: u32) -> bool {
    comp_param_1 & bits::COMP_PARAM_1_SPEED_MODE_MASK == bits::COMP_PARAM_1_SPEED_MODE_HIGH
}

fn controller_role(value: u32) -> &'static str {
    if value == bits::MASTER {
        "master"
    } else if value == bits::SLAVE {
        "slave"
    } else {
        "invalid"
    }
}

/// Target, capability and role values from i2c-designware-core.h:141-158.
#[test]
fn every_target_capability_and_role_definition_matches_linux() {
    assert_eq!(bits::TAR_10BITADDR_MASTER, 0x1000); // i2c-designware-core.h:141
    assert_eq!(bits::COMP_PARAM_1_SPEED_MODE_HIGH, 0xc); // i2c-designware-core.h:143
    assert_eq!(bits::COMP_PARAM_1_SPEED_MODE_MASK, 0xc); // i2c-designware-core.h:144
    assert_eq!(bits::MASTER, 0x0); // i2c-designware-core.h:157
    assert_eq!(bits::SLAVE, 0x1); // i2c-designware-core.h:158
}

/// Decode all encodings of the COMP_PARAM_1 speed-mode field. This pins the mask as a selector and
/// HIGH as a value, rather than only observing one convenient capability word.
#[test]
fn comp_param_speed_mode_selects_only_the_high_speed_encoding() {
    assert!(!high_speed_capable(0x0)); // i2c-designware-core.h:144
    assert!(!high_speed_capable(0x4)); // i2c-designware-core.h:144
    assert!(!high_speed_capable(0x8)); // i2c-designware-core.h:144
    assert!(high_speed_capable(0xc)); // i2c-designware-core.h:143
    assert!(high_speed_capable(0xffff_ffff)); // i2c-designware-core.h:144
}

/// MASTER is zero-valued, so exercise it as a selector and prove it remains distinct from SLAVE.
/// Merely ORing a zero into a word would not establish either property.
#[test]
fn zero_valued_master_selects_the_master_role() {
    assert_ne!(bits::MASTER, bits::SLAVE); // i2c-designware-core.h:157-158
    assert_eq!(controller_role(0x0), "master"); // i2c-designware-core.h:157
    assert_eq!(controller_role(0x1), "slave"); // i2c-designware-core.h:158
    assert_eq!(controller_role(0x2), "invalid"); // i2c-designware-core.h:157-158
}

/// Pin every abort mask independently. In particular, testing TX_ABRT_NOACK's value cannot pin the
/// five independently stored component constants.
#[test]
fn every_abort_mask_matches_its_linux_literal() {
    assert_eq!(bits::TX_ABRT_7B_ADDR_NOACK, 0x1); // i2c-designware-core.h:181
    assert_eq!(bits::TX_ABRT_10ADDR1_NOACK, 0x2); // i2c-designware-core.h:182
    assert_eq!(bits::TX_ABRT_10ADDR2_NOACK, 0x4); // i2c-designware-core.h:183
    assert_eq!(bits::TX_ABRT_TXDATA_NOACK, 0x8); // i2c-designware-core.h:184
    assert_eq!(bits::TX_ABRT_GCALL_NOACK, 0x10); // i2c-designware-core.h:185
    assert_eq!(bits::TX_ABRT_GCALL_READ, 0x20); // i2c-designware-core.h:186
    assert_eq!(bits::TX_ABRT_SBYTE_ACKDET, 0x80); // i2c-designware-core.h:187
    assert_eq!(bits::TX_ABRT_SBYTE_NORSTRT, 0x200); // i2c-designware-core.h:188
    assert_eq!(bits::TX_ABRT_10B_RD_NORSTRT, 0x400); // i2c-designware-core.h:189
    assert_eq!(bits::TX_ABRT_MASTER_DIS, 0x800); // i2c-designware-core.h:190
    assert_eq!(bits::TX_ARB_LOST, 0x1000); // i2c-designware-core.h:191
    assert_eq!(bits::RX_ABRT_SLAVE_RD_INTX, 0x8000); // i2c-designware-core.h:192
    assert_eq!(bits::RX_ABRT_SLAVE_ARBLOST, 0x4000); // i2c-designware-core.h:193
    assert_eq!(bits::RX_ABRT_SLAVE_FLUSH_TXFIFO, 0x2000); // i2c-designware-core.h:194
    assert_eq!(bits::TX_ABRT_NOACK, 0x1f); // i2c-designware-core.h:196
}

/// The NOACK composite is asserted through all five named production constants, as well as by its
/// own Linux literal in `every_abort_mask_matches_its_linux_literal`.
#[test]
fn noack_composite_is_exactly_its_five_named_causes() {
    assert_eq!(
        bits::TX_ABRT_NOACK,
        bits::TX_ABRT_7B_ADDR_NOACK
            | bits::TX_ABRT_10ADDR1_NOACK
            | bits::TX_ABRT_10ADDR2_NOACK
            | bits::TX_ABRT_TXDATA_NOACK
            | bits::TX_ABRT_GCALL_NOACK
    ); // i2c-designware-core.h:196-200
}

/// Drive each named abort mask into the production cause decoder and assert Linux's classification.
/// This covers the NOACK members and all their family siblings, including the reversed-order RX
/// abort declarations at lines 192-194.
#[test]
fn every_abort_mask_selects_its_linux_cause() {
    let vectors = [
        (bits::TX_ABRT_7B_ADDR_NOACK, "7B_ADDR_NOACK"), // i2c-designware-core.h:181
        (bits::TX_ABRT_10ADDR1_NOACK, "10ADDR1_NOACK"), // i2c-designware-core.h:182
        (bits::TX_ABRT_10ADDR2_NOACK, "10ADDR2_NOACK"), // i2c-designware-core.h:183
        (bits::TX_ABRT_TXDATA_NOACK, "TXDATA_NOACK"), // i2c-designware-core.h:184
        (bits::TX_ABRT_GCALL_NOACK, "GCALL_NOACK"), // i2c-designware-core.h:185
        (bits::TX_ABRT_GCALL_READ, "GCALL_READ"), // i2c-designware-core.h:186
        (bits::TX_ABRT_SBYTE_ACKDET, "SBYTE_ACKDET"), // i2c-designware-core.h:187
        (bits::TX_ABRT_SBYTE_NORSTRT, "SBYTE_NORSTRT"), // i2c-designware-core.h:188
        (bits::TX_ABRT_10B_RD_NORSTRT, "10B_RD_NORSTRT"), // i2c-designware-core.h:189
        (bits::TX_ABRT_MASTER_DIS, "MASTER_DIS"), // i2c-designware-core.h:190
        (bits::TX_ARB_LOST, "ARB_LOST"), // i2c-designware-core.h:191
        (bits::RX_ABRT_SLAVE_FLUSH_TXFIFO, "SLAVE_FLUSH_TXFIFO"), // i2c-designware-core.h:194
        (bits::RX_ABRT_SLAVE_ARBLOST, "SLAVE_ARBLOST"), // i2c-designware-core.h:193
        (bits::RX_ABRT_SLAVE_RD_INTX, "SLAVE_RD_INTX"), // i2c-designware-core.h:192
    ];
    for (raw, expected) in vectors {
        let got: Vec<&str> = causes_in(raw).map(|cause| cause.name).collect();
        assert_eq!(got, [expected], "abort source {raw:#x}");
    }
}
