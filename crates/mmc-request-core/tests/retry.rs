// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for retry decisions from `drivers/mmc/core/core.c`,
//! `core.h`, and `sd_ops.c`. Copyright (C) 2003-2004 Russell King; 2005-2008
//! Pierre Ossman; 2006 Philip Langdale.

use mmc_request_core::retry::*;

#[test]
fn retry_bounds_are_pinned_as_linux_literals() {
    assert_eq!(MMC_CMD_RETRIES, 3); // drivers/mmc/core/core.h:18
    assert_eq!(APP_CMD_MAX_ATTEMPTS, 4); // drivers/mmc/core/sd_ops.c:86
    assert_eq!(R1_SPI_ILLEGAL_COMMAND, 0x0000_0004); // include/linux/mmc/mmc.h:186
}

#[test]
fn request_errors_are_pinned_by_count_name_and_value() {
    let got: Vec<(&str, i32)> = REQUEST_ERRORS.iter().map(|x| (x.name, x.errno)).collect();
    assert_eq!(got.len(), 4);
    assert_eq!(got, [
        ("ETIMEDOUT", -110), // drivers/mmc/core/core.c:87; include/uapi/asm-generic/errno.h:94
        ("EILSEQ", -84), // drivers/mmc/core/core.c:88; include/uapi/asm-generic/errno.h:68
        ("EIO", -5), // drivers/mmc/core/core.c:89; include/uapi/asm-generic/errno-base.h:9
        ("ENOMEDIUM", -123), // drivers/mmc/core/core.c:346; include/uapi/asm-generic/errno.h:109
    ]);
}

#[test]
fn ordinary_request_action_maps_complete_retry_abort_and_fail() {
    assert_eq!(request_action(0, 3, false, false, 0), RetryAction::Complete);
    assert_eq!(request_action(-84, 3, false, false, 0), RetryAction::Retry { retries_remaining: 2 }); // core.c:411-420
    assert_eq!(request_action(-110, 1, false, false, 0), RetryAction::Retry { retries_remaining: 0 });
    assert_eq!(request_action(-5, 3, false, true, 0x0000_0004), RetryAction::AbortIllegalSpiCommand); // core.c:151-154
    assert_eq!(request_action(-5, 3, true, false, 0), RetryAction::FailCardRemoved); // core.c:411-413
    assert_eq!(request_action(-5, 0, false, false, 0), RetryAction::FailRetriesExhausted); // core.c:411-413
}

#[test]
fn app_command_action_honours_inclusive_four_attempt_bound() {
    assert_eq!(app_command_action(0, 0, false, 0), AppCommandAction::Complete);
    assert_eq!(app_command_action(-5, 0, false, 0), AppCommandAction::Retry { next_attempt: 1 });
    assert_eq!(app_command_action(-5, 2, false, 0), AppCommandAction::Retry { next_attempt: 3 });
    assert_eq!(app_command_action(-5, 3, false, 0), AppCommandAction::FailAttemptsExhausted { attempts: 4, maximum_attempts: 4 }); // sd_ops.c:86
    assert_eq!(app_command_action(-5, u32::MAX, false, 0), AppCommandAction::FailAttemptsExhausted { attempts: u32::MAX, maximum_attempts: 4 }, "an invalid caller attempt cannot wrap back into retry");
    assert_eq!(app_command_action(-5, 0, true, 0x0000_0004), AppCommandAction::AbortIllegalSpiCommand); // sd_ops.c:111-115
}
