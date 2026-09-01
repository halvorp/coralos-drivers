// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for the load-bearing CMD6 busy state machine.
//!
//! Ported from Linux `drivers/mmc/core/mmc_ops.c` and
//! `include/linux/mmc/mmc.h`. Copyright 2006-2007 Pierre Ossman and the Linux
//! MMC authors.

use mmc_ops_core::busy::*;

/// mmc_ops.c:506 and mmc.h:170-:177. PRG must remain busy even if bit 8 is
/// raised; TRAN must remain busy while bit 8 is clear. Only BOTH conditions
/// together let the next command proceed.
#[test]
fn poll_leaves_prog_only_on_ready_for_data_and_transfer() {
    let mut poller = BusyPoller::new(250);
    assert_eq!(
        poller.sample(0, 0x0e00),
        Ok(BusyPoll::PollAgain { next_delay_us: 32 }),
        "PRG without READY_FOR_DATA remains busy"
    );
    assert_eq!(
        poller.sample(1, 0x0f00),
        Ok(BusyPoll::PollAgain { next_delay_us: 64 }),
        "PRG with a misleading READY_FOR_DATA bit still remains busy"
    );
    assert_eq!(
        poller.sample(2, 0x0800),
        Ok(BusyPoll::PollAgain { next_delay_us: 128 }),
        "TRAN without READY_FOR_DATA remains busy"
    );
    assert_eq!(poller.sample(3, 0x0900), Ok(BusyPoll::Ready));
}

/// mmc_ops.c:524, :532-:537 uses `time_after`, so the exact deadline is still
/// sampled and a busy refusal occurs strictly after it. These are both sides
/// of the boundary demanded by the busy contract.
#[test]
fn timeout_boundary_refuses_a_still_busy_card_by_name() {
    let mut at_boundary = BusyPoller::new(250);
    assert_eq!(
        at_boundary.sample(250, 0x0e00),
        Ok(BusyPoll::PollAgain { next_delay_us: 32 })
    );

    let mut after_boundary = BusyPoller::new(250);
    assert_eq!(
        after_boundary.sample(251, 0x0e00),
        Err(BusyRefusal::CardStuckBusy {
            elapsed_ms: 251,
            timeout_ms: 250,
            status: 0x0e00,
        })
    );
}

/// mmc_ops.c:524-:537 checks expiration before the callback but only refuses
/// when the resulting sample is still busy. A card becoming ready on that
/// late sample succeeds rather than producing a stale timeout.
#[test]
fn a_ready_sample_after_the_deadline_succeeds() {
    let mut poller = BusyPoller::new(250);
    assert_eq!(poller.sample(251, 0x0900), Ok(BusyPoll::Ready));
}

/// mmc_ops.c:438-:465, :489-:491 — CMD6 polling validates SWITCH_ERROR rather
/// than silently treating a failed switch as completion.
#[test]
fn switch_error_is_a_named_refusal() {
    assert_eq!(R1_SWITCH_ERROR, 0x80); // include/linux/mmc/mmc.h:156
    let mut poller = BusyPoller::new(250);
    assert_eq!(
        poller.sample(1, 0x0980),
        Err(BusyRefusal::SwitchError { status: 0x0980 })
    );
}

/// The state field is four bits but Linux defines values only through DIS=8
/// (mmc.h:160-:168). Unknown state 15 must be surfaced by name, not accepted.
#[test]
fn reserved_r1_state_is_a_named_refusal() {
    let mut poller = BusyPoller::new(250);
    assert_eq!(
        poller.sample(1, 0x1e00),
        Err(BusyRefusal::ReservedCardState {
            value: 15,
            maximum_defined: 8
        })
    );
}

/// mmc_ops.c:519, :539-:543 — default 32us exponential backoff capped at
/// 32768us. `with_period` also has a vector for its public explicit-period path.
#[test]
fn poll_backoff_matches_linux_and_caps_without_wrapping() {
    assert_eq!(INITIAL_POLL_DELAY_US, 32);
    assert_eq!(MAX_POLL_DELAY_US, 32_768);

    let mut poller = BusyPoller::with_period(1_000, 10_000);
    assert_eq!(
        poller.sample(0, 0x0e00),
        Ok(BusyPoll::PollAgain {
            next_delay_us: 10_000
        })
    );
    assert_eq!(
        poller.sample(1, 0x0e00),
        Ok(BusyPoll::PollAgain {
            next_delay_us: 20_000
        })
    );
    assert_eq!(
        poller.sample(2, 0x0e00),
        Ok(BusyPoll::PollAgain {
            next_delay_us: 40_000
        })
    );
    assert_eq!(
        poller.sample(3, 0x0e00),
        Ok(BusyPoll::PollAgain {
            next_delay_us: 40_000
        })
    );

    let defaulted = BusyPoller::with_period(1, 0);
    assert_eq!(defaulted, BusyPoller::new(1));
}
