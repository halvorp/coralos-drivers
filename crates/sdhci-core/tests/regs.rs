// SPDX-License-Identifier: GPL-2.0-only

//! Literal vectors for definitions ported from Linux `drivers/mmc/host/sdhci.h` and
//! `include/linux/mmc/core.h`.
//!
//! Original copyright: Copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved.

use sdhci_core::regs::*;

#[test]
fn register_offsets_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:26,30,33,35,37,53,76,78,80,110,124,137,139,144,156,158,
    // 163-165,217,226,251,274,292,320,329-330.
    let expected: [(&str, u16); 27] = [
        ("SDHCI_DMA_ADDRESS", 0x00),
        ("SDHCI_BLOCK_SIZE", 0x04),
        ("SDHCI_BLOCK_COUNT", 0x06),
        ("SDHCI_ARGUMENT", 0x08),
        ("SDHCI_TRANSFER_MODE", 0x0C),
        ("SDHCI_COMMAND", 0x0E),
        ("SDHCI_RESPONSE", 0x10),
        ("SDHCI_BUFFER", 0x20),
        ("SDHCI_PRESENT_STATE", 0x24),
        ("SDHCI_HOST_CONTROL", 0x28),
        ("SDHCI_POWER_CONTROL", 0x29),
        ("SDHCI_BLOCK_GAP_CONTROL", 0x2A),
        ("SDHCI_WAKE_UP_CONTROL", 0x2B),
        ("SDHCI_CLOCK_CONTROL", 0x2C),
        ("SDHCI_TIMEOUT_CONTROL", 0x2E),
        ("SDHCI_SOFTWARE_RESET", 0x2F),
        ("SDHCI_INT_STATUS", 0x30),
        ("SDHCI_INT_ENABLE", 0x34),
        ("SDHCI_SIGNAL_ENABLE", 0x38),
        ("SDHCI_AUTO_CMD_STATUS", 0x3C),
        ("SDHCI_HOST_CONTROL2", 0x3E),
        ("SDHCI_CAPABILITIES", 0x40),
        ("SDHCI_CAPABILITIES_1", 0x44),
        ("SDHCI_MAX_CURRENT", 0x48),
        ("SDHCI_ADMA_ERROR", 0x54),
        ("SDHCI_ADMA_ADDRESS", 0x58),
        ("SDHCI_ADMA_ADDRESS_HI", 0x5C),
    ];
    let actual = [
        ("SDHCI_DMA_ADDRESS", SDHCI_DMA_ADDRESS),
        ("SDHCI_BLOCK_SIZE", SDHCI_BLOCK_SIZE),
        ("SDHCI_BLOCK_COUNT", SDHCI_BLOCK_COUNT),
        ("SDHCI_ARGUMENT", SDHCI_ARGUMENT),
        ("SDHCI_TRANSFER_MODE", SDHCI_TRANSFER_MODE),
        ("SDHCI_COMMAND", SDHCI_COMMAND),
        ("SDHCI_RESPONSE", SDHCI_RESPONSE),
        ("SDHCI_BUFFER", SDHCI_BUFFER),
        ("SDHCI_PRESENT_STATE", SDHCI_PRESENT_STATE),
        ("SDHCI_HOST_CONTROL", SDHCI_HOST_CONTROL),
        ("SDHCI_POWER_CONTROL", SDHCI_POWER_CONTROL),
        ("SDHCI_BLOCK_GAP_CONTROL", SDHCI_BLOCK_GAP_CONTROL),
        ("SDHCI_WAKE_UP_CONTROL", SDHCI_WAKE_UP_CONTROL),
        ("SDHCI_CLOCK_CONTROL", SDHCI_CLOCK_CONTROL),
        ("SDHCI_TIMEOUT_CONTROL", SDHCI_TIMEOUT_CONTROL),
        ("SDHCI_SOFTWARE_RESET", SDHCI_SOFTWARE_RESET),
        ("SDHCI_INT_STATUS", SDHCI_INT_STATUS),
        ("SDHCI_INT_ENABLE", SDHCI_INT_ENABLE),
        ("SDHCI_SIGNAL_ENABLE", SDHCI_SIGNAL_ENABLE),
        ("SDHCI_AUTO_CMD_STATUS", SDHCI_AUTO_CMD_STATUS),
        ("SDHCI_HOST_CONTROL2", SDHCI_HOST_CONTROL2),
        ("SDHCI_CAPABILITIES", SDHCI_CAPABILITIES),
        ("SDHCI_CAPABILITIES_1", SDHCI_CAPABILITIES_1),
        ("SDHCI_MAX_CURRENT", SDHCI_MAX_CURRENT),
        ("SDHCI_ADMA_ERROR", SDHCI_ADMA_ERROR),
        ("SDHCI_ADMA_ADDRESS", SDHCI_ADMA_ADDRESS),
        ("SDHCI_ADMA_ADDRESS_HI", SDHCI_ADMA_ADDRESS_HI),
    ];
    assert_eq!(actual.len(), 27, "Linux register-offset count changed");
    assert_eq!(actual, expected);
}

#[test]
fn reset_and_interrupt_bits_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:159-161,166-175,178,180-192,195.
    let expected: [(&str, u32); 28] = [
        ("SDHCI_RESET_ALL", 0x00000001),
        ("SDHCI_RESET_CMD", 0x00000002),
        ("SDHCI_RESET_DATA", 0x00000004),
        ("SDHCI_INT_RESPONSE", 0x00000001),
        ("SDHCI_INT_DATA_END", 0x00000002),
        ("SDHCI_INT_BLK_GAP", 0x00000004),
        ("SDHCI_INT_DMA_END", 0x00000008),
        ("SDHCI_INT_SPACE_AVAIL", 0x00000010),
        ("SDHCI_INT_DATA_AVAIL", 0x00000020),
        ("SDHCI_INT_CARD_INSERT", 0x00000040),
        ("SDHCI_INT_CARD_REMOVE", 0x00000080),
        ("SDHCI_INT_CARD_INT", 0x00000100),
        ("SDHCI_INT_RETUNE", 0x00001000),
        ("SDHCI_INT_FX_EVENT", 0x00002000),
        ("SDHCI_INT_CQE", 0x00004000),
        ("SDHCI_INT_ERROR", 0x00008000),
        ("SDHCI_INT_TIMEOUT", 0x00010000),
        ("SDHCI_INT_CRC", 0x00020000),
        ("SDHCI_INT_END_BIT", 0x00040000),
        ("SDHCI_INT_INDEX", 0x00080000),
        ("SDHCI_INT_DATA_TIMEOUT", 0x00100000),
        ("SDHCI_INT_DATA_CRC", 0x00200000),
        ("SDHCI_INT_DATA_END_BIT", 0x00400000),
        ("SDHCI_INT_BUS_POWER", 0x00800000),
        ("SDHCI_INT_AUTO_CMD_ERR", 0x01000000),
        ("SDHCI_INT_ADMA_ERROR", 0x02000000),
        ("SDHCI_INT_TUNING_ERROR", 0x04000000),
        ("SDHCI_INT_RESP_ERR", 0x08000000),
    ];
    let actual = [
        ("SDHCI_RESET_ALL", SDHCI_RESET_ALL as u32),
        ("SDHCI_RESET_CMD", SDHCI_RESET_CMD as u32),
        ("SDHCI_RESET_DATA", SDHCI_RESET_DATA as u32),
        ("SDHCI_INT_RESPONSE", SDHCI_INT_RESPONSE),
        ("SDHCI_INT_DATA_END", SDHCI_INT_DATA_END),
        ("SDHCI_INT_BLK_GAP", SDHCI_INT_BLK_GAP),
        ("SDHCI_INT_DMA_END", SDHCI_INT_DMA_END),
        ("SDHCI_INT_SPACE_AVAIL", SDHCI_INT_SPACE_AVAIL),
        ("SDHCI_INT_DATA_AVAIL", SDHCI_INT_DATA_AVAIL),
        ("SDHCI_INT_CARD_INSERT", SDHCI_INT_CARD_INSERT),
        ("SDHCI_INT_CARD_REMOVE", SDHCI_INT_CARD_REMOVE),
        ("SDHCI_INT_CARD_INT", SDHCI_INT_CARD_INT),
        ("SDHCI_INT_RETUNE", SDHCI_INT_RETUNE),
        ("SDHCI_INT_FX_EVENT", SDHCI_INT_FX_EVENT),
        ("SDHCI_INT_CQE", SDHCI_INT_CQE),
        ("SDHCI_INT_ERROR", SDHCI_INT_ERROR),
        ("SDHCI_INT_TIMEOUT", SDHCI_INT_TIMEOUT),
        ("SDHCI_INT_CRC", SDHCI_INT_CRC),
        ("SDHCI_INT_END_BIT", SDHCI_INT_END_BIT),
        ("SDHCI_INT_INDEX", SDHCI_INT_INDEX),
        ("SDHCI_INT_DATA_TIMEOUT", SDHCI_INT_DATA_TIMEOUT),
        ("SDHCI_INT_DATA_CRC", SDHCI_INT_DATA_CRC),
        ("SDHCI_INT_DATA_END_BIT", SDHCI_INT_DATA_END_BIT),
        ("SDHCI_INT_BUS_POWER", SDHCI_INT_BUS_POWER),
        ("SDHCI_INT_AUTO_CMD_ERR", SDHCI_INT_AUTO_CMD_ERR),
        ("SDHCI_INT_ADMA_ERROR", SDHCI_INT_ADMA_ERROR),
        ("SDHCI_INT_TUNING_ERROR", SDHCI_INT_TUNING_ERROR),
        ("SDHCI_INT_RESP_ERR", SDHCI_INT_RESP_ERR),
    ];
    assert_eq!(actual.len(), 28, "Linux reset/interrupt member count changed");
    assert_eq!(actual, expected);
}

#[test]
fn interrupt_composites_match_linux_and_named_members() {
    // Linux drivers/mmc/host/sdhci.h:197-215.
    assert_eq!(SDHCI_INT_NORMAL_MASK, 0x00007FFF);
    assert_eq!(SDHCI_INT_ERROR_MASK, 0xFFFF8000);
    assert_eq!(SDHCI_INT_ALL_MASK, 0xFFFF_FFFF);

    assert_eq!(
        SDHCI_INT_CMD_MASK,
        SDHCI_INT_RESPONSE
            | SDHCI_INT_TIMEOUT
            | SDHCI_INT_CRC
            | SDHCI_INT_END_BIT
            | SDHCI_INT_INDEX
            | SDHCI_INT_AUTO_CMD_ERR
    );
    assert_eq!(SDHCI_INT_CMD_MASK, 0x010F_0001); // sdhci.h:200-202

    assert_eq!(
        SDHCI_INT_DATA_MASK,
        SDHCI_INT_DATA_END
            | SDHCI_INT_DMA_END
            | SDHCI_INT_DATA_AVAIL
            | SDHCI_INT_SPACE_AVAIL
            | SDHCI_INT_DATA_TIMEOUT
            | SDHCI_INT_DATA_CRC
            | SDHCI_INT_DATA_END_BIT
            | SDHCI_INT_ADMA_ERROR
            | SDHCI_INT_BLK_GAP
            | SDHCI_INT_TUNING_ERROR
    );
    assert_eq!(SDHCI_INT_DATA_MASK, 0x0670_003E); // sdhci.h:203-207

    assert_eq!(
        SDHCI_CQE_INT_ERR_MASK,
        SDHCI_INT_ADMA_ERROR
            | SDHCI_INT_BUS_POWER
            | SDHCI_INT_DATA_END_BIT
            | SDHCI_INT_DATA_CRC
            | SDHCI_INT_DATA_TIMEOUT
            | SDHCI_INT_INDEX
            | SDHCI_INT_END_BIT
            | SDHCI_INT_CRC
            | SDHCI_INT_TIMEOUT
    );
    assert_eq!(SDHCI_CQE_INT_ERR_MASK, 0x02FF_0000); // sdhci.h:210-213
    assert_eq!(SDHCI_CQE_INT_MASK, SDHCI_CQE_INT_ERR_MASK | SDHCI_INT_CQE);
    assert_eq!(SDHCI_CQE_INT_MASK, 0x02FF_4000); // sdhci.h:215
}

#[test]
fn present_state_and_auto_cmd_bits_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:81-95 and 218-224.
    // SDHCI_DAT_ACTIVE is the SDHCI specification's DAT Line Active bit retained by this port.
    let expected: [(&str, u32); 15] = [
        ("SDHCI_CMD_INHIBIT", 0x00000001),
        ("SDHCI_DATA_INHIBIT", 0x00000002),
        ("SDHCI_DAT_ACTIVE", 0x00000004),
        ("SDHCI_DOING_WRITE", 0x00000100),
        ("SDHCI_DOING_READ", 0x00000200),
        ("SDHCI_SPACE_AVAILABLE", 0x00000400),
        ("SDHCI_DATA_AVAILABLE", 0x00000800),
        ("SDHCI_CARD_PRESENT", 0x00010000),
        ("SDHCI_CD_STABLE", 0x00020000),
        ("SDHCI_WRITE_PROTECT", 0x00080000),
        ("SDHCI_AUTO_CMD_TIMEOUT", 0x00000002),
        ("SDHCI_AUTO_CMD_CRC", 0x00000004),
        ("SDHCI_AUTO_CMD_END_BIT", 0x00000008),
        ("SDHCI_AUTO_CMD_INDEX", 0x00000010),
        ("SDHCI_AUTO_CMD_RESP_ERR", 0x00000020),
    ];
    let actual = [
        ("SDHCI_CMD_INHIBIT", SDHCI_CMD_INHIBIT),
        ("SDHCI_DATA_INHIBIT", SDHCI_DATA_INHIBIT),
        ("SDHCI_DAT_ACTIVE", SDHCI_DAT_ACTIVE),
        ("SDHCI_DOING_WRITE", SDHCI_DOING_WRITE),
        ("SDHCI_DOING_READ", SDHCI_DOING_READ),
        ("SDHCI_SPACE_AVAILABLE", SDHCI_SPACE_AVAILABLE),
        ("SDHCI_DATA_AVAILABLE", SDHCI_DATA_AVAILABLE),
        ("SDHCI_CARD_PRESENT", SDHCI_CARD_PRESENT),
        ("SDHCI_CD_STABLE", SDHCI_CD_STABLE),
        ("SDHCI_WRITE_PROTECT", SDHCI_WRITE_PROTECT),
        ("SDHCI_AUTO_CMD_TIMEOUT", SDHCI_AUTO_CMD_TIMEOUT as u32),
        ("SDHCI_AUTO_CMD_CRC", SDHCI_AUTO_CMD_CRC as u32),
        ("SDHCI_AUTO_CMD_END_BIT", SDHCI_AUTO_CMD_END_BIT as u32),
        ("SDHCI_AUTO_CMD_INDEX", SDHCI_AUTO_CMD_INDEX as u32),
        ("SDHCI_AUTO_CMD_RESP_ERR", SDHCI_AUTO_CMD_RESP_ERR as u32),
    ];
    assert_eq!(actual.len(), 15, "ported present-state/Auto-CMD count changed");
    assert_eq!(actual, expected);
}

#[test]
fn host_control2_uhs_family_and_tuning_bits_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:227-230,241-242.
    assert_eq!(SDHCI_CTRL_UHS_MASK, 0x0007);
    let cases = [
        ("SDR12", SDHCI_CTRL_UHS_SDR12, 0x0000u16),
        ("SDR25", SDHCI_CTRL_UHS_SDR25, 0x0001u16),
        ("SDR50", SDHCI_CTRL_UHS_SDR50, 0x0002u16),
    ];
    assert_eq!(cases.len(), 3, "ported UHS mode count changed");
    for (name, encoded, expected) in cases {
        assert_eq!(encoded & SDHCI_CTRL_UHS_MASK, expected, "wrong UHS classification for {name}");
    }
    // A zero-valued mode is pinned behaviourally: it selects SDR12 and cannot alias its siblings.
    assert_eq!(SDHCI_CTRL_UHS_SDR12 & SDHCI_CTRL_UHS_MASK, 0x0000);
    assert_ne!(SDHCI_CTRL_UHS_SDR12, SDHCI_CTRL_UHS_SDR25);
    assert_ne!(SDHCI_CTRL_UHS_SDR12, SDHCI_CTRL_UHS_SDR50);
    assert_eq!(SDHCI_CTRL_EXEC_TUNING, 0x0040);
    assert_eq!(SDHCI_CTRL_TUNED_CLK, 0x0080);
}

#[test]
fn transfer_mode_family_matches_linux() {
    // Linux drivers/mmc/host/sdhci.h:38-44.
    let expected: [(&str, u16); 7] = [
        ("SDHCI_TRNS_DMA", 0x0001),
        ("SDHCI_TRNS_BLK_CNT_EN", 0x0002),
        ("SDHCI_TRNS_AUTO_CMD12", 0x0004),
        ("SDHCI_TRNS_AUTO_CMD23", 0x0008),
        ("SDHCI_TRNS_READ", 0x0010),
        ("SDHCI_TRNS_MULTI", 0x0020),
        ("SDHCI_TRNS_AUTO_SEL", 0x000C),
    ];
    let actual = [
        ("SDHCI_TRNS_DMA", SDHCI_TRNS_DMA),
        ("SDHCI_TRNS_BLK_CNT_EN", SDHCI_TRNS_BLK_CNT_EN),
        ("SDHCI_TRNS_AUTO_CMD12", SDHCI_TRNS_AUTO_CMD12),
        ("SDHCI_TRNS_AUTO_CMD23", SDHCI_TRNS_AUTO_CMD23),
        ("SDHCI_TRNS_READ", SDHCI_TRNS_READ),
        ("SDHCI_TRNS_MULTI", SDHCI_TRNS_MULTI),
        ("SDHCI_TRNS_AUTO_SEL", SDHCI_TRNS_AUTO_SEL),
    ];
    assert_eq!(actual.len(), 7, "ported transfer-mode member count changed");
    assert_eq!(actual, expected);
    assert_eq!(SDHCI_TRNS_AUTO_SEL, SDHCI_TRNS_AUTO_CMD12 | SDHCI_TRNS_AUTO_CMD23);
}

#[test]
fn command_response_family_and_fields_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:54,63-74.
    assert_eq!(SDHCI_CMD_RESP_MASK, 0x0003);
    let responses = [
        ("none", SDHCI_CMD_RESP_NONE, 0x0000u16),
        ("long", SDHCI_CMD_RESP_LONG, 0x0001u16),
        ("short", SDHCI_CMD_RESP_SHORT, 0x0002u16),
        ("short busy", SDHCI_CMD_RESP_SHORT_BUSY, 0x0003u16),
    ];
    assert_eq!(responses.len(), 4, "Linux command-response encoding count changed");
    for (name, encoded, expected) in responses {
        assert_eq!(encoded & SDHCI_CMD_RESP_MASK, expected, "wrong response classification for {name}");
    }
    // The zero encoding must select NONE and remain distinct from every response-bearing encoding.
    assert_eq!(SDHCI_CMD_RESP_NONE & SDHCI_CMD_RESP_MASK, 0x0000);
    assert_ne!(SDHCI_CMD_RESP_NONE, SDHCI_CMD_RESP_LONG);
    assert_ne!(SDHCI_CMD_RESP_NONE, SDHCI_CMD_RESP_SHORT);
    assert_ne!(SDHCI_CMD_RESP_NONE, SDHCI_CMD_RESP_SHORT_BUSY);
    assert_eq!(SDHCI_CMD_CRC, 0x0008);
    assert_eq!(SDHCI_CMD_INDEX, 0x0010);
    assert_eq!(SDHCI_CMD_DATA, 0x0020);
    assert_eq!(SDHCI_CMD_ABORTCMD, 0x00C0);

    // Real encode/decode vectors for every public command helper (sdhci.h:73-74).
    assert_eq!(SDHCI_MAKE_CMD(0x12, SDHCI_CMD_RESP_SHORT | SDHCI_CMD_CRC), 0x120A);
    assert_eq!(SDHCI_GET_CMD(0x3F02), 0x003F);
}

#[test]
fn primary_quirk_family_matches_linux() {
    // Linux drivers/mmc/host/sdhci.h:435-491.
    let expected: [(&str, u32); 29] = [
        ("SDHCI_QUIRK_CLOCK_BEFORE_RESET", 0x00000001),
        ("SDHCI_QUIRK_FORCE_DMA", 0x00000002),
        ("SDHCI_QUIRK_NO_CARD_NO_RESET", 0x00000004),
        ("SDHCI_QUIRK_SINGLE_POWER_WRITE", 0x00000008),
        ("SDHCI_QUIRK_BROKEN_DMA", 0x00000020),
        ("SDHCI_QUIRK_BROKEN_ADMA", 0x00000040),
        ("SDHCI_QUIRK_32BIT_DMA_ADDR", 0x00000080),
        ("SDHCI_QUIRK_32BIT_DMA_SIZE", 0x00000100),
        ("SDHCI_QUIRK_32BIT_ADMA_SIZE", 0x00000200),
        ("SDHCI_QUIRK_RESET_AFTER_REQUEST", 0x00000400),
        ("SDHCI_QUIRK_NO_SIMULT_VDD_AND_POWER", 0x00000800),
        ("SDHCI_QUIRK_BROKEN_TIMEOUT_VAL", 0x00001000),
        ("SDHCI_QUIRK_BROKEN_SMALL_PIO", 0x00002000),
        ("SDHCI_QUIRK_NO_BUSY_IRQ", 0x00004000),
        ("SDHCI_QUIRK_BROKEN_CARD_DETECTION", 0x00008000),
        ("SDHCI_QUIRK_INVERTED_WRITE_PROTECT", 0x00010000),
        ("SDHCI_QUIRK_BROKEN_CQE", 0x00020000),
        ("SDHCI_QUIRK_PIO_NEEDS_DELAY", 0x00040000),
        ("SDHCI_QUIRK_NO_LED", 0x00080000),
        ("SDHCI_QUIRK_FORCE_BLK_SZ_2048", 0x00100000),
        ("SDHCI_QUIRK_NO_MULTIBLOCK", 0x00200000),
        ("SDHCI_QUIRK_FORCE_1_BIT_DATA", 0x00400000),
        ("SDHCI_QUIRK_DELAY_AFTER_POWER", 0x00800000),
        ("SDHCI_QUIRK_DATA_TIMEOUT_USES_SDCLK", 0x01000000),
        ("SDHCI_QUIRK_CAP_CLOCK_BASE_BROKEN", 0x02000000),
        ("SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC", 0x04000000),
        ("SDHCI_QUIRK_MULTIBLOCK_READ_ACMD12", 0x10000000),
        ("SDHCI_QUIRK_NO_HISPD_BIT", 0x20000000),
        ("SDHCI_QUIRK_BROKEN_ADMA_ZEROLEN_DESC", 0x40000000),
    ];
    let actual = [
        ("SDHCI_QUIRK_CLOCK_BEFORE_RESET", SDHCI_QUIRK_CLOCK_BEFORE_RESET),
        ("SDHCI_QUIRK_FORCE_DMA", SDHCI_QUIRK_FORCE_DMA),
        ("SDHCI_QUIRK_NO_CARD_NO_RESET", SDHCI_QUIRK_NO_CARD_NO_RESET),
        ("SDHCI_QUIRK_SINGLE_POWER_WRITE", SDHCI_QUIRK_SINGLE_POWER_WRITE),
        ("SDHCI_QUIRK_BROKEN_DMA", SDHCI_QUIRK_BROKEN_DMA),
        ("SDHCI_QUIRK_BROKEN_ADMA", SDHCI_QUIRK_BROKEN_ADMA),
        ("SDHCI_QUIRK_32BIT_DMA_ADDR", SDHCI_QUIRK_32BIT_DMA_ADDR),
        ("SDHCI_QUIRK_32BIT_DMA_SIZE", SDHCI_QUIRK_32BIT_DMA_SIZE),
        ("SDHCI_QUIRK_32BIT_ADMA_SIZE", SDHCI_QUIRK_32BIT_ADMA_SIZE),
        ("SDHCI_QUIRK_RESET_AFTER_REQUEST", SDHCI_QUIRK_RESET_AFTER_REQUEST),
        ("SDHCI_QUIRK_NO_SIMULT_VDD_AND_POWER", SDHCI_QUIRK_NO_SIMULT_VDD_AND_POWER),
        ("SDHCI_QUIRK_BROKEN_TIMEOUT_VAL", SDHCI_QUIRK_BROKEN_TIMEOUT_VAL),
        ("SDHCI_QUIRK_BROKEN_SMALL_PIO", SDHCI_QUIRK_BROKEN_SMALL_PIO),
        ("SDHCI_QUIRK_NO_BUSY_IRQ", SDHCI_QUIRK_NO_BUSY_IRQ),
        ("SDHCI_QUIRK_BROKEN_CARD_DETECTION", SDHCI_QUIRK_BROKEN_CARD_DETECTION),
        ("SDHCI_QUIRK_INVERTED_WRITE_PROTECT", SDHCI_QUIRK_INVERTED_WRITE_PROTECT),
        ("SDHCI_QUIRK_BROKEN_CQE", SDHCI_QUIRK_BROKEN_CQE),
        ("SDHCI_QUIRK_PIO_NEEDS_DELAY", SDHCI_QUIRK_PIO_NEEDS_DELAY),
        ("SDHCI_QUIRK_NO_LED", SDHCI_QUIRK_NO_LED),
        ("SDHCI_QUIRK_FORCE_BLK_SZ_2048", SDHCI_QUIRK_FORCE_BLK_SZ_2048),
        ("SDHCI_QUIRK_NO_MULTIBLOCK", SDHCI_QUIRK_NO_MULTIBLOCK),
        ("SDHCI_QUIRK_FORCE_1_BIT_DATA", SDHCI_QUIRK_FORCE_1_BIT_DATA),
        ("SDHCI_QUIRK_DELAY_AFTER_POWER", SDHCI_QUIRK_DELAY_AFTER_POWER),
        ("SDHCI_QUIRK_DATA_TIMEOUT_USES_SDCLK", SDHCI_QUIRK_DATA_TIMEOUT_USES_SDCLK),
        ("SDHCI_QUIRK_CAP_CLOCK_BASE_BROKEN", SDHCI_QUIRK_CAP_CLOCK_BASE_BROKEN),
        ("SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC", SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC),
        ("SDHCI_QUIRK_MULTIBLOCK_READ_ACMD12", SDHCI_QUIRK_MULTIBLOCK_READ_ACMD12),
        ("SDHCI_QUIRK_NO_HISPD_BIT", SDHCI_QUIRK_NO_HISPD_BIT),
        ("SDHCI_QUIRK_BROKEN_ADMA_ZEROLEN_DESC", SDHCI_QUIRK_BROKEN_ADMA_ZEROLEN_DESC),
    ];
    assert_eq!(actual.len(), 29, "Linux primary quirk count changed");
    assert_eq!(actual, expected);
}

#[test]
fn secondary_quirk_family_matches_linux() {
    // Linux drivers/mmc/host/sdhci.h:495-538.
    let expected: [(&str, u32); 20] = [
        ("SDHCI_QUIRK2_HOST_OFF_CARD_ON", 0x00000001),
        ("SDHCI_QUIRK2_HOST_NO_CMD23", 0x00000002),
        ("SDHCI_QUIRK2_NO_1_8_V", 0x00000004),
        ("SDHCI_QUIRK2_PRESET_VALUE_BROKEN", 0x00000008),
        ("SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON", 0x00000010),
        ("SDHCI_QUIRK2_BROKEN_HOST_CONTROL", 0x00000020),
        ("SDHCI_QUIRK2_BROKEN_HS200", 0x00000040),
        ("SDHCI_QUIRK2_BROKEN_DDR50", 0x00000080),
        ("SDHCI_QUIRK2_STOP_WITH_TC", 0x00000100),
        ("SDHCI_QUIRK2_BROKEN_64_BIT_DMA", 0x00000200),
        ("SDHCI_QUIRK2_CLEAR_TRANSFERMODE_REG_BEFORE_CMD", 0x00000400),
        ("SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400", 0x00000800),
        ("SDHCI_QUIRK2_TUNING_WORK_AROUND", 0x00001000),
        ("SDHCI_QUIRK2_SUPPORT_SINGLE", 0x00002000),
        ("SDHCI_QUIRK2_ACMD23_BROKEN", 0x00004000),
        ("SDHCI_QUIRK2_CLOCK_DIV_ZERO_BROKEN", 0x00008000),
        ("SDHCI_QUIRK2_RSP_136_HAS_CRC", 0x00010000),
        ("SDHCI_QUIRK2_DISABLE_HW_TIMEOUT", 0x00020000),
        ("SDHCI_QUIRK2_USE_32BIT_BLK_CNT", 0x00040000),
        ("SDHCI_QUIRK2_ISSUE_CMD_DAT_RESET_TOGETHER", 0x00080000),
    ];
    let actual = [
        ("SDHCI_QUIRK2_HOST_OFF_CARD_ON", SDHCI_QUIRK2_HOST_OFF_CARD_ON),
        ("SDHCI_QUIRK2_HOST_NO_CMD23", SDHCI_QUIRK2_HOST_NO_CMD23),
        ("SDHCI_QUIRK2_NO_1_8_V", SDHCI_QUIRK2_NO_1_8_V),
        ("SDHCI_QUIRK2_PRESET_VALUE_BROKEN", SDHCI_QUIRK2_PRESET_VALUE_BROKEN),
        ("SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON", SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON),
        ("SDHCI_QUIRK2_BROKEN_HOST_CONTROL", SDHCI_QUIRK2_BROKEN_HOST_CONTROL),
        ("SDHCI_QUIRK2_BROKEN_HS200", SDHCI_QUIRK2_BROKEN_HS200),
        ("SDHCI_QUIRK2_BROKEN_DDR50", SDHCI_QUIRK2_BROKEN_DDR50),
        ("SDHCI_QUIRK2_STOP_WITH_TC", SDHCI_QUIRK2_STOP_WITH_TC),
        ("SDHCI_QUIRK2_BROKEN_64_BIT_DMA", SDHCI_QUIRK2_BROKEN_64_BIT_DMA),
        ("SDHCI_QUIRK2_CLEAR_TRANSFERMODE_REG_BEFORE_CMD", SDHCI_QUIRK2_CLEAR_TRANSFERMODE_REG_BEFORE_CMD),
        ("SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400", SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400),
        ("SDHCI_QUIRK2_TUNING_WORK_AROUND", SDHCI_QUIRK2_TUNING_WORK_AROUND),
        ("SDHCI_QUIRK2_SUPPORT_SINGLE", SDHCI_QUIRK2_SUPPORT_SINGLE),
        ("SDHCI_QUIRK2_ACMD23_BROKEN", SDHCI_QUIRK2_ACMD23_BROKEN),
        ("SDHCI_QUIRK2_CLOCK_DIV_ZERO_BROKEN", SDHCI_QUIRK2_CLOCK_DIV_ZERO_BROKEN),
        ("SDHCI_QUIRK2_RSP_136_HAS_CRC", SDHCI_QUIRK2_RSP_136_HAS_CRC),
        ("SDHCI_QUIRK2_DISABLE_HW_TIMEOUT", SDHCI_QUIRK2_DISABLE_HW_TIMEOUT),
        ("SDHCI_QUIRK2_USE_32BIT_BLK_CNT", SDHCI_QUIRK2_USE_32BIT_BLK_CNT),
        ("SDHCI_QUIRK2_ISSUE_CMD_DAT_RESET_TOGETHER", SDHCI_QUIRK2_ISSUE_CMD_DAT_RESET_TOGETHER),
    ];
    assert_eq!(actual.len(), 20, "Linux secondary quirk count changed");
    assert_eq!(actual, expected);
}

#[test]
fn host_flags_request_count_and_busy_response_match_linux() {
    // Linux drivers/mmc/host/sdhci.h:562-570 and 409; include/linux/mmc/core.h:38.
    let expected: [(&str, u32); 7] = [
        ("SDHCI_USE_SDMA", 0x00000001),
        ("SDHCI_USE_ADMA", 0x00000002),
        ("SDHCI_REQ_USE_DMA", 0x00000004),
        ("SDHCI_DEVICE_DEAD", 0x00000008),
        ("SDHCI_AUTO_CMD12", 0x00000040),
        ("SDHCI_AUTO_CMD23", 0x00000080),
        ("SDHCI_USE_64_BIT_DMA", 0x00001000),
    ];
    let actual = [
        ("SDHCI_USE_SDMA", SDHCI_USE_SDMA),
        ("SDHCI_USE_ADMA", SDHCI_USE_ADMA),
        ("SDHCI_REQ_USE_DMA", SDHCI_REQ_USE_DMA),
        ("SDHCI_DEVICE_DEAD", SDHCI_DEVICE_DEAD),
        ("SDHCI_AUTO_CMD12", SDHCI_AUTO_CMD12),
        ("SDHCI_AUTO_CMD23", SDHCI_AUTO_CMD23),
        ("SDHCI_USE_64_BIT_DMA", SDHCI_USE_64_BIT_DMA),
    ];
    assert_eq!(actual.len(), 7, "ported host-flag count changed");
    assert_eq!(actual, expected);
    assert_eq!(SDHCI_MAX_MRQS, 2); // sdhci.h:409
    assert_eq!(MMC_RSP_BUSY, 0x0008); // include/linux/mmc/core.h:38
}
