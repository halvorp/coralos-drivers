// SPDX-License-Identifier: GPL-2.0-only
//! Every constant pinned by VALUE against its Linux literal (PORT_CONVENTIONS §3a.6: independently of any
//! test that uses it to build input).
//! Copyright (c) 1999-2004 Vojtech Pavlik; Copyright (c) 2004 Dmitry Torokhov.

use i8042_core::{atkbd, ps2, regs::*};

#[test]
fn register_offsets_are_the_legacy_ports_minus_the_base() {
    assert_eq!((DATA_REG, STATUS_REG, COMMAND_REG), (0x60 - 0x60, 0x64 - 0x60, 0x64 - 0x60)); // i8042-io.h:36-38
}

#[test]
fn status_bits() {
    assert_eq!(
        [STR_PARITY, STR_TIMEOUT, STR_AUXDATA, STR_KEYLOCK, STR_CMDDAT, STR_MUXERR, STR_IBF, STR_OBF],
        [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01]
    ); // i8042.h:35-42
}

#[test]
fn ctr_bits() {
    assert_eq!(
        [CTR_KBDINT, CTR_AUXINT, CTR_IGNKEYLOCK, CTR_KBDDIS, CTR_AUXDIS, CTR_XLATE],
        [0x01, 0x02, 0x08, 0x10, 0x20, 0x40]
    ); // i8042.h:48-53
}

#[test]
fn controller_commands() {
    assert_eq!(
        [CMD_CTL_RCTR, CMD_CTL_WCTR, CMD_CTL_TEST, CMD_KBD_DISABLE, CMD_KBD_ENABLE, CMD_KBD_TEST, CMD_AUX_DISABLE,
         CMD_AUX_ENABLE, CMD_AUX_TEST],
        [0x0120, 0x1060, 0x01aa, 0x00ad, 0x00ae, 0x01ab, 0x00a7, 0x00a8, 0x01a9]
    ); // i8042.h:13-24
}

#[test]
fn controller_limits_and_answers() {
    assert_eq!(RET_CTL_TEST, 0x55); // i8042.h:40
    assert_eq!(CTL_TIMEOUT, 10000); // i8042.h:34
    assert_eq!(CTL_POLL_US, 50); // i8042.c:235
    assert_eq!(BUFFER_SIZE, 16); // i8042.h:47
    assert_eq!(SELFTEST_RETRIES, 5); // i8042.c:963
    assert_eq!(SELFTEST_RETRY_MS, 50); // i8042.c:962
    assert_eq!(CTR_READS_MAX, 10); // i8042.c:996
    assert_eq!(CTR_READ_RETRY_US, 50); // i8042.c:1002
    assert_eq!((PORT_TEST_OK, PORT_TEST_OK_NOTEBOOK, PORT_TEST_GENERAL_ERROR), (0x00, 0xfa, 0xff)); // i8042.c:829-830
}

#[test]
fn libps2_literals() {
    assert_eq!(
        [ps2::CMD_EX_SETLEDS, ps2::CMD_SETLEDS, ps2::CMD_GETID, ps2::CMD_SETREP, ps2::CMD_RESET_BAT],
        [0x20eb, 0x10ed, 0x02f2, 0x10f3, 0x02ff]
    ); // libps2.c:25-29
    assert_eq!(
        [ps2::RET_BAT, ps2::RET_ID, ps2::RET_ACK, ps2::RET_NAK, ps2::RET_ERR],
        [0xaa, 0x00, 0xfa, 0xfe, 0xfc]
    ); // libps2.c:31-35
    assert_eq!(
        [ps2::FLAG_ACK, ps2::FLAG_CMD, ps2::FLAG_CMD1, ps2::FLAG_WAITID, ps2::FLAG_NAK, ps2::FLAG_PASS_NOACK],
        [1, 2, 4, 8, 16, 32]
    ); // libps2.c:37-42
    assert_eq!((ps2::ACK_TIMEOUT_MS, ps2::RESET_ACK_TIMEOUT_MS), (200, 1000)); // libps2.c:326
    assert_eq!(ps2::PARAM_ACK_TIMEOUT_MS, 200); // libps2.c:334
    assert_eq!(ps2::SEND_ATTEMPTS, 2); // libps2.c:328
    assert_eq!((ps2::REPLY_TIMEOUT_MS, ps2::RESET_REPLY_TIMEOUT_MS), (500, 4000)); // libps2.c:344
    assert_eq!(ps2::RESET_TAIL_TIMEOUT_MS, 100); // libps2.c:214-215
    assert_eq!(ps2::GETID_MOUSE_LIKE, 0xaa); // libps2.c:225
    assert_eq!(ps2::CMDBUF_LEN, 8); // libps2.h:53
    assert_eq!(ps2::KEYBOARD_IDS, [0xab, 0xac, 0x2b, 0x5d, 0x60, 0x47]); // libps2.c:185-190
    assert_eq!(ps2::polls(1), 20);
}

#[test]
fn atkbd_command_literals() {
    assert_eq!(
        [atkbd::CMD_SETLEDS, atkbd::CMD_GSCANSET, atkbd::CMD_SSCANSET, atkbd::CMD_GETID, atkbd::CMD_SETREP,
         atkbd::CMD_ENABLE, atkbd::CMD_RESET_DIS, atkbd::CMD_RESET_BAT, atkbd::CMD_EX_ENABLE],
        [0x10ed, 0x11f0, 0x10f0, 0x02f2, 0x10f3, 0x00f4, 0x00f5, 0x02ff, 0x10ea]
    ); // atkbd.c:136-148
    assert_eq!(atkbd::ID_INVALID, 0xa5); // atkbd.c:837
    assert_eq!(atkbd::ID_LEDS_ONLY, 0xabba); // atkbd.c:848
    assert_eq!(atkbd::ID_NCD, 0xaca1); // atkbd.c:857
    assert_eq!(atkbd::EX_ENABLE_PARAM, 0x71); // atkbd.c:903
    assert_eq!((atkbd::LED_SCROLL, atkbd::LED_NUM, atkbd::LED_CAPS), (1, 2, 4)); // atkbd.c:612-614
    assert_eq!((atkbd::INPUT_DEFAULT_DELAY_MS, atkbd::INPUT_DEFAULT_PERIOD_MS), (250, 33)); // input.c:2352
    assert_eq!(atkbd::REPEAT_DELAYS_MS, [250, 500, 750, 1000]); // atkbd.c:588-589
    assert_eq!(
        atkbd::REPEAT_PERIODS_MS,
        [33, 37, 42, 46, 50, 54, 58, 63, 67, 75, 83, 92, 100, 109, 116, 125,
         133, 149, 167, 182, 200, 217, 232, 250, 270, 303, 333, 370, 400, 435, 470, 500]
    ); // atkbd.c:585-587
}
