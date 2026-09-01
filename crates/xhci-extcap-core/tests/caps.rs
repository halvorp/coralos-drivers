// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for Linux `drivers/usb/host/xhci-ext-caps.h` capability definitions.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_extcap_core::caps::*;

#[test]
fn list_geometry_and_header_fields_match_linux() {
    assert_eq!(HCC_PARAMS_OFFSET, 0x10); // xhci-ext-caps.h:17
    assert_eq!(MAX_EXT_CAPS, 50); // xhci-ext-caps.h:25
    assert_eq!(hcc_ext_caps(0xabcd_1234), 0xabcd); // xhci-ext-caps.h:19
    assert_eq!(ext_cap_id(0xbeef_342a), 0x2a); // xhci-ext-caps.h:32
    assert_eq!(ext_cap_next(0xbeef_342a), 0x34); // xhci-ext-caps.h:33
    assert_eq!(ext_cap_value(0xbeef_342a), 0xbeef); // xhci-ext-caps.h:34
}

#[test]
fn all_eight_named_capability_ids_match_linux_in_order() {
    assert_eq!(EXT_CAP_IDS.len(), 8); // xhci-ext-caps.h:36-45
    assert_eq!(
        EXT_CAP_IDS,
        [
            ("LEGACY", 1),
            ("PROTOCOL", 2),
            ("PM", 3),
            ("VIRT", 4),
            ("ROUTE", 5),
            ("DEBUG", 10),
            ("VENDOR_INTEL", 192),
            ("INTEL_SPR_SHADOW", 206),
        ]
    ); // xhci-ext-caps.h:36-45

    assert_eq!(EXT_CAP_LEGACY, 1); // xhci-ext-caps.h:36
    assert_eq!(EXT_CAP_PROTOCOL, 2); // xhci-ext-caps.h:37
    assert_eq!(EXT_CAP_PM, 3); // xhci-ext-caps.h:38
    assert_eq!(EXT_CAP_VIRT, 4); // xhci-ext-caps.h:39
    assert_eq!(EXT_CAP_ROUTE, 5); // xhci-ext-caps.h:40
    assert_eq!(EXT_CAP_DEBUG, 10); // xhci-ext-caps.h:42
    assert_eq!(EXT_CAP_VENDOR_INTEL, 192); // xhci-ext-caps.h:44
    assert_eq!(EXT_CAP_INTEL_SPR_SHADOW, 206); // xhci-ext-caps.h:45
}

#[test]
fn legacy_support_layout_and_masks_match_linux() {
    assert_eq!(HC_BIOS_OWNED, 0x0001_0000); // xhci-ext-caps.h:47
    assert_eq!(HC_OS_OWNED, 0x0100_0000); // xhci-ext-caps.h:48
    assert_eq!(LEGACY_SUPPORT_OFFSET, 0x00); // xhci-ext-caps.h:52
    assert_eq!(LEGACY_CONTROL_OFFSET, 0x04); // xhci-ext-caps.h:56
    assert_eq!(LEGACY_DISABLE_SMI, 0x000e_1fee); // xhci-ext-caps.h:58
    assert_eq!(LEGACY_SMI_EVENTS, 0xe000_0000); // xhci-ext-caps.h:59
    assert_eq!(L1C, 0x0001_0000); // xhci-ext-caps.h:62
    assert_eq!(HLC, 0x0008_0000); // xhci-ext-caps.h:65
    assert_eq!(BLC, 0x0010_0000); // xhci-ext-caps.h:66
}

#[test]
fn zero_valued_legacy_support_offset_selects_the_support_header() {
    let capability: [u32; 2] = [0x0100_0001, 0xe00e_1fee];
    assert_ne!(LEGACY_SUPPORT_OFFSET, LEGACY_CONTROL_OFFSET); // xhci-ext-caps.h:52,56
    assert_eq!(
        capability[(LEGACY_SUPPORT_OFFSET / 4) as usize],
        0x0100_0001
    ); // xhci-ext-caps.h:52
    assert_eq!(
        capability[(LEGACY_CONTROL_OFFSET / 4) as usize],
        0xe00e_1fee
    ); // xhci-ext-caps.h:56
}
