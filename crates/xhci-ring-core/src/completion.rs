// SPDX-License-Identifier: GPL-2.0-only
//! Completion-code and event-status decode, ported from Linux
//! `drivers/usb/host/xhci.h:827-:939, :968` and used by `drivers/usb/host/xhci-ring.c`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp, and the Linux xHCI authors.

pub const CODE_MASK: u32 = 0xff00_0000; // xhci.h:830
pub const PARAMETER_MASK: u32 = 0x00ff_ffff; // xhci.h:968

/// One completion code Linux names in xhci.h:832-:867.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionCode {
    pub value: u8,
    pub name: &'static str,
}

/// All 36 named completion codes. Value 30 is reserved and therefore absent.
pub const COMPLETION_CODES: &[CompletionCode] = &[
    CompletionCode { value: 0, name: "Invalid" },
    CompletionCode { value: 1, name: "Success" },
    CompletionCode { value: 2, name: "Data Buffer Error" },
    CompletionCode { value: 3, name: "Babble Detected" },
    CompletionCode { value: 4, name: "USB Transaction Error" },
    CompletionCode { value: 5, name: "TRB Error" },
    CompletionCode { value: 6, name: "Stall Error" },
    CompletionCode { value: 7, name: "Resource Error" },
    CompletionCode { value: 8, name: "Bandwidth Error" },
    CompletionCode { value: 9, name: "No Slots Available Error" },
    CompletionCode { value: 10, name: "Invalid Stream Type Error" },
    CompletionCode { value: 11, name: "Slot Not Enabled Error" },
    CompletionCode { value: 12, name: "Endpoint Not Enabled Error" },
    CompletionCode { value: 13, name: "Short Packet" },
    CompletionCode { value: 14, name: "Ring Underrun" },
    CompletionCode { value: 15, name: "Ring Overrun" },
    CompletionCode { value: 16, name: "VF Event Ring Full Error" },
    CompletionCode { value: 17, name: "Parameter Error" },
    CompletionCode { value: 18, name: "Bandwidth Overrun Error" },
    CompletionCode { value: 19, name: "Context State Error" },
    CompletionCode { value: 20, name: "No Ping Response Error" },
    CompletionCode { value: 21, name: "Event Ring Full Error" },
    CompletionCode { value: 22, name: "Incompatible Device Error" },
    CompletionCode { value: 23, name: "Missed Service Error" },
    CompletionCode { value: 24, name: "Command Ring Stopped" },
    CompletionCode { value: 25, name: "Command Aborted" },
    CompletionCode { value: 26, name: "Stopped" },
    CompletionCode { value: 27, name: "Stopped - Length Invalid" },
    CompletionCode { value: 28, name: "Stopped - Short Packet" },
    CompletionCode { value: 29, name: "Max Exit Latency Too Large Error" },
    CompletionCode { value: 31, name: "Isoch Buffer Overrun" },
    CompletionCode { value: 32, name: "Event Lost Error" },
    CompletionCode { value: 33, name: "Undefined Error" },
    CompletionCode { value: 34, name: "Invalid Stream ID Error" },
    CompletionCode { value: 35, name: "Secondary Bandwidth Error" },
    CompletionCode { value: 36, name: "Split Transaction Error" },
];

/// Decode `GET_COMP_CODE(p)` (xhci.h:831).
pub const fn code(status: u32) -> u8 {
    ((status & CODE_MASK) >> 24) as u8
}

/// Decode `COMP_PARAM(p)` (xhci.h:968).
pub const fn parameter(status: u32) -> u32 {
    status & PARAMETER_MASK
}

/// Return Linux's completion-code name, preserving unknown numeric codes in the result rather than
/// silently mapping them to a known error.
pub fn decode(status: u32) -> DecodedCompletion {
    let value = code(status);
    let known = COMPLETION_CODES.iter().find(|entry| entry.value == value).copied();
    DecodedCompletion { value, parameter: parameter(status), known }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodedCompletion {
    pub value: u8,
    pub parameter: u32,
    pub known: Option<CompletionCode>,
}
