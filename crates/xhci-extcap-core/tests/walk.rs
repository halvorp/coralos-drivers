// SPDX-License-Identifier: GPL-2.0-only
//! Capability-walk vectors from Linux `drivers/usb/host/xhci-ext-caps.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_extcap_core::caps::{EXT_CAP_PROTOCOL, HCC_PARAMS_OFFSET};
use xhci_extcap_core::walk::*;

const LIST: [CapabilityWord; 3] = [
    CapabilityWord {
        offset: 0x40,
        header: 0x0000_0401,
    },
    CapabilityWord {
        offset: 0x50,
        header: 0x0300_0802,
    },
    CapabilityWord {
        offset: 0x70,
        header: 0x0200_0002,
    },
];

const LIST_WITH_HCCPARAMS_CAP: [CapabilityWord; 3] = [
    CapabilityWord {
        offset: 0x10,
        header: 0x0000_0401,
    },
    CapabilityWord {
        offset: 0x20,
        header: 0x0300_0802,
    },
    CapabilityWord {
        offset: 0x40,
        header: 0x0200_0002,
    },
];

#[test]
fn hccparams_dword_pointer_becomes_a_byte_offset() {
    assert_eq!(first_capability_offset(0x0010_0000), Ok(0x40)); // xhci-ext-caps.h:141
}

#[test]
fn a_next_pointer_is_a_dword_step_not_a_byte_step() {
    assert_eq!(next_capability_offset(0x40, 0x0000_0401), Ok(0x50)); // xhci-ext-caps.h:152-153
    assert_eq!(next_capability_offset(0x50, 0x0300_0802), Ok(0x70)); // xhci-ext-caps.h:152-153
}

#[test]
fn a_three_capability_list_is_walked_to_find_repeated_protocols() {
    assert_eq!(
        find_next_ext_cap(0x0010_0000, &LIST, 0, CapabilityMatch::Id(EXT_CAP_PROTOCOL)),
        Ok(0x50)
    ); // xhci-ext-caps.h:137-154
    assert_eq!(
        find_next_ext_cap(
            0x0010_0000,
            &LIST,
            0x50,
            CapabilityMatch::Id(EXT_CAP_PROTOCOL)
        ),
        Ok(0x70)
    ); // xhci-ext-caps.h:125-127,145-154
    assert_eq!(
        find_next_ext_cap(
            0x0004_0000,
            &LIST_WITH_HCCPARAMS_CAP,
            HCC_PARAMS_OFFSET,
            CapabilityMatch::Id(EXT_CAP_PROTOCOL)
        ),
        Ok(0x20)
    ); // xhci-ext-caps.h:120-121,137-153
}

#[test]
fn any_match_means_the_next_capability_after_start() {
    assert_eq!(
        find_next_ext_cap(0x0010_0000, &LIST, 0x40, CapabilityMatch::Any),
        Ok(0x50)
    ); // xhci-ext-caps.h:122-123,149-153
}

#[test]
fn zero_next_pointer_is_a_named_end_of_list_not_a_zero_step() {
    assert_eq!(
        next_capability_offset(0x70, 0x0200_0002),
        Err(WalkRefusal::NextPointerIsZeroEndOfList { offset: 0x70 })
    ); // xhci-ext-caps.h:152-156
    assert_eq!(
        find_next_ext_cap(0x0010_0000, &LIST, 0x70, CapabilityMatch::Any),
        Err(WalkRefusal::NextPointerIsZeroEndOfList { offset: 0x70 })
    ); // xhci-ext-caps.h:152-156
}

#[test]
fn absent_and_inaccessible_hccparams_are_named_refusals() {
    assert_eq!(
        first_capability_offset(0x0000_1234),
        Err(WalkRefusal::NoFirstCapability {
            hcc_params1: 0x0000_1234
        })
    ); // xhci-ext-caps.h:141-143
    assert_eq!(
        first_capability_offset(0xffff_ffff),
        Err(WalkRefusal::InaccessibleRegister { offset: 0x10 })
    ); // xhci-ext-caps.h:138-140
}

#[test]
fn inaccessible_or_missing_capability_reads_are_named() {
    let inaccessible = [CapabilityWord {
        offset: 0x40,
        header: 0xffff_ffff,
    }];
    assert_eq!(
        find_next_ext_cap(0x0010_0000, &inaccessible, 0, CapabilityMatch::Any),
        Err(WalkRefusal::InaccessibleRegister { offset: 0x40 })
    ); // xhci-ext-caps.h:146-148
    assert_eq!(
        find_next_ext_cap(0x0010_0000, &[], 0, CapabilityMatch::Any),
        Err(WalkRefusal::HeaderNotProvided { offset: 0x40 })
    );
}

#[test]
fn offset_overflow_is_refused_with_the_value_and_step() {
    assert_eq!(
        next_capability_offset(0xffff_fffc, 0x0000_0201),
        Err(WalkRefusal::OffsetOverflow {
            offset: 0xffff_fffc,
            next_dwords: 2
        })
    ); // xhci-ext-caps.h:152-153
}

#[test]
fn the_linux_fifty_capability_limit_is_named() {
    let mut words = [CapabilityWord {
        offset: 0,
        header: 0,
    }; 50];
    for (index, word) in words.iter_mut().enumerate() {
        word.offset = 0x40 + (index as u32) * 4;
        word.header = 0x0000_0101;
    }
    assert_eq!(
        find_next_ext_cap(
            0x0010_0000,
            &words,
            0,
            CapabilityMatch::Id(EXT_CAP_PROTOCOL)
        ),
        Err(WalkRefusal::MaximumCapabilitiesExceeded { maximum: 50 })
    ); // xhci-ext-caps.h:25
}
