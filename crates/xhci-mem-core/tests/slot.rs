// SPDX-License-Identifier: GPL-2.0-only
//! Slot vectors from Linux `drivers/usb/host/xhci.h` and `xhci-mem.c`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_mem_core::slot::*;

/// xhci.h:363-365; xhci-mem.c:1109. Last Context is bits 31:27 and zero has no endpoint.
#[test]
fn last_context_encodes_decodes_and_names_invalid_bounds() {
    assert_eq!(encode_last_context(1), Ok(0x0800_0000));
    assert_eq!(encode_last_context(31), Ok(0xf800_0000));
    assert_eq!(decode_last_context(0xa800_0000), 21);
    assert_eq!(last_context_to_endpoint_number(0x0800_0000), Ok(0));
    assert_eq!(last_context_to_endpoint_number(0xf800_0000), Ok(30));
    assert_eq!(encode_last_context(0), Err(SlotFieldError::LastContextOutOfRange { value: 0, minimum: 1, maximum: 31 }));
    assert_eq!(encode_last_context(32), Err(SlotFieldError::LastContextOutOfRange { value: 32, minimum: 1, maximum: 31 }));
    assert_eq!(last_context_to_endpoint_number(0), Err(SlotFieldError::LastContextHasNoEndpoint { value: 0 }));
}

/// xhci.h:373-377. Root port and max ports occupy separate bytes.
#[test]
fn dev_info2_port_fields_pack_into_their_linux_bytes() {
    assert_eq!(encode_root_hub_port(0x12), 0x0012_0000);
    assert_eq!(encode_max_ports(0x34), 0x3400_0000);
    assert_eq!(decode_root_hub_port(0x3412_abcd), 0x12);
    assert_eq!(decode_max_ports(0x3412_abcd), 0x34);
}

/// xhci.h:354-355 and :398-405.
#[test]
fn speed_and_all_distinct_slot_states_decode() {
    assert_eq!(decode_device_speed(0x00b0_0000), 0xb);
    assert_eq!(SLOT_STATE_NAMES.len(), 4);
    assert_eq!(SLOT_STATE_NAMES, ["DISABLED", "DEFAULT", "ADDRESSED", "CONFIGURED"]);
    assert_eq!(decode_slot_state(0x0000_0000), Ok(SlotState::Disabled));
    assert_eq!(decode_slot_state(0x0800_0000), Ok(SlotState::Default));
    assert_eq!(decode_slot_state(0x1000_0000), Ok(SlotState::Addressed));
    assert_eq!(decode_slot_state(0x1800_0000), Ok(SlotState::Configured));
    assert_eq!(decode_slot_state(0x2000_0000), Err(SlotFieldError::UnknownSlotState { value: 4, maximum_known: 3 }));
}

/// xhci.h:352-398. Literal masks guard field placement independently of helpers.
#[test]
fn slot_field_masks_match_linux() {
    assert_eq!(ROUTE_STRING_MASK, 0x000f_ffff);
    assert_eq!(DEVICE_SPEED_MASK, 0x00f0_0000);
    assert_eq!(DEVICE_MTT, 0x0200_0000);
    assert_eq!(DEVICE_HUB, 0x0400_0000);
    assert_eq!(LAST_CONTEXT_MASK, 0xf800_0000);
    assert_eq!(MAX_EXIT_LATENCY_MASK, 0x0000_ffff);
    assert_eq!(ROOT_HUB_PORT_MASK, 0x00ff_0000);
    assert_eq!(MAX_PORTS_MASK, 0xff00_0000);
    assert_eq!(TT_SLOT_MASK, 0x0000_00ff);
    assert_eq!(TT_PORT_MASK, 0x0000_ff00);
    assert_eq!(TT_THINK_TIME_MASK, 0x0003_0000);
    assert_eq!(DEVICE_ADDRESS_MASK, 0x0000_00ff);
    assert_eq!(SLOT_STATE_MASK, 0xf800_0000);
}
