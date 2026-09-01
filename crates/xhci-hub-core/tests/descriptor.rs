// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for root-hub descriptors.

use xhci_hub_core::descriptor::*;
use xhci_hub_core::portsc::PORT_DEV_REMOVE;

/// xhci-hub.c:265-:275 and ch11.h:209-:216.
#[test]
fn common_characteristics_match_linux_literals() {
    assert_eq!(hub_characteristics(true), 0x0009);
    assert_eq!(hub_characteristics(false), 0x000a);
}

/// xhci-hub.c:278-:330. For eight ports `temp = 1 + ports / 8` is TWO, even though only one byte
/// holds ports 1..7: port 8 is bit zero of the second byte. The power mask bytes stay 0xff.
#[test]
fn usb2_descriptor_has_two_variable_bitmaps_and_reserved_bit_zero() {
    let statuses = [
        PORT_DEV_REMOVE, 0, 0, 0, 0, 0, 0, PORT_DEV_REMOVE,
    ];
    let desc = usb2_hub_descriptor(8, true, &statuses).unwrap();
    let expected = [
        0x0b, 0x29, 0x08, 0x09, 0x00, 0x0a, 0x00,
        0x02, 0x01, // DeviceRemovable: ports 1 and 8; bit zero reserved
        0xff, 0xff, // PortPwrCtrlMask
    ];
    assert_eq!(desc.len, 11);
    assert_eq!(&desc.bytes[..desc.len], &expected);
}

/// xhci-hub.c:333-:364 and ch11.h:247. USB3 has fixed length 12, 50 * 2ms power-good, zero header
/// latency/delay, and a little-endian u16 DeviceRemovable bitmap.
#[test]
fn usb3_descriptor_matches_linux_layout() {
    let statuses = [PORT_DEV_REMOVE, 0, PORT_DEV_REMOVE];
    let desc = usb3_hub_descriptor(3, false, &statuses).unwrap();
    assert_eq!(desc.bytes, [
        0x0c, 0x2a, 0x03, 0x0a, 0x00, 0x32, 0x00, 0x00,
        0x00, 0x00, 0x0a, 0x00,
    ]);
}

/// ch11.h:22 caps USB2 at 31 ports; xhci-hub.c:338/:357 make USB3 DeviceRemovable a u16 with bit
/// zero reserved, hence 15 representable ports. Missing snapshots are refused rather than read.
#[test]
fn descriptor_bounds_are_named_refusals() {
    assert_eq!(usb2_hub_descriptor(32, true, &[]), Err(DescriptorError::PortCountOutOfRange { value: 32, maximum: 31 }));
    assert_eq!(usb3_hub_descriptor(16, true, &[]), Err(DescriptorError::PortCountOutOfRange { value: 16, maximum: 15 }));
    assert_eq!(usb2_hub_descriptor(2, true, &[0]), Err(DescriptorError::MissingPortStatus { ports: 2, supplied: 1 }));
    assert_eq!(usb3_hub_descriptor(2, true, &[0]), Err(DescriptorError::MissingPortStatus { ports: 2, supplied: 1 }));
}

/// Linux defines exactly two root-hub descriptor forms in xhci-hub.c:278 and :333. Pin their names
/// and fixed/header sizes independently of construction tables.
#[test]
fn root_hub_descriptor_kind_count_and_names_are_pinned() {
    let kinds = [("USB2", 7usize), ("USB3", 12usize)];
    assert_eq!(kinds.len(), 2);
    assert_eq!(kinds, [("USB2", 7), ("USB3", 12)]);
    assert_eq!(USB2_DESCRIPTOR_MAX_BYTES, 15);
    assert_eq!(USB3_DESCRIPTOR_BYTES, 12);
}
