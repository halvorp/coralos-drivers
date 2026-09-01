// SPDX-License-Identifier: GPL-2.0-only
//! Frozen transfer-type and speed names from Linux `include/uapi/linux/usb/ch9.h:441-444` and
//! `include/uapi/linux/usb/ch9.h:1201-1208`.
//!
//! Copyright (C) the Linux USB Chapter 9 header authors.

use usb_urb_valid_core::{Speed, TransferType, TRANSFER_TYPES, WIRED_SPEEDS};

#[test]
fn all_four_transfer_types_are_pinned_by_count_name_value_and_variant() {
    let expected_names = ["CONTROL", "ISOC", "BULK", "INT"];
    let expected_values = [0, 1, 2, 3]; // include/uapi/linux/usb/ch9.h:441-444
    let expected_types = [
        TransferType::Control,
        TransferType::Isochronous,
        TransferType::Bulk,
        TransferType::Interrupt,
    ];
    assert_eq!(TRANSFER_TYPES.len(), 4); // include/uapi/linux/usb/ch9.h:441-444
    assert_eq!(
        TRANSFER_TYPES
            .iter()
            .map(|entry| entry.0)
            .collect::<Vec<_>>(),
        expected_names
    );
    assert_eq!(
        TRANSFER_TYPES
            .iter()
            .map(|entry| entry.1)
            .collect::<Vec<_>>(),
        expected_values
    );
    assert_eq!(
        TRANSFER_TYPES
            .iter()
            .map(|entry| entry.2)
            .collect::<Vec<_>>(),
        expected_types
    );
}

#[test]
fn all_five_wired_speeds_used_by_urb_validation_are_pinned() {
    let expected_names = ["LOW", "FULL", "HIGH", "SUPER", "SUPER_PLUS"];
    let expected_speeds = [
        Speed::Low,
        Speed::Full,
        Speed::High,
        Speed::Super,
        Speed::SuperPlus,
    ];
    assert_eq!(WIRED_SPEEDS.len(), 5); // include/uapi/linux/usb/ch9.h:1203-1207
    assert_eq!(
        WIRED_SPEEDS.iter().map(|entry| entry.0).collect::<Vec<_>>(),
        expected_names
    );
    assert_eq!(
        WIRED_SPEEDS.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        expected_speeds
    );
}
