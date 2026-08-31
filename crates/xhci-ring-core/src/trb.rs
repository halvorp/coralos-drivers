// SPDX-License-Identifier: GPL-2.0-only
//! TRB words and field codecs, ported from Linux `drivers/usb/host/xhci.h:814-:1164` and
//! `drivers/usb/host/xhci-ring.c:3239-:3256`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

/// One xHCI TRB in the same four-word shape used by `struct xhci_generic_trb`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Trb {
    pub words: [u32; 4],
}

pub const TRB_BYTES: usize = 16; // xhci.h:1262, `TRBS_PER_SEGMENT*16`
pub const TRBS_PER_SEGMENT: usize = 256; // xhci.h:1259
pub const USABLE_TRBS_PER_SEGMENT: usize = 255; // xhci-ring.c:338-:359, excludes one Link TRB
pub const TRB_SEGMENT_SIZE: usize = 4096; // xhci.h:1262, `256*16`

pub const CYCLE: u32 = 1 << 0; // xhci.h:1041
pub const LINK_TOGGLE: u32 = 1 << 1; // xhci.h:957
pub const ENT: u32 = 1 << 1; // xhci.h:1046
pub const ISP: u32 = 1 << 2; // xhci.h:1048
pub const NO_SNOOP: u32 = 1 << 3; // xhci.h:1050
pub const CHAIN: u32 = 1 << 4; // xhci.h:1052
pub const IOC: u32 = 1 << 5; // xhci.h:1054
pub const IDT: u32 = 1 << 6; // xhci.h:1056
pub const BEI: u32 = 1 << 9; // xhci.h:1061
pub const DIR_IN: u32 = 1 << 16; // xhci.h:1064

pub const TYPE_MASK: u32 = 0xfc00; // xhci.h:1095
pub const TRANSFER_LENGTH_MASK: u32 = 0x1ffff; // xhci.h:1030
pub const TD_SIZE_MASK: u32 = 0x3e0000; // xhci.h:1033
pub const INTERRUPTER_TARGET_MASK: u32 = 0xffc00000; // xhci.h:1037, `(0x3ff << 22)`
pub const EVENT_TRANSFER_LENGTH_MASK: u32 = 0x00ff_ffff; // xhci.h:827
pub const SLOT_ID_MASK: u32 = 0xff00_0000; // xhci.h:817-:818
pub const ENDPOINT_ID_MASK: u32 = 0x001f_0000; // xhci.h:820

/// Linux's named TRB type corpus from xhci.h:1100-:1164.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrbType {
    Normal = 1,
    Setup = 2,
    Data = 3,
    Status = 4,
    Isoch = 5,
    Link = 6,
    EventData = 7,
    TransferNoop = 8,
    EnableSlot = 9,
    DisableSlot = 10,
    AddressDevice = 11,
    ConfigureEndpoint = 12,
    EvaluateContext = 13,
    ResetEndpoint = 14,
    StopRing = 15,
    SetDequeue = 16,
    ResetDevice = 17,
    ForceEvent = 18,
    NegotiateBandwidth = 19,
    SetLatencyTolerance = 20,
    GetPortBandwidth = 21,
    ForceHeader = 22,
    CommandNoop = 23,
    TransferEvent = 32,
    CommandCompletion = 33,
    PortStatusChange = 34,
    BandwidthRequest = 35,
    DoorbellEvent = 36,
    HostControllerEvent = 37,
    DeviceNotification = 38,
    MfindexWrap = 39,
    NecCommandCompletion = 48,
    NecGetFirmware = 49,
}

/// Frozen names for every distinct Linux TRB type constant above.
pub const TRB_TYPES: &[(u8, &str)] = &[
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

/// Encode `TRB_TYPE(p)` (xhci.h:1096).
pub const fn encode_type(kind: u8) -> u32 {
    (kind as u32) << 10
}

/// Decode `TRB_FIELD_TO_TYPE(p)` (xhci.h:1097).
pub const fn decode_type(control: u32) -> u8 {
    ((control & TYPE_MASK) >> 10) as u8
}

/// Identify the Link TRB type as `TRB_TYPE_LINK` does (xhci.h:1244).
pub const fn is_link(control: u32) -> bool {
    (control & TYPE_MASK) == encode_type(TrbType::Link as u8)
}

/// Encode `TRB_LEN(p)`; Linux masks rather than refuses (xhci.h:1030).
pub const fn encode_transfer_length(length: u32) -> u32 {
    length & TRANSFER_LENGTH_MASK
}

/// Decode `TRB_LEN(p)` (xhci.h:1030).
pub const fn decode_transfer_length(status: u32) -> u32 {
    status & TRANSFER_LENGTH_MASK
}

/// Encode `TRB_TD_SIZE(p)`, including Linux's clamp to 31 (xhci.h:1032).
pub const fn encode_td_size(packets: u32) -> u32 {
    (if packets > 31 { 31 } else { packets }) << 17
}

/// Decode `GET_TD_SIZE(p)` (xhci.h:1033).
pub const fn decode_td_size(status: u32) -> u8 {
    ((status & TD_SIZE_MASK) >> 17) as u8
}

/// Encode `TRB_INTR_TARGET(p)` (xhci.h:1037).
pub const fn encode_interrupter_target(target: u16) -> u32 {
    ((target as u32) & 0x3ff) << 22
}

/// Decode `GET_INTR_TARGET(p)` (xhci.h:1038).
pub const fn decode_interrupter_target(status: u32) -> u16 {
    ((status >> 22) & 0x3ff) as u16
}

/// Decode `EVENT_TRB_LEN(p)` (xhci.h:827).
pub const fn event_transfer_length(status: u32) -> u32 {
    status & EVENT_TRANSFER_LENGTH_MASK
}

/// Encode `SLOT_ID_FOR_TRB(p)` (xhci.h:818).
pub const fn encode_slot_id(slot_id: u8) -> u32 {
    (slot_id as u32) << 24
}

/// Decode `TRB_TO_SLOT_ID(p)` (xhci.h:817).
pub const fn decode_slot_id(control: u32) -> u8 {
    ((control >> 24) & 0xff) as u8
}

/// Encode Linux's zero-based endpoint index as `EP_INDEX_FOR_TRB(p)` (xhci.h:824).
pub const fn encode_endpoint_index(index: u8) -> u32 {
    (((index as u32) + 1) & 0x1f) << 16
}

/// Decode `TRB_TO_EP_INDEX(p)` (xhci.h:822). Endpoint ID zero is a named refusal because
/// subtracting one would underflow and is outside Linux's stated endpoint-ID range 1..=31.
pub const fn decode_endpoint_index(control: u32) -> Result<u8, FieldError> {
    let id = ((control >> 16) & 0x1f) as u8;
    if id == 0 {
        Err(FieldError::EndpointIdZero)
    } else {
        Ok(id - 1)
    }
}

/// A field could not be represented according to the Linux bit-field contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldError {
    /// Endpoint ID zero refused conversion to a zero-based index; Linux permits IDs 1 through 31.
    EndpointIdZero,
}

/// Whether a consumer owns this TRB: its cycle bit equals CCS (xhci-ring.c:135-:139).
pub const fn consumer_owns(control: u32, cycle_state: bool) -> bool {
    (control & CYCLE != 0) == cycle_state
}

/// Put the producer cycle state into a TRB control word (xhci-ring.c:37-:40, :4389-:4390).
pub const fn with_cycle(control: u32, cycle_state: bool) -> u32 {
    (control & !CYCLE) | cycle_state as u32
}

/// Build the four words queued by `queue_trb`; this is data construction only and deliberately
/// omits Linux's barrier and ring write (xhci-ring.c:3239-:3256).
pub const fn words(field1: u32, field2: u32, field3: u32, field4: u32) -> Trb {
    Trb { words: [field1, field2, field3, field4] }
}
