// SPDX-License-Identifier: GPL-2.0-only
//! Endpoint-context field packing and endpoint types, ported from Linux
//! `drivers/usb/host/xhci.h` and `drivers/usb/host/xhci-mem.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

pub const EP_STATE_MASK: u32 = 0x7; // xhci.h:443
pub const EP_MULT_MASK: u32 = 0x3 << 8; // xhci.h:453
pub const EP_INTERVAL_MASK: u32 = 0xff << 16; // xhci.h:456
pub const MAX_PRIMARY_STREAMS_MASK: u32 = 0x1f << 10; // xhci.h:459
pub const LINEAR_STREAM_ARRAY: u32 = 1 << 15; // xhci.h:463
pub const MAX_ESIT_PAYLOAD_HIGH_MASK: u32 = 0xff << 24; // xhci.h:468
pub const ERROR_COUNT_MASK: u32 = 0x3 << 1; // xhci.h:476
pub const ENDPOINT_TYPE_MASK: u32 = 0x7 << 3; // xhci.h:477-478
pub const MAX_BURST_MASK: u32 = 0xff << 8; // xhci.h:488-489
pub const MAX_PACKET_MASK: u32 = 0xffff << 16; // xhci.h:490-491
pub const AVG_TRB_LENGTH_MASK: u32 = 0xffff; // xhci.h:495
pub const MAX_ESIT_PAYLOAD_LOW_MASK: u32 = 0xffff << 16; // xhci.h:496

/// Linux names for every endpoint-state encoding it defines.
pub const ENDPOINT_STATE_NAMES: [&str; 5] = ["DISABLED", "RUNNING", "HALTED", "STOPPED", "ERROR"]; // xhci.h:444-448
/// Linux names for every endpoint-type encoding it defines.
pub const ENDPOINT_TYPE_NAMES: [&str; 7] = [
    "ISOC_OUT", "BULK_OUT", "INT_OUT", "CONTROL", "ISOC_IN", "BULK_IN", "INT_IN",
]; // xhci.h:479-485

/// Endpoint context state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EndpointState {
    Disabled = 0, // xhci.h:444
    Running = 1, // xhci.h:445
    Halted = 2, // xhci.h:446
    Stopped = 3, // xhci.h:447
    Error = 4, // xhci.h:448
}

/// Endpoint type field encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum EndpointType {
    IsochOut = 1, // xhci.h:479
    BulkOut = 2, // xhci.h:480
    InterruptOut = 3, // xhci.h:481
    Control = 4, // xhci.h:482
    IsochIn = 5, // xhci.h:483
    BulkIn = 6, // xhci.h:484
    InterruptIn = 7, // xhci.h:485
}

/// Encode Mult into `ep_info`.
pub const fn encode_mult(value: u8) -> Result<u32, EndpointFieldError> {
    if value > 3 {
        return Err(EndpointFieldError::MultOutOfRange { value, maximum: 3 });
    }
    Ok((value as u32) << 8) // xhci.h:453; xhci-mem.c:1499
}

/// Decode Mult from `ep_info`.
pub const fn decode_mult(ep_info: u32) -> u8 {
    ((ep_info >> 8) & 0x3) as u8 // xhci.h:454; xhci-mem.c:1583-1584
}

/// Encode Interval into `ep_info`.
pub const fn encode_interval(value: u8) -> u32 {
    (value as u32) << 16 // xhci.h:456; xhci-mem.c:1498
}

/// Decode Interval from `ep_info`.
pub const fn decode_interval(ep_info: u32) -> u8 {
    ((ep_info >> 16) & 0xff) as u8 // xhci.h:458; xhci-mem.c:1580-1581
}

/// Encode Max Primary Streams into `ep_info`.
pub const fn encode_max_primary_streams(value: u8) -> Result<u32, EndpointFieldError> {
    if value > 31 {
        return Err(EndpointFieldError::MaxPrimaryStreamsOutOfRange { value, maximum: 31 });
    }
    Ok((value as u32) << 10) // xhci.h:459-460
}

/// Decode Max Primary Streams from `ep_info`.
pub const fn decode_max_primary_streams(ep_info: u32) -> u8 {
    ((ep_info & MAX_PRIMARY_STREAMS_MASK) >> 10) as u8 // xhci.h:461
}

/// Whether `ep_info` selects a Linear Stream Array.
pub const fn has_linear_stream_array(ep_info: u32) -> bool {
    ep_info & LINEAR_STREAM_ARRAY != 0 // xhci.h:462-463
}

/// Encode endpoint type into `ep_info2`.
pub const fn encode_endpoint_type(endpoint_type: EndpointType) -> u32 {
    (endpoint_type as u32) << 3 // xhci.h:478; xhci-mem.c:1500
}

/// Decode endpoint type from `ep_info2`.
pub const fn decode_endpoint_type(ep_info2: u32) -> Result<EndpointType, EndpointFieldError> {
    let value = ((ep_info2 >> 3) & 0x7) as u8; // xhci.h:477
    match value {
        1 => Ok(EndpointType::IsochOut),
        2 => Ok(EndpointType::BulkOut),
        3 => Ok(EndpointType::InterruptOut),
        4 => Ok(EndpointType::Control),
        5 => Ok(EndpointType::IsochIn),
        6 => Ok(EndpointType::BulkIn),
        7 => Ok(EndpointType::InterruptIn),
        _ => Err(EndpointFieldError::UnknownEndpointType { value, minimum: 1, maximum: 7 }),
    }
}

/// Decode endpoint state from `ep_info`.
pub const fn decode_endpoint_state(ep_info: u32) -> Result<EndpointState, EndpointFieldError> {
    let value = (ep_info & EP_STATE_MASK) as u8; // xhci.h:443-449
    match value {
        0 => Ok(EndpointState::Disabled),
        1 => Ok(EndpointState::Running),
        2 => Ok(EndpointState::Halted),
        3 => Ok(EndpointState::Stopped),
        4 => Ok(EndpointState::Error),
        _ => Err(EndpointFieldError::UnknownEndpointState { value, maximum_known: 4 }),
    }
}

/// Encode maximum packet size into `ep_info2`.
pub const fn encode_max_packet(value: u16) -> u32 {
    (value as u32) << 16 // xhci.h:490; xhci-mem.c:1501
}

/// Decode maximum packet size from `ep_info2`.
pub const fn decode_max_packet(ep_info2: u32) -> u16 {
    ((ep_info2 >> 16) & 0xffff) as u16 // xhci.h:492; xhci-mem.c:1587-1588
}

/// Encode zero-based maximum burst size into `ep_info2`.
pub const fn encode_max_burst(value: u8) -> u32 {
    (value as u32) << 8 // xhci.h:488; xhci-mem.c:1502
}

/// Decode zero-based maximum burst size from `ep_info2`.
pub const fn decode_max_burst(ep_info2: u32) -> u8 {
    ((ep_info2 >> 8) & 0xff) as u8 // xhci.h:489; xhci-mem.c:1585-1586
}

/// Encode the two-bit error count into `ep_info2`.
pub const fn encode_error_count(value: u8) -> Result<u32, EndpointFieldError> {
    if value > 3 {
        return Err(EndpointFieldError::ErrorCountOutOfRange { value, maximum: 3 });
    }
    Ok((value as u32) << 1) // xhci.h:476; xhci-mem.c:1503
}

/// Decode the error count from `ep_info2`.
pub const fn decode_error_count(ep_info2: u32) -> u8 {
    ((ep_info2 >> 1) & 0x3) as u8 // xhci.h:476
}

/// Pack the average TRB length and low 16 bits of Max ESIT Payload into `tx_info`.
pub const fn encode_tx_info(avg_trb_length: u16, max_esit_payload: u32) -> Result<u32, EndpointFieldError> {
    if max_esit_payload > 0x00ff_ffff {
        return Err(EndpointFieldError::MaxEsitPayloadOutOfRange { value: max_esit_payload, maximum: 0x00ff_ffff });
    }
    Ok((avg_trb_length as u32) | ((max_esit_payload & 0xffff) << 16)) // xhci.h:495-496; xhci-mem.c:1507-1508
}

/// Pack the high eight Max ESIT Payload bits into `ep_info`.
pub const fn encode_max_esit_payload_high(max_esit_payload: u32) -> Result<u32, EndpointFieldError> {
    if max_esit_payload > 0x00ff_ffff {
        return Err(EndpointFieldError::MaxEsitPayloadOutOfRange { value: max_esit_payload, maximum: 0x00ff_ffff });
    }
    Ok(((max_esit_payload >> 16) & 0xff) << 24) // xhci.h:497; xhci-mem.c:1497
}

/// Decode the complete 24-bit Max ESIT Payload from `ep_info` and `tx_info`.
pub const fn decode_max_esit_payload(ep_info: u32, tx_info: u32) -> u32 {
    (((ep_info >> 24) & 0xff) << 16) | ((tx_info >> 16) & 0xffff) // xhci.h:468,498; xhci-mem.c:1590-1591
}

/// Decode average TRB length from `tx_info`.
pub const fn decode_avg_trb_length(tx_info: u32) -> u16 {
    (tx_info & 0xffff) as u16 // xhci.h:495
}

/// Endpoint-field refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointFieldError {
    MultOutOfRange { value: u8, maximum: u8 },
    MaxPrimaryStreamsOutOfRange { value: u8, maximum: u8 },
    UnknownEndpointType { value: u8, minimum: u8, maximum: u8 },
    UnknownEndpointState { value: u8, maximum_known: u8 },
    ErrorCountOutOfRange { value: u8, maximum: u8 },
    MaxEsitPayloadOutOfRange { value: u32, maximum: u32 },
}
