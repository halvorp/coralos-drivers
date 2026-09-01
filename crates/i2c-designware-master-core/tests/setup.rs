// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for master setup and message flags.
//!
//! Copyright (C) 2006 Texas Instruments; Copyright (C) 2007 MontaVista Software Inc.;
//! Copyright (C) 2009 Provigent Ltd.

use i2c_designware_master_core::{setup::*, timing::*};

/// include/uapi/linux/i2c.h:77-85 defines exactly the four flags consumed by master.c. Expected
/// names and values are frozen here rather than generated from `FLAG_TABLE`.
#[test]
fn all_four_consumed_message_flags_have_linux_names_and_literals() {
    assert_eq!(FLAG_TABLE.len(), 4);
    assert_eq!(FLAG_TABLE, [
        ("I2C_M_RD", 0x0001),
        ("I2C_M_TEN", 0x0010),
        ("I2C_M_RECV_LEN", 0x0400),
        ("I2C_M_STOP", 0x8000),
    ]);
    assert_eq!(I2C_CLIENT_PEC, 0x04); // include/linux/i2c.h:333
}

/// master.c:199-219 and core.h:34/:141. Ten-bit mode sets CON bit 4 and TAR bit 12.
#[test]
fn target_setup_encodes_seven_and_ten_bit_targets() {
    assert_eq!(target_setup(Message { addr: 0x52, flags: 0, len: 1 }),
               TargetSetup { con_10bit_value: 0, tar_value: 0x52 });
    assert_eq!(target_setup(Message { addr: 0x2aa, flags: 0x0010, len: 1 }),
               TargetSetup { con_10bit_value: 0x10, tar_value: 0x12aa });
}

/// master.c:934-948. Base MASTER|SLAVE_DISABLE|RESTART_EN is 0x61; speed field is 0x2/0x4/0x6.
#[test]
fn master_configuration_uses_linux_speed_switch() {
    assert_eq!(master_config(STANDARD_FREQ_HZ), 0x63);
    assert_eq!(master_config(FAST_FREQ_HZ), 0x65);
    assert_eq!(master_config(FAST_PLUS_FREQ_HZ), 0x65, "default arm is FAST");
    assert_eq!(master_config(HIGH_SPEED_FREQ_HZ), 0x67);
}

/// master.c:829-855. Refusals preserve the actual addresses or directions Linux rejected.
#[test]
fn message_validation_names_address_and_restart_refusals() {
    let differing = [
        Message { addr: 0x50, flags: 0, len: 1 },
        Message { addr: 0x51, flags: I2C_M_RD, len: 1 },
    ];
    assert_eq!(validate_message(&differing, 0, false), Ok(()));
    assert_eq!(validate_message(&differing, 1, false), Err(
        MessageRefusal::InvalidTargetAddress { previous: 0x50, current: 0x51 }));
    let same_direction = [
        Message { addr: 0x50, flags: I2C_M_RD, len: 1 },
        Message { addr: 0x50, flags: I2C_M_RD, len: 1 },
    ];
    assert_eq!(validate_message(&same_direction, 1, false), Err(
        MessageRefusal::CannotEmitRestart { previous_read: true, current_read: true }));
    assert_eq!(validate_message(&same_direction, 1, true), Ok(()));
}

/// master.c:875-900 partitions through the first message carrying literal I2C_M_STOP = 0x8000.
#[test]
fn explicit_stop_partitions_a_message_array() {
    let messages = [
        Message { addr: 0x50, flags: 0, len: 1 },
        Message { addr: 0x50, flags: I2C_M_RD | I2C_M_STOP, len: 1 },
        Message { addr: 0x51, flags: 0, len: 1 },
    ];
    assert_eq!(next_part_len(&messages, true), Ok(2));
    assert_eq!(next_part_len(&messages[2..], true), Ok(1));
    assert_eq!(next_part_len(&[], true), Ok(0));
}
