// SPDX-License-Identifier: GPL-2.0-only
//! Retry state machine from Linux `drivers/mmc/core/core.c`, `core.h`, and
//! `sd_ops.c`.
//!
//! Copyright (C) 2003-2004 Russell King; SD support Copyright (C) 2004 Ian
//! Molton; Copyright (C) 2005-2008 Pierre Ossman; MMCv4 support Copyright (C)
//! 2006 Philip Langdale.

/// Linux's ordinary command retry allowance. The first attempt is additional.
pub const MMC_CMD_RETRIES: u32 = 3; // drivers/mmc/core/core.h:18
/// Number of APP-command pairs attempted by the inclusive loop.
pub const APP_CMD_MAX_ATTEMPTS: u32 = MMC_CMD_RETRIES + 1; // drivers/mmc/core/sd_ops.c:86
/// SPI R1 bit that makes retrying an illegal command pointless.
pub const R1_SPI_ILLEGAL_COMMAND: u32 = 0x0000_0004; // include/linux/mmc/mmc.h:186

/// Linux errors explicitly used by MMC request completion/fault injection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErrorDef { pub name: &'static str, pub errno: i32 }
pub const REQUEST_ERRORS: [ErrorDef; 4] = [
    ErrorDef { name: "ETIMEDOUT", errno: -110 }, // drivers/mmc/core/core.c:87; include/uapi/asm-generic/errno.h:94
    ErrorDef { name: "EILSEQ", errno: -84 }, // drivers/mmc/core/core.c:88,146-148; include/uapi/asm-generic/errno.h:68
    ErrorDef { name: "EIO", errno: -5 }, // drivers/mmc/core/core.c:89; include/uapi/asm-generic/errno-base.h:9
    ErrorDef { name: "ENOMEDIUM", errno: -123 }, // drivers/mmc/core/core.c:346; include/uapi/asm-generic/errno.h:109
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAction {
    Complete,
    Retry { retries_remaining: u32 },
    AbortIllegalSpiCommand,
    FailCardRemoved,
    FailRetriesExhausted,
}

/// Decide the next ordinary-command action after one completed attempt.
///
/// This folds Linux's SPI illegal-command cancellation at core.c:151-154 into
/// the termination/retry loop at core.c:411-420. `error == 0` completes; a
/// removed card and exhausted retry allowance are named failures.
pub fn request_action(error: i32, retries: u32, card_removed: bool,
                      spi_mode: bool, response0: u32) -> RetryAction {
    if error == 0 {
        RetryAction::Complete
    } else if spi_mode && retries != 0 && response0 & R1_SPI_ILLEGAL_COMMAND != 0 {
        RetryAction::AbortIllegalSpiCommand
    } else if card_removed {
        RetryAction::FailCardRemoved
    } else if retries == 0 {
        RetryAction::FailRetriesExhausted
    } else {
        RetryAction::Retry { retries_remaining: retries - 1 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppCommandAction {
    Complete,
    Retry { next_attempt: u32 },
    AbortIllegalSpiCommand,
    FailAttemptsExhausted { attempts: u32, maximum_attempts: u32 },
}

/// Decide after one APP command pair from the inclusive `0..=3` Linux loop.
/// `attempt` is zero-based and therefore cannot silently exceed four tries.
pub fn app_command_action(error: i32, attempt: u32, spi_mode: bool,
                          response0: u32) -> AppCommandAction {
    if error == 0 {
        AppCommandAction::Complete
    } else if spi_mode && response0 & R1_SPI_ILLEGAL_COMMAND != 0 {
        AppCommandAction::AbortIllegalSpiCommand
    } else if attempt >= APP_CMD_MAX_ATTEMPTS - 1 {
        AppCommandAction::FailAttemptsExhausted {
            attempts: attempt.saturating_add(1),
            maximum_attempts: APP_CMD_MAX_ATTEMPTS,
        }
    } else {
        AppCommandAction::Retry { next_attempt: attempt + 1 }
    }
}
