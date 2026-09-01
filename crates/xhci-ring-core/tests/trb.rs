// SPDX-License-Identifier: GPL-2.0-only
//! Literal vectors for TRB fields ported from Linux `drivers/usb/host/xhci.h` and construction in
//! `drivers/usb/host/xhci-ring.c`. Copyright (C) 2008 Intel Corp., Sarah Sharp, and Linux authors.

use xhci_ring_core::trb::*;

/// xhci.h:1100-:1164. These names and values are written independently of `TRB_TYPES`: deriving
/// this expectation from the production table would let an accidentally deleted row delete its test.
#[test]
fn all_thirty_three_distinct_linux_trb_types_are_present_and_named() {
    let expected = [
        (1, "Normal"),
        (2, "Setup Stage"),
        (3, "Data Stage"),
        (4, "Status Stage"),
        (5, "Isoch"),
        (6, "Link"),
        (7, "Event Data"),
        (8, "No-Op"),
        (9, "Enable Slot Command"),
        (10, "Disable Slot Command"),
        (11, "Address Device Command"),
        (12, "Configure Endpoint Command"),
        (13, "Evaluate Context Command"),
        (14, "Reset Endpoint Command"),
        (15, "Stop Ring Command"),
        (16, "Set TR Dequeue Pointer Command"),
        (17, "Reset Device Command"),
        (18, "Force Event Command"),
        (19, "Negotiate Bandwidth Command"),
        (20, "Set Latency Tolerance Value Command"),
        (21, "Get Port Bandwidth Command"),
        (22, "Force Header Command"),
        (23, "No-Op Command"),
        (32, "Transfer Event"),
        (33, "Command Completion Event"),
        (34, "Port Status Change Event"),
        (35, "Bandwidth Request Event"),
        (36, "Doorbell Event"),
        (37, "Host Controller Event"),
        (38, "Device Notification Event"),
        (39, "MFINDEX Wrap Event"),
        (48, "NEC Command Completion Event"),
        (49, "NET Get Firmware Revision Command"),
    ];
    assert_eq!(TRB_TYPES.len(), 33);
    assert_eq!(TRB_TYPES, expected.as_slice());
}

/// xhci.h:1095-:1097, :1110, :1244. Type occupies bits 15:10; unrelated flags must not affect it.
#[test]
fn type_encode_decode_and_link_identification_use_the_literal_field() {
    assert_eq!(TYPE_MASK, 0xfc00);
    assert_eq!(encode_type(6), 0x1800, "TRB_TYPE(TRB_LINK), xhci.h:1096,:1110");
    assert_eq!(decode_type(0xa500_1831), 6);
    assert!(is_link(0xa500_1831));
    assert!(!is_link(0x2000), "0x2000 is Transfer No-op, type 8");
}

/// xhci.h:1030-:1038. Literal masks distinguish three adjacent fields in status word 2. The
/// all-fields-maximum word is a valid encoding and makes every bit of every mask observable.
#[test]
fn transfer_status_fields_encode_and_decode_without_overlap() {
    assert_eq!(encode_transfer_length(0x3ffff), 0x1ffff);
    assert_eq!(decode_transfer_length(0xffff_ffff), 0x1ffff);
    assert_eq!(encode_td_size(7), 0x000e_0000);
    assert_eq!(decode_td_size(0x000e_0000), 7);
    assert_eq!(encode_td_size(32), 0x003e_0000, "min(32, 31) << 17, xhci.h:1032");
    assert_eq!(encode_interrupter_target(0x155), 0x5540_0000);
    assert_eq!(decode_interrupter_target(0x5540_0000), 0x155);
    assert_eq!(encode_interrupter_target(0x7ff), 0xffc0_0000, "0x3ff mask, xhci.h:1037");

    let maximum_status = encode_transfer_length(0x1ffff)
        | encode_td_size(31)
        | encode_interrupter_target(0x3ff);
    assert_eq!(maximum_status, 0xffff_ffff, "xhci.h:1030-:1038");
    assert_eq!(maximum_status & TRANSFER_LENGTH_MASK, 0x0001_ffff, "xhci.h:1030");
    assert_eq!(maximum_status & TD_SIZE_MASK, 0x003e_0000, "xhci.h:1033");
    assert_eq!(maximum_status & INTERRUPTER_TARGET_MASK, 0xffc0_0000, "xhci.h:1037");
}

/// xhci.h:827. Event residual length is a 24-bit field, unlike a transfer TRB's 17-bit length.
#[test]
fn event_transfer_length_keeps_exactly_twenty_four_bits() {
    assert_eq!(EVENT_TRANSFER_LENGTH_MASK, 0x00ff_ffff, "xhci.h:827");
    assert_eq!(event_transfer_length(0xab12_3456), 0x0012_3456);
}

/// xhci.h:817-:824. Slot is bits 31:24 and endpoint ID is bits 20:16; Linux APIs expose a
/// zero-based endpoint index, hence the encode/decode +/- one.
#[test]
fn slot_and_endpoint_fields_round_trip_linux_literals() {
    assert_eq!(SLOT_ID_MASK, 0xff00_0000, "xhci.h:817-:818");
    assert_eq!(ENDPOINT_ID_MASK, 0x001f_0000, "xhci.h:820");
    let command = encode_slot_id(0xa5) | encode_type(TrbType::DisableSlot as u8);
    assert_eq!(command, 0xa500_2800, "xhci-ring.c:775, :4397-:4400");
    assert_eq!(command & SLOT_ID_MASK, 0xa500_0000, "TRB_TO_SLOT_ID, xhci.h:817");
    assert_eq!(decode_slot_id(command), 0xa5);
    assert_eq!(encode_endpoint_index(0), 0x0001_0000);
    assert_eq!(encode_endpoint_index(30), 0x001f_0000);
    assert_eq!(decode_endpoint_index(0x001f_0000), Ok(30));
    assert_eq!(
        decode_endpoint_index(0),
        Err(FieldError::EndpointIdZero),
        "endpoint ID zero refused: Linux states valid IDs are 1 through 31 at xhci.h:820"
    );
}

/// xhci-ring.c:37-:40, :135-:139, :4389-:4390. Producer writes PCS into bit zero; consumer owns
/// exactly those TRBs whose cycle bit equals CCS.
#[test]
fn producer_cycle_write_and_consumer_ownership_are_exact_opposites_after_flip() {
    assert_eq!(with_cycle(0x1820, false), 0x1820);
    assert_eq!(with_cycle(0x1820, true), 0x1821);
    assert!(consumer_owns(0x1820, false));
    assert!(!consumer_owns(0x1820, true));
    assert!(!consumer_owns(0x1821, false));
    assert!(consumer_owns(0x1821, true));
}

/// xhci-ring.c:3239-:3256. Construction retains all four literal words in Linux's order.
#[test]
fn generic_trb_words_keep_linux_field_order() {
    assert_eq!(
        words(0x0123_4567, 0x89ab_cdef, 0x1357_9bdf, 0x2468_ace0).words,
        [0x0123_4567, 0x89ab_cdef, 0x1357_9bdf, 0x2468_ace0]
    );
}

/// xhci.h:1046-:1061. Each public transfer-control flag is encoded into a valid TRB control word
/// on its own. Separate words are intentional: if one mask is accidentally changed to another
/// already-present flag, an aggregate-OR-only vector could stay green.
#[test]
fn every_transfer_control_flag_encodes_its_linux_bit() {
    assert_eq!(encode_type(TrbType::Normal as u8) | ENT, 0x0402, "xhci.h:1046");
    assert_eq!(encode_type(TrbType::Normal as u8) | ISP, 0x0404, "xhci.h:1048");
    assert_eq!(encode_type(TrbType::Normal as u8) | NO_SNOOP, 0x0408, "xhci.h:1050");
    assert_eq!(encode_type(TrbType::Normal as u8) | CHAIN, 0x0410, "xhci.h:1052");
    assert_eq!(encode_type(TrbType::Normal as u8) | IOC, 0x0420, "xhci.h:1054");
    assert_eq!(encode_type(TrbType::Normal as u8) | IDT, 0x0440, "xhci.h:1056");
    assert_eq!(encode_type(TrbType::Isoch as u8) | BEI, 0x1600, "xhci-ring.c:4183-:4187");
    assert_eq!(encode_type(TrbType::Data as u8) | DIR_IN, 0x0001_0c00, "xhci.h:1064");
}

/// xhci.h:957, :1041, :1259-:1262. Geometry and non-transfer control bits are literal hardware
/// values. Transfer-control bits are pinned through complete encoded words above.
#[test]
fn geometry_and_control_bit_literals_are_pinned() {
    assert_eq!(TRB_BYTES, 16);
    assert_eq!(TRBS_PER_SEGMENT, 256);
    assert_eq!(USABLE_TRBS_PER_SEGMENT, 255);
    assert_eq!(TRB_SEGMENT_SIZE, 4096);
    assert_eq!(CYCLE, 0x01);
    assert_eq!(LINK_TOGGLE, 0x02);
}
