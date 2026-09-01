// SPDX-License-Identifier: GPL-2.0-only
//! Supported Protocol vectors from Linux `drivers/usb/host/xhci-ext-caps.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_extcap_core::protocol::*;

#[test]
fn the_fixed_protocol_layout_has_linuxs_three_named_fields() {
    assert_eq!(PROTOCOL_CAP_FIELDS.len(), 3); // xhci-ext-caps.h:95-99
    assert_eq!(
        PROTOCOL_CAP_FIELDS,
        ["revision", "name_string", "port_info"]
    ); // xhci-ext-caps.h:96-98
}

#[test]
fn a_real_usb_protocol_capability_decodes_every_fixed_field() {
    let protocol = SupportedProtocol::parse([0x0300_1002, 0x2042_5355, 0x3004_0405]);
    assert_eq!(protocol.revision, 0x0300_1002); // xhci-ext-caps.h:96
    assert_eq!(protocol.name_string, *b"USB "); // xhci-ext-caps.h:91-92,97
    assert_eq!(protocol.port_info, 0x3004_0405); // xhci-ext-caps.h:98
    assert_eq!(protocol.major(), 0x03); // xhci-ext-caps.h:101
    assert_eq!(protocol.minor(), 0x00); // xhci-ext-caps.h:102
    assert_eq!(protocol.speed_id_count(), 0x03); // xhci-ext-caps.h:103
    assert_eq!(protocol.compatible_port_offset(), 0x05); // xhci-ext-caps.h:104
    assert_eq!(protocol.compatible_port_count(), 0x04); // xhci-ext-caps.h:105
}

#[test]
fn compatible_ports_convert_from_one_based_at_both_ends() {
    let protocol = SupportedProtocol::parse([0x0300_0002, 0x2042_5355, 0x0000_0405]);
    let range = protocol.compatible_port_indices().unwrap();
    assert_eq!(range, PortIndexRange { start: 4, end: 8 }); // xhci-ext-caps.h:104-105

    let ports = ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8", "p9"];
    assert_eq!(
        ports[range.start], "p5",
        "one-based port 5 is zero-based index 4"
    );
    assert_eq!(
        ports[range.end - 1],
        "p8",
        "count 4 ends on one-based port 8"
    );
}

#[test]
fn zero_compatible_port_offset_is_a_named_refusal() {
    let protocol = SupportedProtocol::parse([0x0200_0002, 0x2042_5355, 0x0000_0400]);
    assert_eq!(
        protocol.compatible_port_indices(),
        Err(PortRangeRefusal::OffsetIsZero {
            offset: 0,
            minimum: 1
        })
    ); // xhci-ext-caps.h:104
}

#[test]
fn protocol_speed_id_count_selects_the_following_block() {
    let protocol = SupportedProtocol::parse([0x0300_0002, 0x2042_5355, 0x3000_0101]);
    assert_eq!(
        protocol.speed_id_block(&[0x000c_0001, 0x01e0_0042, 0x1388_c183, 0xdead_beef]),
        Ok(&[0x000c_0001, 0x01e0_0042, 0x1388_c183][..])
    ); // xhci-ext-caps.h:103
    assert_eq!(
        protocol.speed_id_block(&[0x000c_0001, 0x01e0_0042]),
        Err(SpeedIdBlockRefusal::TooShort {
            available: 2,
            required: 3
        })
    ); // xhci-ext-caps.h:103
}

#[test]
fn a_protocol_speed_id_decodes_all_six_linux_fields() {
    let speed = ProtocolSpeedId(0x1388_c1b5);
    assert_eq!(speed.value(), 0x5); // xhci-ext-caps.h:107
    assert_eq!(speed.exponent(), 0x3); // xhci-ext-caps.h:108
    assert_eq!(speed.protocol_type(), 0x2); // xhci-ext-caps.h:109
    assert!(speed.full_duplex()); // xhci-ext-caps.h:110
    assert_eq!(speed.link_protocol(), 0x3); // xhci-ext-caps.h:111
    assert_eq!(speed.mantissa(), 0x1388); // xhci-ext-caps.h:112
}

#[test]
fn zero_speed_fields_are_selected_not_inferred_from_other_fields() {
    let speed = ProtocolSpeedId(0xffff_3200);
    assert_eq!(speed.value(), 0); // xhci-ext-caps.h:107
    assert_eq!(speed.exponent(), 0); // xhci-ext-caps.h:108
    assert_eq!(speed.protocol_type(), 0); // xhci-ext-caps.h:109
    assert!(!speed.full_duplex()); // xhci-ext-caps.h:110
    assert_eq!(speed.link_protocol(), 0); // xhci-ext-caps.h:111
    assert_eq!(speed.mantissa(), 0xffff); // xhci-ext-caps.h:112
}
