// SPDX-License-Identifier: GPL-2.0-only
//! Reset vectors from Linux hub.c. Original copyright: Linus Torvalds, Johannes Erdfelt,
//! Gregory P. Smith, Brad Hards, and the Linux USB core authors.

use usb_hub_enum_core::reset::*;

/// hub.c:2892, :2901-:2905, :3153-:3163. Values are literal, not derived.
#[test]
fn every_reset_count_and_timing_constant_is_pinned() {
    assert_eq!(PORT_RESET_TRIES, 5);
    assert_eq!(ROOT_RESET_MS, 60);
    assert_eq!(SHORT_RESET_MS, 10);
    assert_eq!(BH_RESET_MS, 50);
    assert_eq!(LONG_RESET_MS, 200);
    assert_eq!(RESET_TIMEOUT_MS, 800);
    assert_eq!(RESET_RECOVERY_MS, 50);
    assert_eq!(SLOW_HUB_EXTRA_RECOVERY_MS, 100);
    assert_eq!(FAST_ENUM_RECOVERY_MIN_US, 10_000);
    assert_eq!(FAST_ENUM_RECOVERY_MAX_US, 12_000);
}

/// hub.c:4909, :4923-:4935 and :3158-:3163.
#[test]
fn initial_and_recovery_delays_match_linux_precedence() {
    assert_eq!(initial_reset_delay_ms(false, false), 10);
    assert_eq!(initial_reset_delay_ms(true, false), 60);
    assert_eq!(initial_reset_delay_ms(false, true), 200);
    assert_eq!(initial_reset_delay_ms(true, true), 200, "low-speed override follows root check");
    assert_eq!(reset_recovery_ms(false), 50);
    assert_eq!(reset_recovery_ms(true), 150);
}

/// hub.c:2961-:2992 tests the pre-increment loop counter, so three 10ms waits precede 200ms.
#[test]
fn reset_wait_switches_from_short_to_long_at_linuxs_loop_boundary() {
    let mut wait = ResetWait::new(10, false, false);
    assert_eq!(wait.next_delay_ms(), 10);
    assert_eq!(wait.sample(0x0011, 0), Ok(ResetAction::Wait { delay_ms: 10 }));
    assert_eq!(wait.elapsed_ms(), 10);
    assert_eq!(wait.sample(0x0011, 0), Ok(ResetAction::Wait { delay_ms: 10 }));
    assert_eq!(wait.elapsed_ms(), 20);
    assert_eq!(wait.sample(0x0011, 0), Ok(ResetAction::Wait { delay_ms: 200 }));
    assert_eq!(wait.elapsed_ms(), 220);
    assert_eq!(wait.sample(0x0003, 0), Ok(ResetAction::Complete));
}

/// hub.c:2999-:3021 preserves distinct reasons; no bare failure.
#[test]
fn every_reset_refusal_is_named() {
    let mut timed_out = ResetWait::new(800, false, false);
    assert_eq!(
        timed_out.sample(0x0011, 0),
        Err(ResetError::ResetTimedOut { elapsed_ms: 800, timeout_ms: 800 })
    );
    let mut disconnected = ResetWait::new(800, false, false);
    assert_eq!(disconnected.sample(0, 0), Err(ResetError::DeviceDisconnected));
    let mut bounced = ResetWait::new(10, false, false);
    assert_eq!(bounced.sample(0x0003, 0x0001), Err(ResetError::ConnectionBounced));
    let mut disabled = ResetWait::new(10, false, false);
    assert_eq!(disabled.sample(0x0001, 0), Err(ResetError::PortNotEnabled));
    let mut warm = ResetWait::new(50, true, false);
    assert_eq!(warm.sample(0x00c3, 0), Err(ResetError::WarmResetStillRequired));
}

/// hub.c:3012-:3018 ignores C_CONNECTION for SuperSpeed but not USB2.
#[test]
fn superspeed_connection_bounce_is_ignored_after_successful_reset() {
    let mut ss = ResetWait::new(50, true, false);
    assert_eq!(ss.sample(0x0003, 0x0001), Ok(ResetAction::Complete));
    let mut usb2 = ResetWait::new(10, false, false);
    assert_eq!(usb2.sample(0x0003, 0x0001), Err(ResetError::ConnectionBounced));
}
