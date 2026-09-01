// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for CMD6 encoding and R1B policy.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c`, `drivers/mmc/core/mmc.c`,
//! and `include/linux/mmc/mmc.h`. Copyright 2006-2007 Pierre Ossman and the
//! Linux MMC authors.

use mmc_ops_core::ops::Command;
use mmc_ops_core::switch::*;

/// include/linux/mmc/mmc.h:429-:432. All four enum-like access modes are
/// driven through the encoder; CommandSet's zero mode is thereby behaviourally
/// distinguished rather than only ORed into an expression where it vanishes.
#[test]
fn every_access_mode_is_pinned_by_count_name_and_encoding() {
    assert_eq!(
        ACCESS_MODES
            .iter()
            .map(|mode| (mode.name, mode.value))
            .collect::<Vec<_>>(),
        [
            ("CMD_SET", 0x00),
            ("SET_BITS", 0x01),
            ("CLEAR_BITS", 0x02),
            ("WRITE_BYTE", 0x03),
        ]
    );
    assert_eq!(
        encode_argument(AccessMode::CommandSet, 0xa5, 0x5a, 0x04),
        0x00a5_5a04
    );
    assert_eq!(
        encode_argument(AccessMode::SetBits, 0xa5, 0x5a, 0x04),
        0x01a5_5a04
    );
    assert_eq!(
        encode_argument(AccessMode::ClearBits, 0xa5, 0x5a, 0x04),
        0x02a5_5a04
    );
    assert_eq!(
        encode_argument(AccessMode::WriteByte, 0xa5, 0x5a, 0x04),
        0x03a5_5a04
    );
}

/// include/linux/mmc/mmc.h:345-:347. Each independent command-set bit is
/// pinned directly and driven through the low-byte field.
#[test]
fn every_command_set_is_pinned_by_count_name_and_encoding() {
    assert_eq!(
        COMMAND_SETS
            .iter()
            .map(|set| (set.name, set.value))
            .collect::<Vec<_>>(),
        [("NORMAL", 1), ("SECURE", 2), ("CPSECURE", 4)]
    );
    assert_eq!(
        encode_argument(AccessMode::WriteByte, 0, 0, CommandSet::Normal as u8),
        0x0300_0001
    );
    assert_eq!(
        encode_argument(AccessMode::WriteByte, 0, 0, CommandSet::Secure as u8),
        0x0300_0002
    );
    assert_eq!(
        encode_argument(
            AccessMode::WriteByte,
            0,
            0,
            CommandSet::ContentProtectionSecure as u8
        ),
        0x0300_0004
    );
}

/// mmc_ops.c:999-:1001, :1024-:1026, :1061-:1062 and mmc.h:256, :269-:270.
#[test]
fn every_switch_target_in_mmc_ops_is_pinned_by_count_name_and_index() {
    assert_eq!(
        EXT_CSD_TARGETS
            .iter()
            .map(|field| (field.name, field.index))
            .collect::<Vec<_>>(),
        [
            ("CMDQ_MODE_EN", 15),
            ("BKOPS_START", 164),
            ("SANITIZE_START", 165)
        ]
    );
    assert_eq!(ExtCsdIndex::CmdqModeEn as u8, 15);
    assert_eq!(ExtCsdIndex::BkopsStart as u8, 164);
    assert_eq!(ExtCsdIndex::SanitizeStart as u8, 165);
    assert_eq!(GENERIC_CMD6_TIME_INDEX, 248); // include/linux/mmc/mmc.h:310
}

/// mmc_ops.c:621-:624 — Linux's production path writes one EXT_CSD byte:
/// `(WRITE_BYTE << 24) | (index << 16) | (value << 8) | set`.
#[test]
fn switch_argument_places_each_byte_in_the_documented_field() {
    assert_eq!(
        encode_argument(AccessMode::WriteByte, 185, 2, 1),
        0x03b9_0201
    );
    assert_eq!(
        encode_argument(AccessMode::WriteByte, 15, 1, 1),
        0x030f_0101
    );
}

/// mmc.c:597-:602 and mmc_ops.c:614-:618. The card's raw byte is in 10ms
/// units and is selected exactly when the caller supplies timeout zero.
#[test]
fn timeout_is_derived_from_the_cards_generic_cmd6_time() {
    assert_eq!(GENERIC_CMD6_TIME_UNIT_MS, 10);
    assert_eq!(generic_cmd6_timeout_ms(0), 0);
    assert_eq!(generic_cmd6_timeout_ms(1), 10);
    assert_eq!(generic_cmd6_timeout_ms(25), 250);
    assert_eq!(generic_cmd6_timeout_ms(255), 2_550);
    assert_eq!(effective_timeout_ms(0, 25), 250);
    assert_eq!(effective_timeout_ms(700, 25), 700);
}

/// mmc_ops.c:569-:585. A finite host limit downgrades only when timeout is
/// STRICTLY above it and the host does not require R1B.
#[test]
fn busy_response_policy_covers_both_sides_of_the_host_limit() {
    let host = BusyHost {
        needs_r1b: false,
        max_busy_timeout_ms: 100,
    };
    let at_limit = prepare_busy_response(host, 100);
    assert_eq!(at_limit.flags, 0x49d);
    assert_eq!(at_limit.busy_timeout_ms, 100);
    assert!(at_limit.uses_r1b);

    let above_limit = prepare_busy_response(host, 101);
    assert_eq!(above_limit.flags, 0x095);
    assert_eq!(above_limit.busy_timeout_ms, 0);
    assert!(!above_limit.uses_r1b);

    let required = prepare_busy_response(
        BusyHost {
            needs_r1b: true,
            max_busy_timeout_ms: 1,
        },
        2_550,
    );
    assert_eq!(required.flags, 0x49d);
    assert_eq!(required.busy_timeout_ms, 2_550);
    assert!(required.uses_r1b);

    let unlimited = prepare_busy_response(
        BusyHost {
            needs_r1b: false,
            max_busy_timeout_ms: 0,
        },
        2_550,
    );
    assert!(unlimited.uses_r1b);
}

/// include/linux/mmc/mmc.h:37 says CMD6 is R1b; mmc_ops.c:621-:626 builds the
/// command using the timeout policy.
#[test]
fn switch_command_is_cmd6_write_byte_r1b() {
    let (command, response) = switch_command(
        AccessMode::WriteByte,
        ExtCsdIndex::CmdqModeEn,
        1,
        CommandSet::Normal,
        250,
        BusyHost {
            needs_r1b: false,
            max_busy_timeout_ms: 1_000,
        },
    );
    assert_eq!(
        command,
        Command {
            opcode: 6,
            argument: 0x030f_0101,
            flags: 0x49d
        }
    );
    assert!(response.uses_r1b);
    assert_eq!(response.busy_timeout_ms, 250);
}
