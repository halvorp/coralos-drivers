// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for Linux `sdhci_set_transfer_mode()` (`drivers/mmc/host/sdhci.c:1460-:1498`).
//! Original copyright (C) 2005-2008 Pierre Ossman and Linux SDHCI/MMC authors.

use sdhci_ops_core::transfer::{
    decode_transfer_mode, transfer_mode, AutoCommand, DataDirection, TransferConfig, TransferMode,
    AUTO_COMMANDS,
};

fn config() -> TransferConfig {
    TransferConfig {
        opcode: 17,
        blocks: 1,
        direction: DataDirection::Write,
        use_dma: false,
        support_single: false,
        auto_command: AutoCommand::None,
        version_410_or_later: false,
        v4_mode: false,
    }
}

/// sdhci.c:1454-:1457 has three mutually exclusive outcomes. Pin count and names literally.
#[test]
fn automatic_command_count_and_names_are_pinned() {
    assert_eq!(AUTO_COMMANDS.len(), 3);
    assert_eq!(AUTO_COMMANDS[0], ("None", AutoCommand::None));
    assert_eq!(AUTO_COMMANDS[1], ("SDHCI_AUTO_CMD12", AutoCommand::Cmd12));
    assert_eq!(AUTO_COMMANDS[2], ("SDHCI_AUTO_CMD23", AutoCommand::Cmd23));
}

/// sdhci.c:1484-:1498 and sdhci.h:38-:44. This all-fields vector is literal: DMA 0x01, block-count
/// 0x02, Auto CMD23 0x08, read 0x10, multi 0x20 => 0x3b.
#[test]
fn multiblock_dma_read_with_cmd23_matches_linux_word_and_decodes() {
    let got = transfer_mode(TransferConfig {
        opcode: 18,
        blocks: 8,
        direction: DataDirection::Read,
        use_dma: true,
        auto_command: AutoCommand::Cmd23,
        ..config()
    });
    assert_eq!(got, TransferMode { word: 0x003b, cmd23_enable: None });
    assert_eq!(decode_transfer_mode(0x003b), (true, true, true, true, 0x0008));
}

#[test]
fn single_block_support_and_opcode_multi_rules_match_linux() {
    assert_eq!(transfer_mode(config()).word, 0x0002); // sdhci.c:1483-:1484
    assert_eq!(
        transfer_mode(TransferConfig { support_single: true, ..config() }).word,
        0x0000
    );
    assert_eq!(
        transfer_mode(TransferConfig { opcode: 25, support_single: true, ..config() }).word,
        0x0022
    ); // mmc.h:64; sdhci.c:1486-:1487
}

/// sdhci.c:1425-:1426 excludes SDIO CMD53 from Auto CMD12.
#[test]
fn cmd53_refuses_auto_cmd12_but_not_auto_cmd23() {
    let cmd12 = transfer_mode(TransferConfig {
        opcode: 53,
        blocks: 2,
        auto_command: AutoCommand::Cmd12,
        ..config()
    });
    assert_eq!(cmd12.word, 0x0022);
    let cmd23 = transfer_mode(TransferConfig {
        opcode: 53,
        blocks: 2,
        auto_command: AutoCommand::Cmd23,
        ..config()
    });
    assert_eq!(cmd23.word, 0x002a);
}

/// sdhci.c:1429-:1445: v4.10 + v4 mode emits AUTO_SEL (0x0c), then separately controls CMD23.
#[test]
fn v410_auto_select_pins_word_and_cmd23_selector() {
    let common = TransferConfig {
        blocks: 2,
        version_410_or_later: true,
        v4_mode: true,
        ..config()
    };
    assert_eq!(
        transfer_mode(TransferConfig { auto_command: AutoCommand::Cmd12, ..common }),
        TransferMode { word: 0x002e, cmd23_enable: Some(false) }
    );
    assert_eq!(
        transfer_mode(TransferConfig { auto_command: AutoCommand::Cmd23, ..common }),
        TransferMode { word: 0x002e, cmd23_enable: Some(true) }
    );
}
