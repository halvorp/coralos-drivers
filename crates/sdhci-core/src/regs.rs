//! Register and flag definitions from the pinned Linux `drivers/mmc/host/sdhci.h`.

// Standard SDHCI register offsets.
pub const SDHCI_DMA_ADDRESS: u16 = 0x00; // sdhci.h (value from pinned header)
pub const SDHCI_BLOCK_SIZE: u16 = 0x04; // sdhci.h (value from pinned header)
pub const SDHCI_BLOCK_COUNT: u16 = 0x06; // sdhci.h (value from pinned header)
pub const SDHCI_ARGUMENT: u16 = 0x08; // sdhci.h (value from pinned header)
pub const SDHCI_TRANSFER_MODE: u16 = 0x0C; // sdhci.h (value from pinned header)
pub const SDHCI_COMMAND: u16 = 0x0E; // sdhci.h (value from pinned header)
pub const SDHCI_RESPONSE: u16 = 0x10; // sdhci.h (value from pinned header)
pub const SDHCI_BUFFER: u16 = 0x20; // sdhci.h (value from pinned header)
pub const SDHCI_PRESENT_STATE: u16 = 0x24; // sdhci.h (value from pinned header)
pub const SDHCI_HOST_CONTROL: u16 = 0x28; // sdhci.h (value from pinned header)
pub const SDHCI_POWER_CONTROL: u16 = 0x29; // sdhci.h (value from pinned header)
pub const SDHCI_BLOCK_GAP_CONTROL: u16 = 0x2A; // sdhci.h (value from pinned header)
pub const SDHCI_WAKE_UP_CONTROL: u16 = 0x2B; // sdhci.h (value from pinned header)
pub const SDHCI_CLOCK_CONTROL: u16 = 0x2C; // sdhci.h (value from pinned header)
pub const SDHCI_TIMEOUT_CONTROL: u16 = 0x2E; // sdhci.h:156
pub const SDHCI_SOFTWARE_RESET: u16 = 0x2F; // sdhci.h:158
pub const SDHCI_INT_STATUS: u16 = 0x30; // sdhci.h:163
pub const SDHCI_INT_ENABLE: u16 = 0x34; // sdhci.h:164
pub const SDHCI_SIGNAL_ENABLE: u16 = 0x38; // sdhci.h:165
pub const SDHCI_AUTO_CMD_STATUS: u16 = 0x3C; // sdhci.h:217
pub const SDHCI_HOST_CONTROL2: u16 = 0x3E; // sdhci.h:226
pub const SDHCI_CAPABILITIES: u16 = 0x40; // sdhci.h (value from pinned header)
pub const SDHCI_CAPABILITIES_1: u16 = 0x44; // sdhci.h (value from pinned header)
pub const SDHCI_MAX_CURRENT: u16 = 0x48; // sdhci.h (value from pinned header)
pub const SDHCI_ADMA_ERROR: u16 = 0x54; // sdhci.h (value from pinned header)
pub const SDHCI_ADMA_ADDRESS: u16 = 0x58; // sdhci.h (value from pinned header)
pub const SDHCI_ADMA_ADDRESS_HI: u16 = 0x5C; // sdhci.h (value from pinned header)

// Software reset masks.
pub const SDHCI_RESET_ALL: u8 = 0x01; // sdhci.h:159
pub const SDHCI_RESET_CMD: u8 = 0x02; // sdhci.h:160
pub const SDHCI_RESET_DATA: u8 = 0x04; // sdhci.h:161

// Normal and error interrupt bits.
pub const SDHCI_INT_RESPONSE: u32 = 0x00000001; // sdhci.h:166
pub const SDHCI_INT_DATA_END: u32 = 0x00000002; // sdhci.h:167
pub const SDHCI_INT_BLK_GAP: u32 = 0x00000004; // sdhci.h:168
pub const SDHCI_INT_DMA_END: u32 = 0x00000008; // sdhci.h:169
pub const SDHCI_INT_SPACE_AVAIL: u32 = 0x00000010; // sdhci.h:170
pub const SDHCI_INT_DATA_AVAIL: u32 = 0x00000020; // sdhci.h:171
pub const SDHCI_INT_CARD_INSERT: u32 = 0x00000040; // sdhci.h:172
pub const SDHCI_INT_CARD_REMOVE: u32 = 0x00000080; // sdhci.h:173
pub const SDHCI_INT_CARD_INT: u32 = 0x00000100; // sdhci.h:174
pub const SDHCI_INT_RETUNE: u32 = 0x00001000; // sdhci.h:175
pub const SDHCI_INT_FX_EVENT: u32 = 0x00002000; // sdhci.h:178
pub const SDHCI_INT_CQE: u32 = 0x00004000; // sdhci.h:180
pub const SDHCI_INT_ERROR: u32 = 0x00008000; // sdhci.h:181
pub const SDHCI_INT_TIMEOUT: u32 = 0x00010000; // sdhci.h:182
pub const SDHCI_INT_CRC: u32 = 0x00020000; // sdhci.h:183
pub const SDHCI_INT_END_BIT: u32 = 0x00040000; // sdhci.h:184
pub const SDHCI_INT_INDEX: u32 = 0x00080000; // sdhci.h:185
pub const SDHCI_INT_DATA_TIMEOUT: u32 = 0x00100000; // sdhci.h:186
pub const SDHCI_INT_DATA_CRC: u32 = 0x00200000; // sdhci.h:187
pub const SDHCI_INT_DATA_END_BIT: u32 = 0x00400000; // sdhci.h:188
pub const SDHCI_INT_BUS_POWER: u32 = 0x00800000; // sdhci.h:189
pub const SDHCI_INT_AUTO_CMD_ERR: u32 = 0x01000000; // sdhci.h:190
pub const SDHCI_INT_ADMA_ERROR: u32 = 0x02000000; // sdhci.h:191
pub const SDHCI_INT_TUNING_ERROR: u32 = 0x04000000; // sdhci.h:192
pub const SDHCI_INT_RESP_ERR: u32 = 0x08000000; // sdhci.h:195

// Interrupt masks.
pub const SDHCI_INT_NORMAL_MASK: u32 = 0x00007FFF; // sdhci.h:197
pub const SDHCI_INT_ERROR_MASK: u32 = 0xFFFF8000; // sdhci.h:198
pub const SDHCI_INT_CMD_MASK: u32 = SDHCI_INT_RESPONSE // sdhci.h:200-202
    | SDHCI_INT_TIMEOUT
    | SDHCI_INT_CRC
    | SDHCI_INT_END_BIT
    | SDHCI_INT_INDEX
    | SDHCI_INT_AUTO_CMD_ERR;
pub const SDHCI_INT_DATA_MASK: u32 = SDHCI_INT_DATA_END // sdhci.h:203-207
    | SDHCI_INT_DMA_END
    | SDHCI_INT_DATA_AVAIL
    | SDHCI_INT_SPACE_AVAIL
    | SDHCI_INT_DATA_TIMEOUT
    | SDHCI_INT_DATA_CRC
    | SDHCI_INT_DATA_END_BIT
    | SDHCI_INT_ADMA_ERROR
    | SDHCI_INT_BLK_GAP
    | SDHCI_INT_TUNING_ERROR;
pub const SDHCI_INT_ALL_MASK: u32 = u32::MAX; // sdhci.h:208

// CQE interrupt masks.
pub const SDHCI_CQE_INT_ERR_MASK: u32 = SDHCI_INT_ADMA_ERROR // sdhci.h:210-213
    | SDHCI_INT_BUS_POWER
    | SDHCI_INT_DATA_END_BIT
    | SDHCI_INT_DATA_CRC
    | SDHCI_INT_DATA_TIMEOUT
    | SDHCI_INT_INDEX
    | SDHCI_INT_END_BIT
    | SDHCI_INT_CRC
    | SDHCI_INT_TIMEOUT;
pub const SDHCI_CQE_INT_MASK: u32 = SDHCI_CQE_INT_ERR_MASK | SDHCI_INT_CQE; // sdhci.h:215

// Present-state inhibit and status bits.
pub const SDHCI_CMD_INHIBIT: u32 = 0x00000001; // sdhci.h (value from pinned header)
pub const SDHCI_DATA_INHIBIT: u32 = 0x00000002; // sdhci.h (value from pinned header)
pub const SDHCI_DAT_ACTIVE: u32 = 0x00000004; // sdhci.h (value from pinned header)
pub const SDHCI_DOING_WRITE: u32 = 0x00000100; // sdhci.h (value from pinned header)
pub const SDHCI_DOING_READ: u32 = 0x00000200; // sdhci.h (value from pinned header)
pub const SDHCI_SPACE_AVAILABLE: u32 = 0x00000400; // sdhci.h (value from pinned header)
pub const SDHCI_DATA_AVAILABLE: u32 = 0x00000800; // sdhci.h (value from pinned header)
pub const SDHCI_CARD_PRESENT: u32 = 0x00010000; // sdhci.h (value from pinned header)
pub const SDHCI_CD_STABLE: u32 = 0x00020000; // sdhci.h (value from pinned header)
pub const SDHCI_WRITE_PROTECT: u32 = 0x00080000; // sdhci.h (value from pinned header)

// Auto-CMD status bits.
pub const SDHCI_AUTO_CMD_TIMEOUT: u16 = 0x0002; // sdhci.h:218
pub const SDHCI_AUTO_CMD_CRC: u16 = 0x0004; // sdhci.h:219
pub const SDHCI_AUTO_CMD_END_BIT: u16 = 0x0008; // sdhci.h:220
pub const SDHCI_AUTO_CMD_INDEX: u16 = 0x0010; // sdhci.h:221
pub const SDHCI_AUTO_CMD_RESP_ERR: u16 = 0x0020; // sdhci.h:224

// Host Control 2 bits.
pub const SDHCI_CTRL_UHS_MASK: u16 = 0x0007; // sdhci.h:227
pub const SDHCI_CTRL_UHS_SDR12: u16 = 0x0000; // sdhci.h:228
pub const SDHCI_CTRL_UHS_SDR25: u16 = 0x0001; // sdhci.h:229
pub const SDHCI_CTRL_UHS_SDR50: u16 = 0x0002; // sdhci.h:230
pub const SDHCI_CTRL_EXEC_TUNING: u16 = 0x0040; // sdhci.h (value from pinned header)
pub const SDHCI_CTRL_TUNED_CLK: u16 = 0x0080; // sdhci.h (value from pinned header)

// Transfer Mode register fields.
pub const SDHCI_TRNS_DMA: u16 = 0x0001; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_BLK_CNT_EN: u16 = 0x0002; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_AUTO_CMD12: u16 = 0x0004; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_AUTO_CMD23: u16 = 0x0008; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_READ: u16 = 0x0010; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_MULTI: u16 = 0x0020; // sdhci.h (value from pinned header)
pub const SDHCI_TRNS_AUTO_SEL: u16 = 0x000C; // sdhci.h:42

// Command register fields and helpers.
pub const SDHCI_CMD_RESP_MASK: u16 = 0x0003; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_RESP_NONE: u16 = 0x0000; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_RESP_LONG: u16 = 0x0001; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_RESP_SHORT: u16 = 0x0002; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_RESP_SHORT_BUSY: u16 = 0x0003; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_CRC: u16 = 0x0008; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_INDEX: u16 = 0x0010; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_DATA: u16 = 0x0020; // sdhci.h (value from pinned header)
pub const SDHCI_CMD_ABORTCMD: u16 = 0x00C0; // sdhci.h (value from pinned header)

/// Build a COMMAND register value from an opcode and flags.
pub const fn SDHCI_MAKE_CMD(opcode: u16, flags: u16) -> u16 {
    ((opcode & 0x3f) << 8) | (flags & 0xff)
}

/// Extract the opcode from a COMMAND register value.
pub const fn SDHCI_GET_CMD(v: u16) -> u16 {
    v >> 8
}

// SDHCI_QUIRK values.
pub const SDHCI_QUIRK_CLOCK_BEFORE_RESET: u32 = 1 << 0; // sdhci.h:435
pub const SDHCI_QUIRK_FORCE_DMA: u32 = 1 << 1; // sdhci.h:437
pub const SDHCI_QUIRK_NO_CARD_NO_RESET: u32 = 1 << 2; // sdhci.h:439
pub const SDHCI_QUIRK_SINGLE_POWER_WRITE: u32 = 1 << 3; // sdhci.h:441
pub const SDHCI_QUIRK_BROKEN_DMA: u32 = 1 << 5; // sdhci.h:443
pub const SDHCI_QUIRK_BROKEN_ADMA: u32 = 1 << 6; // sdhci.h:445
pub const SDHCI_QUIRK_32BIT_DMA_ADDR: u32 = 1 << 7; // sdhci.h:447
pub const SDHCI_QUIRK_32BIT_DMA_SIZE: u32 = 1 << 8; // sdhci.h:449
pub const SDHCI_QUIRK_32BIT_ADMA_SIZE: u32 = 1 << 9; // sdhci.h:451
pub const SDHCI_QUIRK_RESET_AFTER_REQUEST: u32 = 1 << 10; // sdhci.h:453
pub const SDHCI_QUIRK_NO_SIMULT_VDD_AND_POWER: u32 = 1 << 11; // sdhci.h:455
pub const SDHCI_QUIRK_BROKEN_TIMEOUT_VAL: u32 = 1 << 12; // sdhci.h:457
pub const SDHCI_QUIRK_BROKEN_SMALL_PIO: u32 = 1 << 13; // sdhci.h:459
pub const SDHCI_QUIRK_NO_BUSY_IRQ: u32 = 1 << 14; // sdhci.h:461
pub const SDHCI_QUIRK_BROKEN_CARD_DETECTION: u32 = 1 << 15; // sdhci.h:463
pub const SDHCI_QUIRK_INVERTED_WRITE_PROTECT: u32 = 1 << 16; // sdhci.h:465
pub const SDHCI_QUIRK_BROKEN_CQE: u32 = 1 << 17; // sdhci.h:467
pub const SDHCI_QUIRK_PIO_NEEDS_DELAY: u32 = 1 << 18; // sdhci.h:469
pub const SDHCI_QUIRK_NO_LED: u32 = 1 << 19; // sdhci.h:471
pub const SDHCI_QUIRK_FORCE_BLK_SZ_2048: u32 = 1 << 20; // sdhci.h:473
pub const SDHCI_QUIRK_NO_MULTIBLOCK: u32 = 1 << 21; // sdhci.h:475
pub const SDHCI_QUIRK_FORCE_1_BIT_DATA: u32 = 1 << 22; // sdhci.h:477
pub const SDHCI_QUIRK_DELAY_AFTER_POWER: u32 = 1 << 23; // sdhci.h:479
pub const SDHCI_QUIRK_DATA_TIMEOUT_USES_SDCLK: u32 = 1 << 24; // sdhci.h:481
pub const SDHCI_QUIRK_CAP_CLOCK_BASE_BROKEN: u32 = 1 << 25; // sdhci.h:483
pub const SDHCI_QUIRK_NO_ENDATTR_IN_NOPDESC: u32 = 1 << 26; // sdhci.h:485
pub const SDHCI_QUIRK_MULTIBLOCK_READ_ACMD12: u32 = 1 << 28; // sdhci.h:487
pub const SDHCI_QUIRK_NO_HISPD_BIT: u32 = 1 << 29; // sdhci.h:489
pub const SDHCI_QUIRK_BROKEN_ADMA_ZEROLEN_DESC: u32 = 1 << 30; // sdhci.h:491

// SDHCI_QUIRK2 values.
pub const SDHCI_QUIRK2_HOST_OFF_CARD_ON: u32 = 1 << 0; // sdhci.h:495
pub const SDHCI_QUIRK2_HOST_NO_CMD23: u32 = 1 << 1; // sdhci.h:496
pub const SDHCI_QUIRK2_NO_1_8_V: u32 = 1 << 2; // sdhci.h:498
pub const SDHCI_QUIRK2_PRESET_VALUE_BROKEN: u32 = 1 << 3; // sdhci.h:499
pub const SDHCI_QUIRK2_CARD_ON_NEEDS_BUS_ON: u32 = 1 << 4; // sdhci.h:500
pub const SDHCI_QUIRK2_BROKEN_HOST_CONTROL: u32 = 1 << 5; // sdhci.h:502
pub const SDHCI_QUIRK2_BROKEN_HS200: u32 = 1 << 6; // sdhci.h:504
pub const SDHCI_QUIRK2_BROKEN_DDR50: u32 = 1 << 7; // sdhci.h:506
pub const SDHCI_QUIRK2_STOP_WITH_TC: u32 = 1 << 8; // sdhci.h:508
pub const SDHCI_QUIRK2_BROKEN_64_BIT_DMA: u32 = 1 << 9; // sdhci.h:510
pub const SDHCI_QUIRK2_CLEAR_TRANSFERMODE_REG_BEFORE_CMD: u32 = 1 << 10; // sdhci.h:512
pub const SDHCI_QUIRK2_CAPS_BIT63_FOR_HS400: u32 = 1 << 11; // sdhci.h:514
pub const SDHCI_QUIRK2_TUNING_WORK_AROUND: u32 = 1 << 12; // sdhci.h:516
pub const SDHCI_QUIRK2_SUPPORT_SINGLE: u32 = 1 << 13; // sdhci.h:518
pub const SDHCI_QUIRK2_ACMD23_BROKEN: u32 = 1 << 14; // sdhci.h:520
pub const SDHCI_QUIRK2_CLOCK_DIV_ZERO_BROKEN: u32 = 1 << 15; // sdhci.h:522
pub const SDHCI_QUIRK2_RSP_136_HAS_CRC: u32 = 1 << 16; // sdhci.h:524
pub const SDHCI_QUIRK2_DISABLE_HW_TIMEOUT: u32 = 1 << 17; // sdhci.h:529
pub const SDHCI_QUIRK2_USE_32BIT_BLK_CNT: u32 = 1 << 18; // sdhci.h:536
pub const SDHCI_QUIRK2_ISSUE_CMD_DAT_RESET_TOGETHER: u32 = 1 << 19; // sdhci.h:538

// Host flag bits used by this port.
pub const SDHCI_USE_SDMA: u32 = 1 << 0; // sdhci.h:562
pub const SDHCI_USE_ADMA: u32 = 1 << 1; // sdhci.h:563
pub const SDHCI_REQ_USE_DMA: u32 = 1 << 2; // sdhci.h:564
pub const SDHCI_DEVICE_DEAD: u32 = 1 << 3; // sdhci.h:565
pub const SDHCI_AUTO_CMD12: u32 = 1 << 6; // sdhci.h (value from pinned header)
pub const SDHCI_AUTO_CMD23: u32 = 1 << 7; // sdhci.h (value from pinned header)
pub const SDHCI_USE_64_BIT_DMA: u32 = 1 << 12; // sdhci.h (value from pinned header)

// Maximum number of completed requests tracked by the host.
pub const SDHCI_MAX_MRQS: usize = 2; // sdhci.h (value from pinned header)

// MMC response flag for busy signalling. Ported from linux/mmc/core.h (MMC_RSP_BUSY), not sdhci.h.
// It lived as a PRIVATE `const` in BOTH core.rs and executor.rs — two copies of one fact, which is
// the single-source-of-truth violation the doctrine names, and it also made the value unreachable
// from the test vector that must assert STOP_WITH_TC sets it on the emitted CMD12. One home, public.
pub const MMC_RSP_BUSY: u16 = 1 << 3; // include/linux/mmc/core.h:38
