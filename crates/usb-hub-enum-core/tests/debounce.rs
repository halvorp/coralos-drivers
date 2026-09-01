// SPDX-License-Identifier: GPL-2.0-only
//! Debounce vectors from Linux hub.c. Original copyright: Linus Torvalds, Johannes Erdfelt,
//! Gregory P. Smith, Brad Hards, and the Linux USB core authors.

use usb_hub_enum_core::debounce::*;

/// hub.c:138-:140. Every timing literal is pinned independently of production arithmetic.
#[test]
fn every_debounce_timing_constant_matches_linux() {
    assert_eq!(DEBOUNCE_TIMEOUT_MS, 2_000);
    assert_eq!(DEBOUNCE_STEP_MS, 25);
    assert_eq!(DEBOUNCE_STABLE_MS, 100);
}

/// hub.c:4707-:4717 requires four unchanged 25ms increments after the initial sample.
#[test]
fn connected_status_must_be_unchanged_for_one_hundred_ms() {
    let mut d = Debouncer::new(true);
    assert_eq!(d.sample(0x0001, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(d.sample(0x0001, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(d.stable_ms(), 25);
    assert_eq!(d.sample(0x0001, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(d.sample(0x0001, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(
        d.sample(0x0001, 0),
        Ok(DebounceAction::Stable { portstatus: 0x0001, elapsed_ms: 100 })
    );
}

/// hub.c:4709-:4724 resets stability on a change and asks the caller to clear C_CONNECTION.
#[test]
fn a_change_bit_restarts_stability_and_is_explicitly_cleared() {
    let mut d = Debouncer::new(true);
    assert_eq!(
        d.sample(0x0001, 0x0001),
        Ok(DebounceAction::ClearConnectionChangeThenWait { wait_ms: 25 })
    );
    assert_eq!(d.stable_ms(), 0);
    assert_eq!(d.sample(0x0001, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(d.stable_ms(), 25);
    assert_eq!(d.sample(0, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    assert_eq!(d.stable_ms(), 0);
}

/// hub.c:4711-:4713: disconnected may stabilize only when must_be_connected is false.
#[test]
fn must_be_connected_controls_whether_disconnect_can_settle() {
    let mut allowed = Debouncer::new(false);
    allowed.sample(0, 0).unwrap();
    allowed.sample(0, 0).unwrap();
    allowed.sample(0, 0).unwrap();
    allowed.sample(0, 0).unwrap();
    assert_eq!(
        allowed.sample(0, 0),
        Ok(DebounceAction::Stable { portstatus: 0, elapsed_ms: 100 })
    );
    let mut required = Debouncer::new(true);
    for _ in 0..5 {
        assert_eq!(required.sample(0, 0), Ok(DebounceAction::Wait { wait_ms: 25 }));
    }
    assert_eq!(required.stable_ms(), 0);
}

/// hub.c:4726 and :4735. The refusal names elapsed and required stable times.
#[test]
fn unstable_connection_times_out_with_named_bounds() {
    let mut d = Debouncer::new(true);
    for n in 0..=80 {
        let status = if n % 2 == 0 { 0x0001 } else { 0 };
        if n < 80 {
            d.sample(status, 0).unwrap();
        } else {
            assert_eq!(
                d.sample(status, 0),
                Err(DebounceError::ConnectionDidNotStabilize {
                    elapsed_ms: 2_000,
                    required_stable_ms: 100,
                })
            );
        }
    }
}
