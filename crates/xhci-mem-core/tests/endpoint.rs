// SPDX-License-Identifier: GPL-2.0-only
//! Endpoint vectors from Linux `drivers/usb/host/xhci.h` and `xhci-mem.c`.
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

use xhci_mem_core::endpoint::*;

/// xhci.h:453-463 and xhci-mem.c:1498-1499. Stream, Mult, and Interval fields occupy distinct
/// parts of ep_info and reject values wider than Linux's fields.
#[test]
fn ep_info_mult_interval_and_stream_fields_pack_independently() {
    assert_eq!(encode_mult(3), Ok(0x0000_0300));
    assert_eq!(decode_mult(0x00ab_ff00), 3);
    assert_eq!(encode_mult(4), Err(EndpointFieldError::MultOutOfRange { value: 4, maximum: 3 }));
    assert_eq!(encode_interval(0xab), 0x00ab_0000);
    assert_eq!(decode_interval(0x00ab_7c00), 0xab);
    assert_eq!(encode_max_primary_streams(31), Ok(0x0000_7c00));
    assert_eq!(decode_max_primary_streams(0x0000_fc00), 31);
    assert_eq!(encode_max_primary_streams(32), Err(EndpointFieldError::MaxPrimaryStreamsOutOfRange { value: 32, maximum: 31 }));
    assert!(has_linear_stream_array(0x0000_8000));
    assert!(!has_linear_stream_array(0x0000_7c00));
    assert_eq!(encode_mult(2).unwrap() | encode_interval(0xab) | encode_max_primary_streams(5).unwrap() | LINEAR_STREAM_ARRAY, 0x00ab_9600);
}

/// xhci.h:479-485. All seven nonzero type values and names are frozen literals.
#[test]
fn all_seven_endpoint_types_are_present_and_encoded_at_bit_three() {
    let expected = [
        ("ISOC_OUT", EndpointType::IsochOut, 1u32, 0x08u32),
        ("BULK_OUT", EndpointType::BulkOut, 2, 0x10),
        ("INT_OUT", EndpointType::InterruptOut, 3, 0x18),
        ("CONTROL", EndpointType::Control, 4, 0x20),
        ("ISOC_IN", EndpointType::IsochIn, 5, 0x28),
        ("BULK_IN", EndpointType::BulkIn, 6, 0x30),
        ("INT_IN", EndpointType::InterruptIn, 7, 0x38),
    ];
    assert_eq!(ENDPOINT_TYPE_NAMES.len(), 7);
    assert_eq!(ENDPOINT_TYPE_NAMES, ["ISOC_OUT", "BULK_OUT", "INT_OUT", "CONTROL", "ISOC_IN", "BULK_IN", "INT_IN"]);
    for (name, ty, raw, encoded) in expected {
        assert_eq!(ty as u32, raw, "{name}");
        assert_eq!(encode_endpoint_type(ty), encoded, "{name}");
        assert_eq!(decode_endpoint_type(encoded), Ok(ty), "{name}");
    }
    assert_eq!(decode_endpoint_type(0), Err(EndpointFieldError::UnknownEndpointType { value: 0, minimum: 1, maximum: 7 }));
}

/// xhci.h:443-449. Values 5-7 are reserved rather than silently reported as valid states.
#[test]
fn all_five_endpoint_states_are_named_and_unknown_states_refuse() {
    assert_eq!(ENDPOINT_STATE_NAMES.len(), 5);
    assert_eq!(ENDPOINT_STATE_NAMES, ["DISABLED", "RUNNING", "HALTED", "STOPPED", "ERROR"]);
    assert_eq!(decode_endpoint_state(0), Ok(EndpointState::Disabled));
    assert_eq!(decode_endpoint_state(1), Ok(EndpointState::Running));
    assert_eq!(decode_endpoint_state(2), Ok(EndpointState::Halted));
    assert_eq!(decode_endpoint_state(3), Ok(EndpointState::Stopped));
    assert_eq!(decode_endpoint_state(4), Ok(EndpointState::Error));
    assert_eq!(decode_endpoint_state(0xffff_ffff), Err(EndpointFieldError::UnknownEndpointState { value: 7, maximum_known: 4 }));
}

/// xhci.h:476,488-492 and xhci-mem.c:1500-1503. Distinct literals pin every ep_info2 field.
#[test]
fn max_packet_burst_error_count_and_type_pack_without_overlap() {
    assert_eq!(encode_max_packet(0x1234), 0x1234_0000);
    assert_eq!(decode_max_packet(0x1234_ab38), 0x1234);
    assert_eq!(encode_max_burst(0xab), 0x0000_ab00);
    assert_eq!(decode_max_burst(0x1234_ab38), 0xab);
    assert_eq!(encode_error_count(3), Ok(0x6));
    assert_eq!(decode_error_count(0x6), 3);
    assert_eq!(encode_error_count(4), Err(EndpointFieldError::ErrorCountOutOfRange { value: 4, maximum: 3 }));
    let word = encode_max_packet(0x1234) | encode_max_burst(0xab) | encode_endpoint_type(EndpointType::InterruptIn) | 0x6;
    assert_eq!(word, 0x1234_ab3e);
}

/// xhci.h:495-498 and xhci-mem.c:1497,1507-1508. Max ESIT is split 8 high + 16 low.
#[test]
fn max_esit_payload_round_trips_across_two_dwords() {
    assert_eq!(encode_max_esit_payload_high(0x12_3456), Ok(0x1200_0000));
    assert_eq!(encode_tx_info(0xabcd, 0x12_3456), Ok(0x3456_abcd));
    assert_eq!(decode_max_esit_payload(0x1200_0000, 0x3456_abcd), 0x12_3456);
    assert_eq!(decode_avg_trb_length(0x3456_abcd), 0xabcd);
    assert_eq!(encode_max_esit_payload_high(0x100_0000), Err(EndpointFieldError::MaxEsitPayloadOutOfRange { value: 0x100_0000, maximum: 0xff_ffff }));
    assert_eq!(encode_tx_info(8, 0x100_0000), Err(EndpointFieldError::MaxEsitPayloadOutOfRange { value: 0x100_0000, maximum: 0xff_ffff }));
}

/// xhci.h:443,453,456,459,463,468,476-478,488-497.
#[test]
fn endpoint_field_masks_match_linux_literals() {
    assert_eq!(EP_STATE_MASK, 0x0000_0007);
    assert_eq!(EP_MULT_MASK, 0x0000_0300);
    assert_eq!(MAX_PRIMARY_STREAMS_MASK, 0x0000_7c00);
    assert_eq!(LINEAR_STREAM_ARRAY, 0x0000_8000);
    assert_eq!(EP_INTERVAL_MASK, 0x00ff_0000);
    assert_eq!(MAX_ESIT_PAYLOAD_HIGH_MASK, 0xff00_0000);
    assert_eq!(ERROR_COUNT_MASK, 0x0000_0006);
    assert_eq!(ENDPOINT_TYPE_MASK, 0x0000_0038);
    assert_eq!(MAX_BURST_MASK, 0x0000_ff00);
    assert_eq!(MAX_PACKET_MASK, 0xffff_0000);
    assert_eq!(AVG_TRB_LENGTH_MASK, 0x0000_ffff);
    assert_eq!(MAX_ESIT_PAYLOAD_LOW_MASK, 0xffff_0000);
}
