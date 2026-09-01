// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for `i2c_msg` flags.
//!
//! Ported from Linux `include/uapi/linux/i2c.h:16-85`, originally copyrighted by Simon G. Vogl,
//! Kyösti Mälkki, and Frodo Looijaard.

use i2c_core_msg_core::flags::*;

/// DELIBERATELY literal: never derive this expectation from `MESSAGE_FLAGS`.
const LINUX_FLAG_NAMES: [&str; 7] = [
    "I2C_M_RD",
    "I2C_M_TEN",
    "I2C_M_RECV_LEN",
    "I2C_M_NO_RD_ACK",
    "I2C_M_IGNORE_NAK",
    "I2C_M_NOSTART",
    "I2C_M_STOP",
];

/// DELIBERATELY literal values from include/uapi/linux/i2c.h:77-85.
const LINUX_FLAG_VALUES: [u16; 7] = [0x0001, 0x0010, 0x0400, 0x0800, 0x1000, 0x4000, 0x8000];

#[test]
fn all_seven_scoped_flags_are_pinned_by_count_name_and_literal() {
    assert_eq!(MESSAGE_FLAGS.len(), 7);
    let names: Vec<&str> = MESSAGE_FLAGS.iter().map(|flag| flag.name).collect();
    let values: Vec<u16> = MESSAGE_FLAGS.iter().map(|flag| flag.value).collect();
    assert_eq!(names, LINUX_FLAG_NAMES);
    assert_eq!(values, LINUX_FLAG_VALUES);
}

#[test]
fn each_flag_semantic_tests_its_own_bit() {
    // include/uapi/linux/i2c.h:22-44,77-85.
    assert!(is_read(0x0001));
    assert!(!is_read(0x0000));
    assert!(is_ten_bit(0x0010));
    assert!(!is_ten_bit(0x0001));
    assert!(receives_length(0x0400));
    assert!(!receives_length(0x0800));
    assert!(skips_read_ack(0x0800));
    assert!(!skips_read_ack(0x0400));
    assert!(ignores_nak(0x1000));
    assert!(!ignores_nak(0x0800));
    assert!(omits_start(0x4000));
    assert!(!omits_start(0x8000));
    assert!(forces_stop(0x8000));
    assert!(!forces_stop(0x4000));

    let all = 0x0001 | 0x0010 | 0x0400 | 0x0800 | 0x1000 | 0x4000 | 0x8000;
    assert!(is_read(all));
    assert!(is_ten_bit(all));
    assert!(receives_length(all));
    assert!(skips_read_ack(all));
    assert!(ignores_nak(all));
    assert!(omits_start(all));
    assert!(forces_stop(all));
}
