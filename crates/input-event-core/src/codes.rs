// SPDX-License-Identifier: GPL-2.0-only
//! Event code-space bounds used by Linux `drivers/input/input.c`.
//!
//! Values are from Linux `include/uapi/linux/input-event-codes.h` and
//! `include/uapi/linux/input.h`.
//!
//! Copyright (c) 1999-2002 Vojtech Pavlik and the Linux input subsystem authors.

pub const EV_SYN: u16 = 0x00; // input-event-codes.h:39
pub const EV_KEY: u16 = 0x01; // input-event-codes.h:40
pub const EV_REL: u16 = 0x02; // input-event-codes.h:41
pub const EV_ABS: u16 = 0x03; // input-event-codes.h:42
pub const EV_MSC: u16 = 0x04; // input-event-codes.h:43
pub const EV_SW: u16 = 0x05; // input-event-codes.h:44
pub const EV_LED: u16 = 0x11; // input-event-codes.h:45
pub const EV_SND: u16 = 0x12; // input-event-codes.h:46
pub const EV_REP: u16 = 0x14; // input-event-codes.h:47
pub const EV_FF: u16 = 0x15; // input-event-codes.h:48
pub const EV_PWR: u16 = 0x16; // input-event-codes.h:49
pub const EV_FF_STATUS: u16 = 0x17; // input-event-codes.h:50
pub const EV_MAX: u16 = 0x1f; // input-event-codes.h:51
pub const EV_CNT: usize = 0x20; // input-event-codes.h:52

pub const SYN_REPORT: u16 = 0; // input-event-codes.h:58
pub const SYN_CONFIG: u16 = 1; // input-event-codes.h:59
pub const SYN_MT_REPORT: u16 = 2; // input-event-codes.h:60
pub const SYN_DROPPED: u16 = 3; // input-event-codes.h:61
pub const SYN_MAX: u16 = 0x0f; // input-event-codes.h:62
pub const SYN_CNT: usize = 0x10; // input-event-codes.h:63

pub const KEY_RESERVED: u16 = 0; // input-event-codes.h:76
pub const KEY_MAX: u16 = 0x2ff; // input-event-codes.h:833
pub const KEY_CNT: usize = 0x300; // input-event-codes.h:834
pub const REL_MAX: u16 = 0x0f; // input-event-codes.h:860
pub const REL_CNT: usize = 0x10; // input-event-codes.h:861
pub const ABS_MT_SLOT: u16 = 0x2f; // input-event-codes.h:907
pub const ABS_MT_TOUCH_MAJOR: u16 = 0x30; // input-event-codes.h:908
pub const ABS_MT_TOOL_Y: u16 = 0x3d; // input-event-codes.h:921
pub const ABS_MAX: u16 = 0x3f; // input-event-codes.h:924
pub const ABS_CNT: usize = 0x40; // input-event-codes.h:925
pub const SW_MAX: u16 = 0x11; // input-event-codes.h:951
pub const SW_CNT: usize = 0x12; // input-event-codes.h:952
pub const MSC_MAX: u16 = 0x07; // input-event-codes.h:964
pub const MSC_CNT: usize = 0x08; // input-event-codes.h:965
pub const LED_MAX: u16 = 0x0f; // input-event-codes.h:982
pub const LED_CNT: usize = 0x10; // input-event-codes.h:983
pub const REP_DELAY: u16 = 0; // input-event-codes.h:989
pub const REP_PERIOD: u16 = 1; // input-event-codes.h:990
pub const REP_MAX: u16 = 1; // input-event-codes.h:991
pub const REP_CNT: usize = 2; // input-event-codes.h:992
pub const SND_MAX: u16 = 0x07; // input-event-codes.h:1001
pub const SND_CNT: usize = 0x08; // input-event-codes.h:1002
pub const FF_MAX: u16 = 0x7f; // input.h:536
pub const FF_CNT: usize = 0x80; // input.h:537

/// One event type that has a code-space bound in Linux's `input_max_code[]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeSpace {
    pub event_type: u16,
    pub name: &'static str,
    pub max: u16,
    pub count: usize,
}

/// The eight bounded event types in `input_max_code[]` (input.c:54-63).
pub const CODE_SPACES: &[CodeSpace] = &[
    CodeSpace { event_type: EV_KEY, name: "KEY", max: KEY_MAX, count: KEY_CNT }, // input.c:55
    CodeSpace { event_type: EV_REL, name: "REL", max: REL_MAX, count: REL_CNT }, // input.c:56
    CodeSpace { event_type: EV_ABS, name: "ABS", max: ABS_MAX, count: ABS_CNT }, // input.c:57
    CodeSpace { event_type: EV_MSC, name: "MSC", max: MSC_MAX, count: MSC_CNT }, // input.c:58
    CodeSpace { event_type: EV_SW, name: "SW", max: SW_MAX, count: SW_CNT }, // input.c:59
    CodeSpace { event_type: EV_LED, name: "LED", max: LED_MAX, count: LED_CNT }, // input.c:60
    CodeSpace { event_type: EV_SND, name: "SND", max: SND_MAX, count: SND_CNT }, // input.c:61
    CodeSpace { event_type: EV_FF, name: "FF", max: FF_MAX, count: FF_CNT }, // input.c:62
];

/// Named refusal from `input_set_capability` (input.c:2071-2119).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityError {
    CodeOutOfRange { event_type: u16, code: u16, max: u16 },
    UnknownEventType { event_type: u16, code: u16 },
}

/// Return the Linux code bound for a type, when that type has one.
pub fn max_code(event_type: u16) -> Option<u16> {
    CODE_SPACES.iter().find(|space| space.event_type == event_type).map(|space| space.max)
}

/// Validate a capability as `input_set_capability` does (input.c:2071-2119).
pub fn validate_capability(event_type: u16, code: u16) -> Result<(), CapabilityError> {
    if let Some(max) = max_code(event_type) {
        if code > max {
            return Err(CapabilityError::CodeOutOfRange { event_type, code, max });
        }
        return Ok(());
    }
    if event_type == EV_PWR {
        return Ok(());
    }
    Err(CapabilityError::UnknownEventType { event_type, code })
}

/// Linux's inclusive range-and-capability test (input.c:65-69).
pub fn is_event_supported(code: u16, max: u16, capability_set: bool) -> bool {
    code <= max && capability_set
}
