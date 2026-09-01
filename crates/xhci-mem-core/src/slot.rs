// SPDX-License-Identifier: GPL-2.0-only
//! Slot-context fields, ported from Linux `drivers/usb/host/xhci.h` and their use in
//! `drivers/usb/host/xhci-mem.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

pub const ROUTE_STRING_MASK: u32 = 0x000f_ffff; // xhci.h:352
pub const DEVICE_SPEED_MASK: u32 = 0x00f0_0000; // xhci.h:354
pub const DEVICE_MTT: u32 = 1 << 25; // xhci.h:359
pub const DEVICE_HUB: u32 = 1 << 26; // xhci.h:361
pub const LAST_CONTEXT_MASK: u32 = 0x1f << 27; // xhci.h:363
pub const MAX_EXIT_LATENCY_MASK: u32 = 0xffff; // xhci.h:371
pub const ROOT_HUB_PORT_MASK: u32 = 0xff << 16; // xhci.h:373
pub const MAX_PORTS_MASK: u32 = 0xff << 24; // xhci.h:376
pub const TT_SLOT_MASK: u32 = 0xff; // xhci.h:385
pub const TT_PORT_MASK: u32 = 0xff << 8; // xhci.h:390
pub const TT_THINK_TIME_MASK: u32 = 0x3 << 16; // xhci.h:391
pub const DEVICE_ADDRESS_MASK: u32 = 0xff; // xhci.h:395
pub const SLOT_STATE_MASK: u32 = 0x1f << 27; // xhci.h:398

/// Linux names for all four distinct slot-state encodings.
pub const SLOT_STATE_NAMES: [&str; 4] = ["DISABLED", "DEFAULT", "ADDRESSED", "CONFIGURED"]; // xhci.h:401-405

/// Slot state field values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotState {
    Disabled = 0, // xhci.h:401-402 (ENABLED is the same encoding)
    Default = 1, // xhci.h:403
    Addressed = 2, // xhci.h:404
    Configured = 3, // xhci.h:405
}

/// Encode the Last Context field (valid values 1 through 31).
pub const fn encode_last_context(last: u8) -> Result<u32, SlotFieldError> {
    if last == 0 || last > 31 {
        return Err(SlotFieldError::LastContextOutOfRange { value: last, minimum: 1, maximum: 31 });
    }
    Ok((last as u32) << 27) // xhci.h:364; xhci-mem.c:1109
}

/// Decode the Last Context field.
pub const fn decode_last_context(dev_info: u32) -> u8 {
    ((dev_info & LAST_CONTEXT_MASK) >> 27) as u8 // xhci.h:363-365
}

/// Convert Last Context to Linux's zero-based last endpoint number.
pub const fn last_context_to_endpoint_number(dev_info: u32) -> Result<u8, SlotFieldError> {
    let last = decode_last_context(dev_info);
    if last == 0 {
        return Err(SlotFieldError::LastContextHasNoEndpoint { value: last });
    }
    Ok(last - 1) // xhci.h:365
}

/// Encode the root-hub port number.
pub const fn encode_root_hub_port(port: u8) -> u32 {
    (port as u32) << 16 // xhci.h:373; xhci-mem.c:1143
}

/// Decode the root-hub port number.
pub const fn decode_root_hub_port(dev_info2: u32) -> u8 {
    ((dev_info2 >> 16) & 0xff) as u8 // xhci.h:374
}

/// Encode the number of downstream hub ports.
pub const fn encode_max_ports(ports: u8) -> u32 {
    (ports as u32) << 24 // xhci.h:376
}

/// Decode the number of downstream hub ports.
pub const fn decode_max_ports(dev_info2: u32) -> u8 {
    ((dev_info2 >> 24) & 0xff) as u8 // xhci.h:377
}

/// Decode the device speed field.
pub const fn decode_device_speed(dev_info: u32) -> u8 {
    ((dev_info & DEVICE_SPEED_MASK) >> 20) as u8 // xhci.h:354-355
}

/// Decode slot state.
pub const fn decode_slot_state(dev_state: u32) -> Result<SlotState, SlotFieldError> {
    let value = ((dev_state & SLOT_STATE_MASK) >> 27) as u8; // xhci.h:398-399
    match value {
        0 => Ok(SlotState::Disabled),
        1 => Ok(SlotState::Default),
        2 => Ok(SlotState::Addressed),
        3 => Ok(SlotState::Configured),
        _ => Err(SlotFieldError::UnknownSlotState { value, maximum_known: 3 }),
    }
}

/// A slot-field refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotFieldError {
    LastContextOutOfRange { value: u8, minimum: u8, maximum: u8 },
    LastContextHasNoEndpoint { value: u8 },
    UnknownSlotState { value: u8, maximum_known: u8 },
}
