// SPDX-License-Identifier: GPL-2.0-only
//! Literal endpoint-field decode vectors from Linux `include/uapi/linux/usb/ch9.h`.
//!
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! Chapter 9 header authors and contributors.

use usb_config_parse_core::decode::{
    decode_address, decode_attributes, decode_max_packet_size, Direction, SyncType, TransferType,
    UsageType, ENDPOINT_DIRECTION_MASK, ENDPOINT_NUMBER_MASK, MAX_PACKET_MASK,
    MAX_PACKET_MULT_MASK, MAX_PACKET_MULT_SHIFT, SYNC_TYPES, SYNC_TYPE_MASK, TRANSFER_TYPES,
    TRANSFER_TYPE_MASK, USAGE_TYPES, USAGE_TYPE_MASK,
};

#[test]
fn masks_are_linux_literals() {
    assert_eq!(ENDPOINT_NUMBER_MASK, 0x0f); // ch9.h:437
    assert_eq!(ENDPOINT_DIRECTION_MASK, 0x80); // ch9.h:438
    assert_eq!(TRANSFER_TYPE_MASK, 0x03); // ch9.h:440
    assert_eq!(MAX_PACKET_MASK, 0x07ff); // ch9.h:447
    assert_eq!(MAX_PACKET_MULT_SHIFT, 11); // ch9.h:448
    assert_eq!(MAX_PACKET_MULT_MASK, 3 << 11); // ch9.h:449
    assert_eq!(SYNC_TYPE_MASK, 0x0c); // ch9.h:458
    assert_eq!(USAGE_TYPE_MASK, 0x30); // ch9.h:464
}

#[test]
fn address_decode_covers_both_directions_and_reserved_bits() {
    assert_eq!(decode_address(0x03).number, 3);
    assert_eq!(decode_address(0x03).direction, Direction::Out); // USB_DIR_OUT, ch9.h:45
    assert_eq!(decode_address(0x8f).number, 15);
    assert_eq!(decode_address(0x8f).direction, Direction::In); // USB_DIR_IN, ch9.h:46
    assert_eq!(decode_address(0xf2).number, 2, "bits 6:4 are reserved");
}

/// Linux defines exactly four transfer encodings (`ch9.h:441-444`), pinned by count and name.
#[test]
fn all_four_transfer_types_are_decoded_by_name() {
    let got = [0u8, 1, 2, 3].map(|v| decode_attributes(v).transfer_type);
    let linux = [
        TransferType::Control,
        TransferType::Isochronous,
        TransferType::Bulk,
        TransferType::Interrupt,
    ];
    let linux_names = ["CONTROL", "ISOC", "BULK", "INT"];
    let linux_values = [0, 1, 2, 3];
    assert_eq!(TRANSFER_TYPES.len(), 4);
    assert_eq!(
        TRANSFER_TYPES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        linux_names
    );
    assert_eq!(
        TRANSFER_TYPES.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
        linux_values
    );
    assert_eq!(got, linux);
}

/// Linux defines exactly four synchronization encodings (`ch9.h:458-462`).
#[test]
fn all_four_sync_types_are_decoded_by_name() {
    let got = [0x00u8, 0x04, 0x08, 0x0c].map(|v| decode_attributes(v).sync_type);
    let linux = [
        SyncType::None,
        SyncType::Asynchronous,
        SyncType::Adaptive,
        SyncType::Synchronous,
    ];
    let linux_names = ["NONE", "ASYNC", "ADAPTIVE", "SYNC"];
    let linux_values = [0x00, 0x04, 0x08, 0x0c];
    assert_eq!(SYNC_TYPES.len(), 4);
    assert_eq!(
        SYNC_TYPES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        linux_names
    );
    assert_eq!(
        SYNC_TYPES.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
        linux_values
    );
    assert_eq!(got, linux);
}

/// Linux names three usage values and leaves `0x30` reserved (`ch9.h:464-467`).
#[test]
fn all_four_usage_bit_patterns_are_decoded_by_name() {
    let got = [0x00u8, 0x10, 0x20, 0x30].map(|v| decode_attributes(v).usage_type);
    let linux = [
        UsageType::Data,
        UsageType::Feedback,
        UsageType::ImplicitFeedback,
        UsageType::Reserved,
    ];
    let linux_names = ["DATA", "FEEDBACK", "IMPLICIT_FB"];
    let linux_values = [0x00, 0x10, 0x20];
    assert_eq!(USAGE_TYPES.len(), 3);
    assert_eq!(
        USAGE_TYPES.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
        linux_names
    );
    assert_eq!(
        USAGE_TYPES.iter().map(|(_, v)| *v).collect::<Vec<_>>(),
        linux_values
    );
    assert_eq!(got, linux);
}

#[test]
fn attributes_fields_do_not_overlap() {
    let d = decode_attributes(0x2d); // implicit feedback + synchronous + isochronous
    assert_eq!(d.transfer_type, TransferType::Isochronous);
    assert_eq!(d.sync_type, SyncType::Synchronous);
    assert_eq!(d.usage_type, UsageType::ImplicitFeedback);
}

/// `usb_endpoint_maxp[_mult]`, `ch9.h:650-671`: bits 10:0 and encoded bits 12:11 + 1.
#[test]
fn max_packet_decodes_all_four_high_bandwidth_multiplier_patterns() {
    let vectors = [
        (0x0400, 1024, 1),
        (0x0c00, 1024, 2),
        (0x1400, 1024, 3),
        (0x1c00, 1024, 4),
    ];
    for (raw, bytes, transactions) in vectors {
        let got = decode_max_packet_size(raw);
        assert_eq!(got.bytes, bytes);
        assert_eq!(got.transactions, transactions);
    }
    assert_eq!(decode_max_packet_size(0xffff).bytes, 0x07ff);
    assert_eq!(decode_max_packet_size(0xffff).transactions, 4);
}
