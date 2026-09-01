// SPDX-License-Identifier: GPL-2.0-only
//! Frozen vectors for Linux input event dispositions and handler filtering.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use input_event_core::codes::*;
use input_event_core::routing::*;

fn context() -> RouteContext {
    RouteContext {
        inhibited: false,
        code_supported: true,
        current_state: false,
        current_value: 0,
        fuzz: 0,
        mt_slot_count: None,
        mt_current_slot: 0,
        mt_stored_slot: 0,
    }
}

fn event(event_type: u16, code: u16, value: i32) -> Event { Event { event_type, code, value } }

#[test]
fn disposition_bits_are_linux_literals() {
    assert_eq!(PASS_TO_HANDLERS, 1); // input.c:150
    assert_eq!(PASS_TO_DEVICE, 2); // input.c:151
    assert_eq!(SLOT, 4); // input.c:152
    assert_eq!(FLUSH, 8); // input.c:153
    assert_eq!(PASS_TO_ALL, 3); // input.c:154
}

#[test]
fn sync_codes_have_distinct_routes() {
    assert_eq!(route_event(event(EV_SYN, SYN_CONFIG, 1), context()).disposition, 3); // input.c:222-224
    assert_eq!(route_event(event(EV_SYN, SYN_REPORT, 1), context()).disposition, 9); // input.c:226-227
    assert_eq!(route_event(event(EV_SYN, SYN_MT_REPORT, 1), context()).disposition, 1); // input.c:229-230
    assert_eq!(route_event(event(EV_SYN, SYN_DROPPED, 1), context()).disposition, 0);
}

#[test]
fn key_repeat_bypasses_state_and_duplicate_keys_are_filtered() {
    let repeat = route_event(event(EV_KEY, 30, 2), context());
    assert_eq!((repeat.disposition, repeat.state_update), (1, None)); // input.c:238-241

    let press = route_event(event(EV_KEY, 30, 1), context());
    assert_eq!((press.disposition, press.state_update), (1, Some(1))); // input.c:243-247

    let duplicate = route_event(event(EV_KEY, 30, 1), RouteContext { current_state: true, ..context() });
    assert_eq!(duplicate.disposition, 0);
    let unsupported = route_event(event(EV_KEY, 30, 1), RouteContext { code_supported: false, ..context() });
    assert_eq!(unsupported.disposition, 0);
}

#[test]
fn every_non_sync_type_has_a_linux_vector() {
    assert_eq!(route_event(event(EV_SW, 0, 1), context()).disposition, 1); // input.c:252-257
    assert_eq!(route_event(event(EV_ABS, 0, 7), context()).disposition, 1); // input.c:261-263
    assert_eq!(route_event(event(EV_REL, 0, 1), context()).disposition, 1); // input.c:267-269
    assert_eq!(route_event(event(EV_REL, 0, 0), context()).disposition, 0);
    assert_eq!(route_event(event(EV_MSC, 0, -9), context()).disposition, 3); // input.c:273-275
    assert_eq!(route_event(event(EV_LED, 0, 1), context()).disposition, 3); // input.c:279-284
    assert_eq!(route_event(event(EV_SND, 0, 1), context()).disposition, 3); // input.c:288-293
    assert_eq!(route_event(event(EV_REP, REP_PERIOD, 33), context()).disposition, 3); // input.c:297-300
    assert_eq!(route_event(event(EV_FF, 0, 0), context()).disposition, 3); // input.c:304-306
    assert_eq!(route_event(event(EV_FF, 0, -1), context()).disposition, 0);
    assert_eq!(route_event(event(EV_PWR, 999, -1), context()).disposition, 3); // input.c:309-310
}

#[test]
fn inhibited_device_filters_everything_first() {
    let got = route_event(event(EV_PWR, 0, 1), RouteContext { inhibited: true, ..context() });
    assert_eq!(got.disposition, 0); // input.c:214-216
}

#[test]
fn absolute_values_are_defuzzed_before_routing() {
    let ctx = RouteContext { current_value: 100, fuzz: 20, ..context() };
    assert_eq!(route_event(event(EV_ABS, 0, 105), ctx).disposition, 0);
    let changed = route_event(event(EV_ABS, 0, 130), ctx);
    assert_eq!((changed.disposition, changed.value, changed.state_update), (1, 115, Some(115)));
}

#[test]
fn mt_slots_are_staged_then_prepended_on_real_data() {
    let ctx = RouteContext { mt_slot_count: Some(3), mt_current_slot: 0, ..context() };
    let staged = route_event(event(EV_ABS, ABS_MT_SLOT, 2), ctx);
    assert_eq!((staged.disposition, staged.slot_update), (0, Some(2))); // input.c:158-169
    assert_eq!(route_event(event(EV_ABS, ABS_MT_SLOT, 3), ctx).slot_update, None);

    let data_ctx = RouteContext { mt_slot_count: Some(3), mt_current_slot: 2, mt_stored_slot: 0, ..context() };
    let data = route_event(event(EV_ABS, ABS_MT_TOUCH_MAJOR, 10), data_ctx);
    assert_eq!((data.disposition, data.prepend_slot), (5, Some(2))); // input.c:200-203
}

#[test]
fn mt_without_slots_bypasses_duplicate_filtering() {
    let ctx = RouteContext { current_value: 10, ..context() };
    let got = route_event(event(EV_ABS, ABS_MT_TOUCH_MAJOR, 10), ctx);
    assert_eq!(got.disposition, 1); // input.c:181-184
    assert_eq!(got.state_update, None);
}

#[test]
fn filter_events_removes_true_results_and_preserves_order() {
    let mut values = [event(EV_KEY, 1, 1), event(EV_KEY, 2, 0), event(EV_KEY, 3, 1)];
    let count = filter_events(&mut values, |value| value.value == 0); // input.c:2560-2568
    assert_eq!(count, 2);
    assert_eq!(&values[..count], &[event(EV_KEY, 1, 1), event(EV_KEY, 3, 1)]);
}
