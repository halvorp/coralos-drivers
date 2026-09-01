// SPDX-License-Identifier: GPL-2.0-only
//! Completion vectors from Linux `drivers/usb/host/xhci.h:827-:939, :968`.
//! Copyright (C) 2008 Intel Corp., Sarah Sharp, and the Linux xHCI authors.

use xhci_ring_core::completion::*;

/// xhci.h:832-:939. All names/values are literal and independent of the production table. There
/// are 36 named codes in 0..=36 because value 30 is reserved.
#[test]
fn all_thirty_six_completion_codes_are_present_and_named() {
    let expected = [
        (0, "Invalid"),
        (1, "Success"),
        (2, "Data Buffer Error"),
        (3, "Babble Detected"),
        (4, "USB Transaction Error"),
        (5, "TRB Error"),
        (6, "Stall Error"),
        (7, "Resource Error"),
        (8, "Bandwidth Error"),
        (9, "No Slots Available Error"),
        (10, "Invalid Stream Type Error"),
        (11, "Slot Not Enabled Error"),
        (12, "Endpoint Not Enabled Error"),
        (13, "Short Packet"),
        (14, "Ring Underrun"),
        (15, "Ring Overrun"),
        (16, "VF Event Ring Full Error"),
        (17, "Parameter Error"),
        (18, "Bandwidth Overrun Error"),
        (19, "Context State Error"),
        (20, "No Ping Response Error"),
        (21, "Event Ring Full Error"),
        (22, "Incompatible Device Error"),
        (23, "Missed Service Error"),
        (24, "Command Ring Stopped"),
        (25, "Command Aborted"),
        (26, "Stopped"),
        (27, "Stopped - Length Invalid"),
        (28, "Stopped - Short Packet"),
        (29, "Max Exit Latency Too Large Error"),
        (31, "Isoch Buffer Overrun"),
        (32, "Event Lost Error"),
        (33, "Undefined Error"),
        (34, "Invalid Stream ID Error"),
        (35, "Secondary Bandwidth Error"),
        (36, "Split Transaction Error"),
    ];
    let got: Vec<(u8, &str)> = COMPLETION_CODES.iter().map(|c| (c.value, c.name)).collect();
    assert_eq!(COMPLETION_CODES.len(), 36);
    assert_eq!(got, expected);
    assert!(!got.iter().any(|entry| entry.0 == 30), "completion code 30 is reserved");
}

/// xhci.h:830-:831, :968. Completion code is the top byte; command completion parameter is the
/// bottom 24 bits.
#[test]
fn status_word_splits_into_code_and_parameter() {
    assert_eq!(CODE_MASK, 0xff00_0000);
    assert_eq!(PARAMETER_MASK, 0x00ff_ffff);
    assert_eq!(code(0x19ab_cdef), 25, "COMP_COMMAND_ABORTED, xhci.h:857");
    assert_eq!(parameter(0x19ab_cdef), 0x00ab_cdef);
}

/// xhci.h:833 and :856. Decode returns the literal known record plus the low parameter.
#[test]
fn decode_reports_known_completion_and_parameter() {
    let got = decode(0x1801_2345);
    assert_eq!(got.value, 24);
    assert_eq!(got.parameter, 0x0001_2345);
    assert_eq!(got.known, Some(CompletionCode { value: 24, name: "Command Ring Stopped" }));
}

/// Linux's default string is `Unknown!!` (xhci.h:936-:938), but the numeric status must survive.
#[test]
fn decode_does_not_silently_discard_an_unknown_code() {
    let got = decode(0xfe65_4321);
    assert_eq!(got.value, 0xfe);
    assert_eq!(got.parameter, 0x0065_4321);
    assert_eq!(got.known, None);
}
