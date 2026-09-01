// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for code-space bounds from input-event-codes.h/input.h and input.c.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

use input_event_core::codes::*;

/// input.c:54-63 has exactly these eight bounded type names. This list is literal, not derived.
const LINUX_CODE_SPACE_NAMES: [&str; 8] = ["KEY", "REL", "ABS", "MSC", "SW", "LED", "SND", "FF"];

#[test]
fn linux_code_spaces_are_complete_by_count_and_name() {
    assert_eq!(CODE_SPACES.len(), 8); // input.c:55-62
    let ours: Vec<&str> = CODE_SPACES.iter().map(|space| space.name).collect();
    assert_eq!(ours, LINUX_CODE_SPACE_NAMES);
}

#[test]
fn linux_bounds_and_counts_are_literal() {
    assert_eq!((EV_MAX, EV_CNT), (0x1f, 0x20)); // input-event-codes.h:51-52
    assert_eq!((SYN_MAX, SYN_CNT), (0x0f, 0x10)); // input-event-codes.h:62-63
    assert_eq!((KEY_MAX, KEY_CNT), (0x2ff, 0x300)); // input-event-codes.h:833-834
    assert_eq!((REL_MAX, REL_CNT), (0x0f, 0x10)); // input-event-codes.h:860-861
    assert_eq!((ABS_MAX, ABS_CNT), (0x3f, 0x40)); // input-event-codes.h:924-925
    assert_eq!((SW_MAX, SW_CNT), (0x11, 0x12)); // input-event-codes.h:951-952
    assert_eq!((MSC_MAX, MSC_CNT), (0x07, 0x08)); // input-event-codes.h:964-965
    assert_eq!((LED_MAX, LED_CNT), (0x0f, 0x10)); // input-event-codes.h:982-983
    assert_eq!((REP_MAX, REP_CNT), (0x01, 0x02)); // input-event-codes.h:991-992
    assert_eq!((SND_MAX, SND_CNT), (0x07, 0x08)); // input-event-codes.h:1001-1002
    assert_eq!((FF_MAX, FF_CNT), (0x7f, 0x80)); // input.h:536-537
}

#[test]
fn event_type_names_and_values_are_literal() {
    // input-event-codes.h:39-50: all twelve named event types, independently frozen.
    let expected = [
        ("SYN", 0x00), ("KEY", 0x01), ("REL", 0x02), ("ABS", 0x03),
        ("MSC", 0x04), ("SW", 0x05), ("LED", 0x11), ("SND", 0x12),
        ("REP", 0x14), ("FF", 0x15), ("PWR", 0x16), ("FF_STATUS", 0x17),
    ];
    let ours = [
        ("SYN", EV_SYN), ("KEY", EV_KEY), ("REL", EV_REL), ("ABS", EV_ABS),
        ("MSC", EV_MSC), ("SW", EV_SW), ("LED", EV_LED), ("SND", EV_SND),
        ("REP", EV_REP), ("FF", EV_FF), ("PWR", EV_PWR), ("FF_STATUS", EV_FF_STATUS),
    ];
    assert_eq!(ours, expected);
}

#[test]
fn max_code_maps_only_linux_bounded_types() {
    assert_eq!(max_code(EV_KEY), Some(0x2ff)); // input.c:55
    assert_eq!(max_code(EV_ABS), Some(0x3f)); // input.c:57
    assert_eq!(max_code(EV_REP), None); // input.c:54-63 leaves this array entry zero
}

#[test]
fn capability_validation_names_the_value_and_bound() {
    assert_eq!(validate_capability(EV_ABS, 0x3f), Ok(())); // input.c:2073: inclusive
    assert_eq!(
        validate_capability(EV_ABS, 0x40),
        Err(CapabilityError::CodeOutOfRange { event_type: 0x03, code: 0x40, max: 0x3f })
    );
    assert_eq!(validate_capability(EV_PWR, 99), Ok(())); // input.c:2109-2111
    assert_eq!(
        validate_capability(EV_REP, 0),
        Err(CapabilityError::UnknownEventType { event_type: 0x14, code: 0 })
    ); // input.c:2113-2117
}

#[test]
fn support_check_requires_both_bound_and_bit() {
    assert!(is_event_supported(0x2ff, 0x2ff, true)); // input.c:68
    assert!(!is_event_supported(0x300, 0x2ff, true));
    assert!(!is_event_supported(1, 0x2ff, false));
}
