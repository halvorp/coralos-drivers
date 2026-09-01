// SPDX-License-Identifier: GPL-2.0-only
//! Extended-capability and USB Legacy Support register definitions.
//!
//! Ported from Linux `drivers/usb/host/xhci-ext-caps.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

/// HCCPARAMS1 offset from the controller capability-register base.
pub const HCC_PARAMS_OFFSET: u32 = 0x10; // xhci-ext-caps.h:17
/// Maximum number of extended capabilities Linux permits a caller to inspect.
pub const MAX_EXT_CAPS: usize = 50; // xhci-ext-caps.h:25

/// Extended-capability IDs, in Linux declaration order. IDs 6 through 9 are reserved.
pub const EXT_CAP_IDS: [(&str, u8); 8] = [
    ("LEGACY", 1),             // xhci-ext-caps.h:36
    ("PROTOCOL", 2),           // xhci-ext-caps.h:37
    ("PM", 3),                 // xhci-ext-caps.h:38
    ("VIRT", 4),               // xhci-ext-caps.h:39
    ("ROUTE", 5),              // xhci-ext-caps.h:40
    ("DEBUG", 10),             // xhci-ext-caps.h:42
    ("VENDOR_INTEL", 192),     // xhci-ext-caps.h:44
    ("INTEL_SPR_SHADOW", 206), // xhci-ext-caps.h:45
];

pub const EXT_CAP_LEGACY: u8 = 1; // xhci-ext-caps.h:36
pub const EXT_CAP_PROTOCOL: u8 = 2; // xhci-ext-caps.h:37
pub const EXT_CAP_PM: u8 = 3; // xhci-ext-caps.h:38
pub const EXT_CAP_VIRT: u8 = 4; // xhci-ext-caps.h:39
pub const EXT_CAP_ROUTE: u8 = 5; // xhci-ext-caps.h:40
pub const EXT_CAP_DEBUG: u8 = 10; // xhci-ext-caps.h:42
pub const EXT_CAP_VENDOR_INTEL: u8 = 192; // xhci-ext-caps.h:44
pub const EXT_CAP_INTEL_SPR_SHADOW: u8 = 206; // xhci-ext-caps.h:45

pub const HC_BIOS_OWNED: u32 = 1 << 16; // xhci-ext-caps.h:47
pub const HC_OS_OWNED: u32 = 1 << 24; // xhci-ext-caps.h:48
pub const LEGACY_SUPPORT_OFFSET: u32 = 0x00; // xhci-ext-caps.h:52
pub const LEGACY_CONTROL_OFFSET: u32 = 0x04; // xhci-ext-caps.h:56
pub const LEGACY_DISABLE_SMI: u32 = (0x7 << 1) + (0xff << 5) + (0x7 << 17); // xhci-ext-caps.h:58
pub const LEGACY_SMI_EVENTS: u32 = 0x7 << 29; // xhci-ext-caps.h:59

/// USB 2.0 xHCI 0.96 L1C protocol capability bit.
pub const L1C: u32 = 1 << 16; // xhci-ext-caps.h:62
/// USB 2.0 xHCI 1.0 hardware LMP protocol capability bit.
pub const HLC: u32 = 1 << 19; // xhci-ext-caps.h:65
/// USB 2.0 xHCI 1.0 best-effort service latency protocol capability bit.
pub const BLC: u32 = 1 << 20; // xhci-ext-caps.h:66

/// Extract xECP, the first extended-capability pointer in DWORD units, from HCCPARAMS1.
pub const fn hcc_ext_caps(hcc_params1: u32) -> u16 {
    ((hcc_params1 >> 16) & 0xffff) as u16 // xhci-ext-caps.h:19
}

/// Extract an extended capability's ID.
pub const fn ext_cap_id(header: u32) -> u8 {
    ((header >> 0) & 0xff) as u8 // xhci-ext-caps.h:32
}

/// Extract an extended capability's next-pointer in DWORD units. Zero terminates the list.
pub const fn ext_cap_next(header: u32) -> u8 {
    ((header >> 8) & 0xff) as u8 // xhci-ext-caps.h:33
}

/// Extract the capability-specific upper half of an extended-capability header.
pub const fn ext_cap_value(header: u32) -> u16 {
    (header >> 16) as u16 // xhci-ext-caps.h:34
}
