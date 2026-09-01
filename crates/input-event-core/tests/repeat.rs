// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux's software autorepeat state machine.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use input_event_core::codes::{REP_DELAY, REP_PERIOD};
use input_event_core::repeat::*;

#[test]
fn repeat_indices_are_distinct_and_select_the_named_values() {
    // Linux stores delay at index 0 and period at index 1 (input-event-codes.h:989-990), then uses
    // those exact indices to arm and rearm the timer (input.c:89-95, input.c:2230-2233).
    assert_ne!(REP_DELAY, REP_PERIOD, "REP_DELAY must not collide with REP_PERIOD");
    let settings = [250_u32, 33_u32];
    assert_eq!(settings[REP_DELAY as usize], 250, "REP_DELAY must select the repeat delay");
    assert_eq!(settings[REP_PERIOD as usize], 33, "REP_PERIOD must select the repeat period");
}

#[test]
fn constructors_preserve_driver_values_and_apply_linux_defaults() {
    assert_eq!(AutoRepeat::new(500, 40), AutoRepeat { delay_ms: 500, period_ms: 40, repeat_key: None, timer_armed_ms: None });
    assert_eq!(AutoRepeat::with_registration_defaults(0, 0), AutoRepeat::new(250, 33)); // input.c:2351-2352
    assert_eq!(AutoRepeat::with_registration_defaults(0, 40), AutoRepeat::new(0, 40)); // both must be zero
}

#[test]
fn press_arms_delay_and_release_stops_timer() {
    let mut repeat = AutoRepeat::new(250, 33);
    repeat.on_key_event(30, 1, true, true);
    assert_eq!((repeat.repeat_key, repeat.timer_armed_ms), (Some(30), Some(250))); // input.c:87-95
    repeat.on_key_event(30, 0, true, true);
    assert_eq!(repeat.timer_armed_ms, None); // input.c:98-101
}

#[test]
fn start_requires_every_linux_condition() {
    for (ev_rep, delay, period, timer) in [
        (false, 250, 33, true),
        (true, 0, 33, true),
        (true, 250, 0, true),
        (true, 250, 33, false),
    ] {
        let mut repeat = AutoRepeat::new(delay, period);
        repeat.on_key_event(30, 1, ev_rep, timer);
        assert_eq!(repeat.timer_armed_ms, None, "input.c:89-91 requires all four conditions");
    }
}

#[test]
fn value_two_does_not_restart_or_stop_timer() {
    let mut repeat = AutoRepeat::new(250, 33);
    repeat.on_key_event(30, 1, true, true);
    repeat.on_key_event(31, 2, true, true); // input.c:139: `value != 2`
    assert_eq!((repeat.repeat_key, repeat.timer_armed_ms), (Some(30), Some(250)));
}

#[test]
fn timer_emits_and_rearms_at_period() {
    let mut repeat = AutoRepeat::new(250, 33);
    repeat.on_key_event(30, 1, true, true);
    let got = repeat.tick(RepeatTickContext { inhibited: false, key_down: true, key_supported: true });
    assert_eq!(got, TickOutcome::Emit { key: 30, next_ms: Some(33) }); // input.c:2221-2233
    assert_eq!(repeat.timer_armed_ms, Some(33));
}

#[test]
fn timer_refuses_when_key_cannot_repeat() {
    for context in [
        RepeatTickContext { inhibited: true, key_down: true, key_supported: true },
        RepeatTickContext { inhibited: false, key_down: false, key_supported: true },
        RepeatTickContext { inhibited: false, key_down: true, key_supported: false },
    ] {
        let mut repeat = AutoRepeat::new(250, 33);
        repeat.on_key_event(30, 1, true, true);
        assert_eq!(repeat.tick(context), TickOutcome::Idle); // input.c:2221-2224
        assert_eq!(repeat.timer_armed_ms, None);
    }
}

#[test]
fn zero_period_emits_once_without_rearming() {
    // Direct state represents a timer armed before period was changed to zero; callback checks it.
    let mut repeat = AutoRepeat { delay_ms: 250, period_ms: 0, repeat_key: Some(30), timer_armed_ms: Some(250) };
    assert_eq!(
        repeat.tick(RepeatTickContext { inhibited: false, key_down: true, key_supported: true }),
        TickOutcome::Emit { key: 30, next_ms: None }
    ); // input.c:2230-2233
}
