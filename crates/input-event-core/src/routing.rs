// SPDX-License-Identifier: GPL-2.0-only
//! Per-type event validation, state filtering, and handler filtering from Linux
//! `drivers/input/input.c`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use crate::absolute::{filter_value, is_mt_value, AbsFilter};
use crate::codes::*;

pub const PASS_TO_HANDLERS: u8 = 1; // input.c:150
pub const PASS_TO_DEVICE: u8 = 2; // input.c:151
pub const SLOT: u8 = 4; // input.c:152
pub const FLUSH: u8 = 8; // input.c:153
pub const PASS_TO_ALL: u8 = 3; // input.c:154

/// A Linux input value (`struct input_value` at input.c:52).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
    pub event_type: u16,
    pub code: u16,
    pub value: i32,
}

/// State and capability facts needed for one `input_get_disposition` decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteContext {
    pub inhibited: bool,
    pub code_supported: bool,
    /// Current boolean state for KEY, SW, LED, and SND.
    pub current_state: bool,
    /// Current value for ABS and current setting for REP.
    pub current_value: i32,
    /// Absolute-axis fuzz.
    pub fuzz: i32,
    /// Number of MT slots, or `None` when the device is not employing slots.
    pub mt_slot_count: Option<u16>,
    /// Currently staged MT slot.
    pub mt_current_slot: u16,
    /// Slot last exposed to handlers.
    pub mt_stored_slot: u16,
}

/// The pure result of Linux's disposition decision (input.c:208-315).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RouteOutcome {
    pub disposition: u8,
    pub value: i32,
    pub state_update: Option<i32>,
    /// A valid ABS_MT_SLOT report stages this slot even though the event itself is ignored.
    pub slot_update: Option<u16>,
    /// Slot value to prepend before changed MT data (`INPUT_SLOT`, input.c:200-203).
    pub prepend_slot: Option<u16>,
}

impl RouteOutcome {
    fn ignored(value: i32) -> Self {
        Self::routed(0, value, None)
    }

    fn routed(disposition: u8, value: i32, state_update: Option<i32>) -> Self {
        Self { disposition, value, state_update, slot_update: None, prepend_slot: None }
    }
}

/// Validate and filter one event as `input_get_disposition` does (input.c:208-315).
pub fn route_event(event: Event, context: RouteContext) -> RouteOutcome {
    let Event { event_type, code, value } = event;
    if context.inhibited {
        return RouteOutcome::ignored(value);
    }

    match event_type {
        EV_SYN => {
            let disposition = match code {
                SYN_CONFIG => PASS_TO_ALL,
                SYN_REPORT => PASS_TO_HANDLERS | FLUSH,
                SYN_MT_REPORT => PASS_TO_HANDLERS,
                _ => 0,
            };
            RouteOutcome::routed(disposition, value, None)
        }
        EV_KEY => {
            if !is_event_supported(code, KEY_MAX, context.code_supported) {
                return RouteOutcome::ignored(value);
            }
            if value == 2 {
                return RouteOutcome::routed(PASS_TO_HANDLERS, value, None);
            }
            let new_state = value != 0;
            if context.current_state == new_state {
                RouteOutcome::ignored(value)
            } else {
                RouteOutcome::routed(PASS_TO_HANDLERS, value, Some(new_state as i32))
            }
        }
        EV_SW | EV_LED => {
            let max = if event_type == EV_SW { SW_MAX } else { LED_MAX };
            if !is_event_supported(code, max, context.code_supported)
                || context.current_state == (value != 0)
            {
                return RouteOutcome::ignored(value);
            }
            let disposition = if event_type == EV_SW { PASS_TO_HANDLERS } else { PASS_TO_ALL };
            RouteOutcome::routed(disposition, value, Some((value != 0) as i32))
        }
        EV_ABS => {
            if !is_event_supported(code, ABS_MAX, context.code_supported) {
                return RouteOutcome::ignored(value);
            }
            if code == ABS_MT_SLOT {
                let slot_update = context.mt_slot_count.and_then(|count| {
                    (value >= 0 && (value as u32) < count as u32).then_some(value as u16)
                });
                return RouteOutcome { slot_update, ..RouteOutcome::ignored(value) };
            }
            if is_mt_value(code) && context.mt_slot_count.is_none() {
                return RouteOutcome::routed(PASS_TO_HANDLERS, value, None);
            }
            match filter_value(value, context.current_value, context.fuzz) {
                AbsFilter::Unchanged => RouteOutcome::ignored(context.current_value),
                AbsFilter::Changed(filtered) => {
                    let new_slot = is_mt_value(code)
                        && context.mt_current_slot != context.mt_stored_slot;
                    RouteOutcome {
                        disposition: PASS_TO_HANDLERS | if new_slot { SLOT } else { 0 },
                        value: filtered,
                        state_update: Some(filtered),
                        slot_update: None,
                        prepend_slot: new_slot.then_some(context.mt_current_slot),
                    }
                }
            }
        }
        EV_REL => {
            if is_event_supported(code, REL_MAX, context.code_supported) && value != 0 {
                RouteOutcome::routed(PASS_TO_HANDLERS, value, None)
            } else {
                RouteOutcome::ignored(value)
            }
        }
        EV_MSC => {
            if is_event_supported(code, MSC_MAX, context.code_supported) {
                RouteOutcome::routed(PASS_TO_ALL, value, None)
            } else {
                RouteOutcome::ignored(value)
            }
        }
        EV_SND => {
            if !is_event_supported(code, SND_MAX, context.code_supported) {
                return RouteOutcome::ignored(value);
            }
            let update = (context.current_state != (value != 0)).then_some((value != 0) as i32);
            RouteOutcome::routed(PASS_TO_ALL, value, update)
        }
        EV_REP => {
            if code <= REP_MAX && value >= 0 && context.current_value != value {
                RouteOutcome::routed(PASS_TO_ALL, value, Some(value))
            } else {
                RouteOutcome::ignored(value)
            }
        }
        EV_FF => {
            if value >= 0 {
                RouteOutcome::routed(PASS_TO_ALL, value, None)
            } else {
                RouteOutcome::ignored(value)
            }
        }
        EV_PWR => RouteOutcome::routed(PASS_TO_ALL, value, None),
        _ => RouteOutcome::ignored(value),
    }
}

/// Compact events in place using Linux's filter convention (input.c:2551-2571).
///
/// Returning `true` from `filter` removes that event. Survivors retain their order; the returned
/// count identifies the live prefix without allocation.
pub fn filter_events(events: &mut [Event], mut filter: impl FnMut(Event) -> bool) -> usize {
    let mut end = 0;
    for index in 0..events.len() {
        let event = events[index];
        if filter(event) {
            continue;
        }
        events[end] = event;
        end += 1;
    }
    end
}
