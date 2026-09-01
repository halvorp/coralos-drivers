// SPDX-License-Identifier: GPL-2.0-only
//! R1/R2/R3 response formats and native/SPI status bits.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c` and the definitions it uses
//! in `include/linux/mmc/core.h` and `include/linux/mmc/mmc.h`.
//! Copyright 2006-2007 Pierre Ossman and the Linux MMC authors.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseFormat { pub name: &'static str, pub flags: u32 }

pub const RSP_PRESENT: u32 = 1 << 0; // include/linux/mmc/core.h:35
pub const RSP_136: u32 = 1 << 1; // include/linux/mmc/core.h:36
pub const RSP_CRC: u32 = 1 << 2; // include/linux/mmc/core.h:37
pub const RSP_BUSY: u32 = 1 << 3; // include/linux/mmc/core.h:38
pub const RSP_OPCODE: u32 = 1 << 4; // include/linux/mmc/core.h:39
pub const R1_FLAGS: u32 = RSP_PRESENT | RSP_CRC | RSP_OPCODE; // include/linux/mmc/core.h:58
pub const R2_FLAGS: u32 = RSP_PRESENT | RSP_136 | RSP_CRC; // include/linux/mmc/core.h:61
pub const R3_FLAGS: u32 = RSP_PRESENT; // include/linux/mmc/core.h:62
pub const RESPONSE_FORMATS: [ResponseFormat; 3] = [
    ResponseFormat { name: "R1", flags: R1_FLAGS }, // mmc_ops.c:77
    ResponseFormat { name: "R2", flags: R2_FLAGS }, // mmc_ops.c:276
    ResponseFormat { name: "R3", flags: R3_FLAGS }, // mmc_ops.c:243
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseBit { pub name: &'static str, pub bit: u8, pub mask: u32 }

/// Every named bit in Linux's native R1 STATUS word, including error and state-dependent status
/// bits. Keeping the complete corpus prevents a newly observed refusal from decoding as unknown.
pub const R1_STATUS_BITS: [ResponseBit; 23] = [
    ResponseBit { name: "OUT_OF_RANGE", bit: 31, mask: 0x8000_0000 }, // include/linux/mmc/mmc.h:134
    ResponseBit { name: "ADDRESS_ERROR", bit: 30, mask: 0x4000_0000 }, // include/linux/mmc/mmc.h:135
    ResponseBit { name: "BLOCK_LEN_ERROR", bit: 29, mask: 0x2000_0000 }, // include/linux/mmc/mmc.h:136
    ResponseBit { name: "ERASE_SEQ_ERROR", bit: 28, mask: 0x1000_0000 }, // include/linux/mmc/mmc.h:137
    ResponseBit { name: "ERASE_PARAM", bit: 27, mask: 0x0800_0000 }, // include/linux/mmc/mmc.h:138
    ResponseBit { name: "WP_VIOLATION", bit: 26, mask: 0x0400_0000 }, // include/linux/mmc/mmc.h:139
    ResponseBit { name: "CARD_IS_LOCKED", bit: 25, mask: 0x0200_0000 }, // include/linux/mmc/mmc.h:140
    ResponseBit { name: "LOCK_UNLOCK_FAILED", bit: 24, mask: 0x0100_0000 }, // include/linux/mmc/mmc.h:141
    ResponseBit { name: "COM_CRC_ERROR", bit: 23, mask: 0x0080_0000 }, // include/linux/mmc/mmc.h:142
    ResponseBit { name: "ILLEGAL_COMMAND", bit: 22, mask: 0x0040_0000 }, // include/linux/mmc/mmc.h:143
    ResponseBit { name: "CARD_ECC_FAILED", bit: 21, mask: 0x0020_0000 }, // include/linux/mmc/mmc.h:144
    ResponseBit { name: "CC_ERROR", bit: 20, mask: 0x0010_0000 }, // include/linux/mmc/mmc.h:145
    ResponseBit { name: "ERROR", bit: 19, mask: 0x0008_0000 }, // include/linux/mmc/mmc.h:146
    ResponseBit { name: "UNDERRUN", bit: 18, mask: 0x0004_0000 }, // include/linux/mmc/mmc.h:147
    ResponseBit { name: "OVERRUN", bit: 17, mask: 0x0002_0000 }, // include/linux/mmc/mmc.h:148
    ResponseBit { name: "CID_CSD_OVERWRITE", bit: 16, mask: 0x0001_0000 }, // include/linux/mmc/mmc.h:149
    ResponseBit { name: "WP_ERASE_SKIP", bit: 15, mask: 0x0000_8000 }, // include/linux/mmc/mmc.h:150
    ResponseBit { name: "CARD_ECC_DISABLED", bit: 14, mask: 0x0000_4000 }, // include/linux/mmc/mmc.h:151
    ResponseBit { name: "ERASE_RESET", bit: 13, mask: 0x0000_2000 }, // include/linux/mmc/mmc.h:152
    ResponseBit { name: "READY_FOR_DATA", bit: 8, mask: 0x0000_0100 }, // include/linux/mmc/mmc.h:155
    ResponseBit { name: "SWITCH_ERROR", bit: 7, mask: 0x0000_0080 }, // include/linux/mmc/mmc.h:156
    ResponseBit { name: "EXCEPTION_EVENT", bit: 6, mask: 0x0000_0040 }, // include/linux/mmc/mmc.h:157
    ResponseBit { name: "APP_CMD", bit: 5, mask: 0x0000_0020 }, // include/linux/mmc/mmc.h:158
];
/// Bits whose Linux type annotation contains `e` (error), rather than `s` alone (status).
pub const R1_ERROR_BITS: [ResponseBit; 15] = [
    R1_STATUS_BITS[0], R1_STATUS_BITS[1], R1_STATUS_BITS[2], R1_STATUS_BITS[3],
    R1_STATUS_BITS[4], R1_STATUS_BITS[5], R1_STATUS_BITS[7], R1_STATUS_BITS[8],
    R1_STATUS_BITS[9], R1_STATUS_BITS[10], R1_STATUS_BITS[11], R1_STATUS_BITS[12],
    R1_STATUS_BITS[13], R1_STATUS_BITS[14], R1_STATUS_BITS[15],
];
pub const R1_STATUS_MASK: u32 = 0xfff9_a000; // include/linux/mmc/mmc.h:153

pub const SPI_R1_ERROR_BITS: [ResponseBit; 6] = [
    ResponseBit { name: "ERASE_RESET", bit: 1, mask: 0x0002 }, // include/linux/mmc/mmc.h:185
    ResponseBit { name: "ILLEGAL_COMMAND", bit: 2, mask: 0x0004 }, // include/linux/mmc/mmc.h:186
    ResponseBit { name: "COM_CRC", bit: 3, mask: 0x0008 }, // include/linux/mmc/mmc.h:187
    ResponseBit { name: "ERASE_SEQ", bit: 4, mask: 0x0010 }, // include/linux/mmc/mmc.h:188
    ResponseBit { name: "ADDRESS", bit: 5, mask: 0x0020 }, // include/linux/mmc/mmc.h:189
    ResponseBit { name: "PARAMETER", bit: 6, mask: 0x0040 }, // include/linux/mmc/mmc.h:190
];
pub const SPI_R2_ERROR_BITS: [ResponseBit; 8] = [
    ResponseBit { name: "CARD_LOCKED", bit: 8, mask: 0x0100 }, // include/linux/mmc/mmc.h:192
    ResponseBit { name: "WP_ERASE_SKIP_OR_LOCK_UNLOCK_FAIL", bit: 9, mask: 0x0200 }, // include/linux/mmc/mmc.h:193-194
    ResponseBit { name: "ERROR", bit: 10, mask: 0x0400 }, // include/linux/mmc/mmc.h:195
    ResponseBit { name: "CC_ERROR", bit: 11, mask: 0x0800 }, // include/linux/mmc/mmc.h:196
    ResponseBit { name: "CARD_ECC_ERROR", bit: 12, mask: 0x1000 }, // include/linux/mmc/mmc.h:197
    ResponseBit { name: "WP_VIOLATION", bit: 13, mask: 0x2000 }, // include/linux/mmc/mmc.h:198
    ResponseBit { name: "ERASE_PARAM", bit: 14, mask: 0x4000 }, // include/linux/mmc/mmc.h:199
    ResponseBit { name: "OUT_OF_RANGE_OR_CSD_OVERWRITE", bit: 15, mask: 0x8000 }, // include/linux/mmc/mmc.h:200-201
];
pub const R3_CARD_BUSY: u32 = 0x8000_0000; // include/linux/mmc/mmc.h:206

pub fn r1_errors(status: u32) -> impl Iterator<Item = &'static ResponseBit> {
    R1_ERROR_BITS.iter().filter(move |bit| status & bit.mask != 0)
}
pub fn spi_r1_errors(status: u16) -> impl Iterator<Item = &'static ResponseBit> {
    SPI_R1_ERROR_BITS.iter().filter(move |bit| u32::from(status) & bit.mask != 0)
}
pub fn spi_r2_errors(status: u16) -> impl Iterator<Item = &'static ResponseBit> {
    SPI_R2_ERROR_BITS.iter().filter(move |bit| u32::from(status) & bit.mask != 0)
}
pub fn r1_status_errors(status: u32) -> u32 { status & R1_STATUS_MASK }
pub fn r3_power_up_complete(ocr: u32) -> bool { ocr & R3_CARD_BUSY != 0 }
