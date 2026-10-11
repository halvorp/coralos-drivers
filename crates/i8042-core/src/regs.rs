// SPDX-License-Identifier: GPL-2.0-only
//! i8042 register offsets, status and control bits, commands and timeouts.
//!
//! Ported from Linux `include/linux/i8042.h`, `drivers/input/serio/i8042.h` and
//! `drivers/input/serio/i8042-io.h`. Copyright (c) 1999-2004 Vojtech Pavlik.

/// Register OFFSETS from the I/O base: the legacy base is 0x60 (i8042-io.h:36-38).
pub const DATA_REG: u16 = 0; // i8042-io.h:38 (0x60 - 0x60)
pub const STATUS_REG: u16 = 4; // i8042-io.h:37 (0x64 - 0x60)
pub const COMMAND_REG: u16 = 4; // i8042-io.h:36 (0x64 - 0x60)

// Status register bits.
pub const STR_PARITY: u8 = 0x80; // i8042.h:35
pub const STR_TIMEOUT: u8 = 0x40; // i8042.h:36
pub const STR_AUXDATA: u8 = 0x20; // i8042.h:37
pub const STR_KEYLOCK: u8 = 0x10; // i8042.h:38
pub const STR_CMDDAT: u8 = 0x08; // i8042.h:39
pub const STR_MUXERR: u8 = 0x04; // i8042.h:40
pub const STR_IBF: u8 = 0x02; // i8042.h:41
pub const STR_OBF: u8 = 0x01; // i8042.h:42

// Control register (CTR) bits.
pub const CTR_KBDINT: u8 = 0x01; // i8042.h:48
pub const CTR_AUXINT: u8 = 0x02; // i8042.h:49
pub const CTR_IGNKEYLOCK: u8 = 0x08; // i8042.h:50
pub const CTR_KBDDIS: u8 = 0x10; // i8042.h:51
pub const CTR_AUXDIS: u8 = 0x20; // i8042.h:52
pub const CTR_XLATE: u8 = 0x40; // i8042.h:53

// Controller commands: low byte = command, bits 8..11 = reply bytes, bits 12..15 = parameter bytes.
pub const CMD_CTL_RCTR: u16 = 0x0120; // i8042.h:13
pub const CMD_CTL_WCTR: u16 = 0x1060; // i8042.h:14
pub const CMD_CTL_TEST: u16 = 0x01aa; // i8042.h:15
pub const CMD_KBD_DISABLE: u16 = 0x00ad; // i8042.h:17
pub const CMD_KBD_ENABLE: u16 = 0x00ae; // i8042.h:18
pub const CMD_KBD_TEST: u16 = 0x01ab; // i8042.h:19
pub const CMD_AUX_DISABLE: u16 = 0x00a7; // i8042.h:22
pub const CMD_AUX_ENABLE: u16 = 0x00a8; // i8042.h:23
pub const CMD_AUX_TEST: u16 = 0x01a9; // i8042.h:24

/// The self-test's passing answer.
pub const RET_CTL_TEST: u8 = 0x55; // i8042.h:40

/// 50 µs units the controller is given to react — long enough for it to time out on a byte to an
/// absent mouse.
pub const CTL_TIMEOUT: u32 = 10000; // i8042.h:34
/// The unit of [`CTL_TIMEOUT`] (`udelay(50)` in i8042_wait_read/write).
pub const CTL_POLL_US: u32 = 50; // i8042.c:235
/// The controller's expected largest internal buffer; a flush past it means no controller.
pub const BUFFER_SIZE: usize = 16; // i8042.h:47
/// Self-test tries: `while (i++ < 5)` runs the body six times.
pub const SELFTEST_RETRIES: u32 = 5; // i8042.c:963
/// The pause between self-test tries.
pub const SELFTEST_RETRY_MS: u32 = 50; // i8042.c:962
/// CTR reads before two must agree.
pub const CTR_READS_MAX: u32 = 10; // i8042.c:996
/// The pause between CTR reads.
pub const CTR_READ_RETRY_US: u32 = 50; // i8042.c:1002

/// An interface test's passing answers: 0x00, and 0xfa "on some notebooks which ignore the spec".
/// 0x01-0x03 mean clock/data stuck, 0xff a general error.
pub const PORT_TEST_OK: u8 = 0x00; // i8042.c:829
pub const PORT_TEST_OK_NOTEBOOK: u8 = 0xfa; // i8042.c:830
pub const PORT_TEST_GENERAL_ERROR: u8 = 0xff; // i8042.c:829
