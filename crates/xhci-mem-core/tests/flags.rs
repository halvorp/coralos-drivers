// SPDX-License-Identifier: GPL-2.0-only
//! Configure-endpoint flag vectors from Linux `drivers/usb/host/xhci.h` and `xhci.c`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_mem_core::flags::*;

/// xhci.h:366-367 and xhci.c:1480-1485. Slot is bit 0; endpoint index N is bit N+1.
#[test]
fn control_and_endpoint_flags_have_literal_positions() {
    assert_eq!(CONTROL_FLAG_NAMES.len(), 2);
    assert_eq!(CONTROL_FLAG_NAMES, ["SLOT", "EP0"]);
    assert_eq!(SLOT_FLAG, 0x0000_0001);
    assert_eq!(EP0_FLAG, 0x0000_0002);
    assert_eq!(endpoint_flag(0), Ok(0x0000_0002));
    assert_eq!(endpoint_flag(1), Ok(0x0000_0004));
    assert_eq!(endpoint_flag(30), Ok(0x8000_0000));
    assert_eq!(endpoint_flag(31), Err(FlagError::EndpointIndexOutOfRange { index: 31, maximum: 30 }));
}

/// xhci.h:519-522. Added and dropped predicates read distinct words.
#[test]
fn add_and_drop_predicates_do_not_cross_words() {
    let flags = ConfigureFlags { add: 0x0000_0008, drop: 0x0000_0010 };
    assert_eq!(endpoint_is_added(flags, 2), Ok(true));
    assert_eq!(endpoint_is_dropped(flags, 2), Ok(false));
    assert_eq!(endpoint_is_added(flags, 3), Ok(false));
    assert_eq!(endpoint_is_dropped(flags, 3), Ok(true));
    assert_eq!(endpoint_is_added(flags, 31), Err(FlagError::EndpointIndexOutOfRange { index: 31, maximum: 30 }));
    assert_eq!(endpoint_is_dropped(flags, 31), Err(FlagError::EndpointIndexOutOfRange { index: 31, maximum: 30 }));
}

/// xhci.c:1953-1957. Dropping sets Dn and clears a stale An.
#[test]
fn drop_sets_drop_and_clears_add() {
    let before = ConfigureFlags { add: 0x0000_000c, drop: 0x0000_0010 };
    assert_eq!(request_drop(before, 2), Ok(ConfigureFlags { add: 0x0000_0004, drop: 0x0000_0018 }));
    assert_eq!(request_drop(ConfigureFlags::default(), 0), Err(FlagError::CannotDropDefaultControlEndpoint { ep_index: 0 }));
}

/// xhci.c:2029-2069. Existing endpoints require a prior drop; re-add preserves Dn as a change.
#[test]
fn add_requires_prior_drop_for_in_use_endpoint_and_preserves_it() {
    assert_eq!(request_add(ConfigureFlags::default(), 2, true), Err(FlagError::EndpointInUseWithoutDrop { ep_index: 2 }));
    let dropped = ConfigureFlags { add: 0, drop: 0x0000_0008 };
    assert_eq!(request_add(dropped, 2, true), Ok(ConfigureFlags { add: 0x0000_0008, drop: 0x0000_0008 }));
    assert_eq!(request_add(ConfigureFlags { add: 0x8, drop: 0 }, 2, false), Err(FlagError::EndpointAlreadyMarkedAdded { ep_index: 2 }));
    assert_eq!(request_add(ConfigureFlags::default(), 0, false), Err(FlagError::CannotAddDefaultControlEndpoint { ep_index: 0 }));
}

/// xhci.c:3110-3117, section 4.6.6: A0=1 and A1=D0=D1=0.
#[test]
fn configure_preparation_enforces_slot_and_ep0_rules() {
    assert_eq!(prepare_configure(ConfigureFlags { add: 0x0000_000a, drop: 0x0000_000f }), ConfigureFlags { add: 0x0000_0009, drop: 0x0000_000c });
    assert_eq!(prepare_configure(ConfigureFlags::default()), ConfigureFlags { add: 1, drop: 0 });
}

/// xhci.c:2221-2252. A bit in both words changes an endpoint and counts as neither new nor dropped.
#[test]
fn resource_counts_ignore_slot_ep0_and_changed_endpoints() {
    let flags = ConfigureFlags {
        add: 0x0000_001f,  // slot, EP0, endpoint indices 1,2,3
        drop: 0x0000_003b, // slot, EP0, endpoint indices 2,3,4
    };
    assert_eq!(count_new_endpoints(flags), 1, "only endpoint index 1 is genuinely new");
    assert_eq!(count_dropped_endpoints(flags), 1, "only endpoint index 4 is genuinely dropped");
    assert_eq!(count_new_endpoints(ConfigureFlags { add: 3, drop: 3 }), 0);
    assert_eq!(count_dropped_endpoints(ConfigureFlags { add: 3, drop: 3 }), 0);
}
