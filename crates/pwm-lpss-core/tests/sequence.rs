// SPDX-License-Identifier: GPL-2.0-only
//! Update sequencing vectors from Linux `drivers/pwm/pwm-lpss.c` and
//! `drivers/pwm/pwm-lpss.h`.
//!
//! Copyright (C) 2014 Intel Corporation and the Linux pwm-lpss authors.

use pwm_lpss_core::sequence::{
    update_cleared, UpdateAction, UpdateRefusal, UpdateSequence, UPDATE_BUSY_MESSAGE,
    UPDATE_POLL_INTERVAL_US, UPDATE_TIMEOUT_MESSAGE, UPDATE_TIMEOUT_US,
};

/// pwm-lpss.c:93, :108, :110, :118. Messages and waits are Linux literals.
#[test]
fn wait_constants_and_messages_match_linux() {
    assert_eq!(UPDATE_POLL_INTERVAL_US, 40); // pwm-lpss.c:108
    assert_eq!(UPDATE_TIMEOUT_US, 500_000); // pwm-lpss.c:93
    assert_eq!(UPDATE_TIMEOUT_MESSAGE, "PWM_SW_UPDATE was not cleared"); // pwm-lpss.c:110
    assert_eq!(
        UPDATE_BUSY_MESSAGE,
        "PWM_SW_UPDATE is still set, skipping update"
    ); // pwm-lpss.c:118
}

/// pwm-lpss.c:117-:120 refuses before writing if SW_UPDATE remains set.
#[test]
fn an_in_progress_update_is_refused_by_name() {
    assert_eq!(
        UpdateSequence::start(0x4000_0001, 0x0000_22c0, 0x4000_22c0, false),
        Err(UpdateRefusal::SwUpdateStillSet {
            ctrl: 0x4000_0001,
            mask: 0x4000_0000,
            message: "PWM_SW_UPDATE is still set, skipping update",
        })
    );
}

/// pwm-lpss.c:156-:182. Non-bypass hardware enables BEFORE waiting.
#[test]
fn ordinary_hardware_enables_before_waiting_for_update_clear() {
    let mut seq = UpdateSequence::start(0, 0x0000_22c0, 0x4000_22c0, false).unwrap();
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::WriteConfigured(0x0000_22c0)
    );
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::WriteCommitted(0x4000_22c0)
    );
    assert_eq!(
        seq.advance(0x4000_22c0).unwrap(),
        UpdateAction::WriteEnabled(0xc000_22c0)
    );
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::PollForUpdateClear {
            interval_us: 40,
            timeout_us: 500_000
        }
    );
    assert_eq!(seq.advance(0x8000_22c0).unwrap(), UpdateAction::Complete);
}

/// pwm-lpss.c:176-:183. Bypass hardware waits FIRST and enables only after
/// observing SW_UPDATE clear; reversing these two actions changes Linux's
/// hardware workaround.
#[test]
fn bypass_hardware_enables_only_after_the_wait() {
    let mut seq = UpdateSequence::start(0, 0x0000_22c0, 0x4000_22c0, true).unwrap();
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::WriteConfigured(0x0000_22c0)
    );
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::WriteCommitted(0x4000_22c0)
    );
    assert_eq!(
        seq.advance(0).unwrap(),
        UpdateAction::PollForUpdateClear {
            interval_us: 40,
            timeout_us: 500_000
        }
    );
    assert_eq!(
        seq.advance(0x0000_22c0).unwrap(),
        UpdateAction::WriteEnabled(0x8000_22c0)
    );
    assert_eq!(seq.advance(0).unwrap(), UpdateAction::Complete);
}

/// pwm-lpss.c:108-:112 returns the poll error and emits a named diagnostic.
#[test]
fn a_poll_that_never_clears_returns_the_named_timeout() {
    let mut seq = UpdateSequence::start(0, 0x22c0, 0x4000_22c0, true).unwrap();
    let _ = seq.advance(0).unwrap();
    let _ = seq.advance(0).unwrap();
    let _ = seq.advance(0).unwrap();
    assert_eq!(
        seq.advance(0x4000_22c0),
        Err(UpdateRefusal::SwUpdateWasNotCleared {
            ctrl: 0x4000_22c0,
            timeout_us: 500_000,
            message: "PWM_SW_UPDATE was not cleared",
        })
    );
}

/// pwm-lpss.c:108 polls until `!(val & PWM_SW_UPDATE)`; all other bits are
/// irrelevant to this predicate.
#[test]
fn update_clear_predicate_checks_only_the_linux_bit() {
    assert!(update_cleared(0x8000_ffff));
    assert!(!update_cleared(0x4000_0000));
    assert!(!update_cleared(0xffff_ffff));
}
