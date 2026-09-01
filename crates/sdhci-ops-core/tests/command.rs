// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for command flags, ported from Linux `drivers/mmc/host/sdhci.c:1709-:1725` and
//! `include/linux/mmc/core.h:57-:66`. Original copyright (C) 2005-2008 Pierre Ossman and Linux MMC
//! authors.

use sdhci_ops_core::command::{command_flags, response_flags, ResponseType, RESPONSE_TYPES};

/// Linux defines exactly ten named native response macros at core.h:57-:66. Names are literal and
/// independent of the production table so deleting a production entry cannot delete the test.
#[test]
fn native_response_count_names_and_literals_match_linux() {
    let expected = [
        ("MMC_RSP_NONE", ResponseType::None, 0x00), // include/linux/mmc/core.h:57
        ("MMC_RSP_R1", ResponseType::R1, 0x15), // include/linux/mmc/core.h:58
        ("MMC_RSP_R1B", ResponseType::R1b, 0x1d), // include/linux/mmc/core.h:59
        ("MMC_RSP_R1B_NO_CRC", ResponseType::R1bNoCrc, 0x19), // core.h:60
        ("MMC_RSP_R2", ResponseType::R2, 0x07), // include/linux/mmc/core.h:61
        ("MMC_RSP_R3", ResponseType::R3, 0x01), // include/linux/mmc/core.h:62
        ("MMC_RSP_R4", ResponseType::R4, 0x01), // include/linux/mmc/core.h:63
        ("MMC_RSP_R5", ResponseType::R5, 0x15), // include/linux/mmc/core.h:64
        ("MMC_RSP_R6", ResponseType::R6, 0x15), // include/linux/mmc/core.h:65
        ("MMC_RSP_R7", ResponseType::R7, 0x15), // include/linux/mmc/core.h:66
    ];
    assert_eq!(RESPONSE_TYPES.len(), 10);
    for index in 0..10 {
        assert_eq!(RESPONSE_TYPES[index], (expected[index].0, expected[index].1));
        assert_eq!(response_flags(expected[index].1), expected[index].2);
    }
}

/// sdhci.c:1709-:1722 and sdhci.h:63-:71: response shape, CRC, and opcode checks.
#[test]
fn each_response_shape_composes_the_literal_command_flags() {
    assert_eq!(command_flags(ResponseType::None, false, 0), 0x00);
    assert_eq!(command_flags(ResponseType::R1, false, 0), 0x1a);
    assert_eq!(command_flags(ResponseType::R1b, false, 0), 0x1b);
    assert_eq!(command_flags(ResponseType::R1bNoCrc, false, 0), 0x13);
    assert_eq!(command_flags(ResponseType::R2, false, 0), 0x09);
    assert_eq!(command_flags(ResponseType::R3, false, 0), 0x02);
}

/// sdhci.c:1723-:1725: an attached transfer OR either tuning opcode sets Data Present Select.
#[test]
fn data_present_includes_tuning_commands_without_a_data_object() {
    assert_eq!(command_flags(ResponseType::R1, true, 17), 0x3a);
    assert_eq!(command_flags(ResponseType::R1, false, 19), 0x3a); // mmc.h:55
    assert_eq!(command_flags(ResponseType::R1, false, 21), 0x3a); // mmc.h:56
    assert_eq!(command_flags(ResponseType::R1, false, 18), 0x1a);
}
