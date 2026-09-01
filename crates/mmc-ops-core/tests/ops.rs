// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for MMC operation command descriptors.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c` and
//! `include/linux/mmc/{core.h,mmc.h}`. Copyright 2006-2007 Pierre Ossman and
//! the Linux MMC authors.

use mmc_ops_core::ops::*;

/// include/linux/mmc/mmc.h:32-:44. This module deliberately owns only the five
/// commands in the task; the already-landed command crate remains the owner of
/// broad opcode tables.
#[test]
fn operation_family_is_pinned_by_count_name_and_opcode() {
    let got: Vec<_> = OPERATIONS.iter().map(|op| (op.name, op.opcode)).collect();
    assert_eq!(
        got,
        [
            ("SEND_OP_COND", 1),
            ("SET_RELATIVE_ADDR", 3),
            ("SWITCH", 6),
            ("SEND_EXT_CSD", 8),
            ("SEND_STATUS", 13),
        ]
    );
}

/// include/linux/mmc/core.h:42-:52, :75-:78. Each component is pinned before
/// composites are checked, preventing compensating mutations.
#[test]
fn command_and_spi_response_flags_match_linux() {
    assert_eq!(CMD_AC, 0x00);
    assert_eq!(CMD_ADTC, 0x20);
    assert_eq!(CMD_BCR, 0x60);
    assert_ne!(
        CMD_AC, CMD_ADTC,
        "zero-valued AC must remain distinct from ADTC"
    );
    assert_ne!(
        CMD_AC, CMD_BCR,
        "zero-valued AC must remain distinct from BCR"
    );

    assert_eq!(RSP_SPI_S1, 0x080);
    assert_eq!(RSP_SPI_S2, 0x100);
    assert_eq!(RSP_SPI_B4, 0x200);
    assert_eq!(RSP_SPI_BUSY, 0x400);
    assert_eq!(RSP_SPI_R1, RSP_SPI_S1);
    assert_eq!(RSP_SPI_R1B, RSP_SPI_S1 | RSP_SPI_BUSY);
    assert_eq!(RSP_SPI_R2, RSP_SPI_S1 | RSP_SPI_S2);
    assert_eq!(RSP_SPI_R3, RSP_SPI_S1 | RSP_SPI_B4);
}

/// mmc_ops.c:73-:77 — native argument is RCA << 16, while SPI argument stays
/// zero; response is SPI R2 plus native R1 and AC.
#[test]
fn send_status_has_native_and_spi_vectors() {
    assert_eq!(
        send_status(0x1234, false),
        Command {
            opcode: 13,
            argument: 0x1234_0000,
            flags: 0x195
        }
    );
    assert_eq!(
        send_status(0x1234, true),
        Command {
            opcode: 13,
            argument: 0,
            flags: 0x195
        }
    );
}

/// mmc_ops.c:388-:389 and :303-:317 — CMD8, arg zero, R1 ADTC; the transfer
/// layer supplies one 512-byte read block.
#[test]
fn send_ext_csd_descriptor_matches_linux() {
    assert_eq!(
        send_ext_csd(),
        Command {
            opcode: 8,
            argument: 0,
            flags: 0x0b5
        }
    );
    assert_eq!(mmc_ops_core::command::ext_csd::EXT_CSD_LEN, 512); // mmc_ops.c:386
}

/// mmc_ops.c:240-:243 — native CMD1 carries OCR and R3/BCR; SPI carries zero.
#[test]
fn send_op_cond_has_native_and_spi_vectors() {
    assert_eq!(
        send_op_cond(0x40ff_8000, false),
        Command {
            opcode: 1,
            argument: 0x40ff_8000,
            flags: 0x0e1
        }
    );
    assert_eq!(
        send_op_cond(0x40ff_8000, true),
        Command {
            opcode: 1,
            argument: 0,
            flags: 0x0e1
        }
    );
}

/// mmc_ops.c:202-:222. Native completion is MMC_CARD_BUSY bit 31; SPI
/// completion is R1_SPI_IDLE bit 0 clearing. Zero-OCR native inquiry feeds the
/// response back with bit 30 set so repeated CMD1 keeps the device in idle.
#[test]
fn op_cond_poll_decodes_each_bus_mode_and_updates_the_argument() {
    assert!(op_cond_busy(0x7fff_8000, false));
    assert!(!op_cond_busy(0x8000_0000, false));
    assert!(op_cond_busy(0x0000_0001, true));
    assert!(!op_cond_busy(0x0000_0000, true));

    assert_eq!(next_op_cond_argument(0, 0x00ff_8000, false), 0x40ff_8000);
    assert_eq!(
        next_op_cond_argument(0x00ff_8000, 0x1234_5678, false),
        0x00ff_8000
    );
    assert_eq!(next_op_cond_argument(0, 0xffff_ffff, true), 0);
}

/// mmc_ops.c:261-:263 — CMD3 carries RCA in bits 31:16 with native R1 AC.
#[test]
fn set_relative_addr_matches_linux() {
    assert_eq!(
        set_relative_addr(0xbeef),
        Command {
            opcode: 3,
            argument: 0xbeef_0000,
            flags: 0x015
        }
    );
}
