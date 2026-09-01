// SPDX-License-Identifier: GPL-2.0-only
//! Frozen Linux vectors for PORTSC fields and encode/decode helpers.

use xhci_hub_core::portsc::*;

/// xhci-port.h:5-:119 and xhci-hub.c:399-:428. Expected names and literals are independent of the
/// production constants. Linux names 20 PORTSC fields here.
#[test]
fn portsc_literal_corpus_is_pinned() {
    let actual = [
        ("PORT_CONNECT", PORT_CONNECT), ("PORT_PE", PORT_PE), ("PORT_OC", PORT_OC),
        ("PORT_RESET", PORT_RESET), ("PORT_PLS_MASK", PORT_PLS_MASK),
        ("PORT_POWER", PORT_POWER), ("PORT_SPEED_MASK", PORT_SPEED_MASK),
        ("PORT_LINK_STROBE", PORT_LINK_STROBE), ("PORT_CSC", PORT_CSC),
        ("PORT_PEC", PORT_PEC), ("PORT_WRC", PORT_WRC), ("PORT_OCC", PORT_OCC),
        ("PORT_RC", PORT_RC), ("PORT_PLC", PORT_PLC), ("PORT_CEC", PORT_CEC),
        ("PORT_CAS", PORT_CAS), ("PORT_WKCONN_E", PORT_WKCONN_E),
        ("PORT_WKDISC_E", PORT_WKDISC_E), ("PORT_WKOC_E", PORT_WKOC_E),
        ("PORT_DEV_REMOVE", PORT_DEV_REMOVE),
    ];
    let expected = [
        ("PORT_CONNECT", 0x0000_0001), ("PORT_PE", 0x0000_0002), ("PORT_OC", 0x0000_0008),
        ("PORT_RESET", 0x0000_0010), ("PORT_PLS_MASK", 0x0000_01e0),
        ("PORT_POWER", 0x0000_0200), ("PORT_SPEED_MASK", 0x0000_3c00),
        ("PORT_LINK_STROBE", 0x0001_0000), ("PORT_CSC", 0x0002_0000),
        ("PORT_PEC", 0x0004_0000), ("PORT_WRC", 0x0008_0000), ("PORT_OCC", 0x0010_0000),
        ("PORT_RC", 0x0020_0000), ("PORT_PLC", 0x0040_0000), ("PORT_CEC", 0x0080_0000),
        ("PORT_CAS", 0x0100_0000), ("PORT_WKCONN_E", 0x0200_0000),
        ("PORT_WKDISC_E", 0x0400_0000), ("PORT_WKOC_E", 0x0800_0000),
        ("PORT_DEV_REMOVE", 0x4000_0000),
    ];
    assert_eq!(actual.len(), 20);
    assert_eq!(actual, expected);
    assert_eq!(PORT_WR, 0x8000_0000); // xhci-port.h:119
    assert_eq!((XHCI_PORT_RO, XHCI_PORT_RWS), (0x4000_3c09, 0x0e00_c3e0)); // xhci-hub.c:399/:405
    assert_eq!((XHCI_PORT_RW1S, XHCI_PORT_RW1CS, XHCI_PORT_RW, XHCI_PORT_RZ),
               (0x10, 0x00fe_0002, 0x0001_0000, 0xf100_0004)); // xhci-hub.c:410/:418/:423/:428
}

/// xhci-port.h:18-:30 defines 13 named PLS values. Count and names are frozen explicitly.
#[test]
fn all_linux_link_state_names_and_values_are_pinned() {
    let actual = [
        ("U0", LinkState::U0.bits()), ("U1", LinkState::U1.bits()),
        ("U2", LinkState::U2.bits()), ("U3", LinkState::U3.bits()),
        ("Disabled", LinkState::Disabled.bits()), ("RxDetect", LinkState::RxDetect.bits()),
        ("Inactive", LinkState::Inactive.bits()), ("Polling", LinkState::Polling.bits()),
        ("Recovery", LinkState::Recovery.bits()), ("HotReset", LinkState::HotReset.bits()),
        ("Compliance", LinkState::Compliance.bits()), ("Test", LinkState::Test.bits()),
        ("Resume", LinkState::Resume.bits()),
    ];
    let expected = [
        ("U0", 0x000), ("U1", 0x020), ("U2", 0x040), ("U3", 0x060),
        ("Disabled", 0x080), ("RxDetect", 0x0a0), ("Inactive", 0x0c0),
        ("Polling", 0x0e0), ("Recovery", 0x100), ("HotReset", 0x120),
        ("Compliance", 0x140), ("Test", 0x160), ("Resume", 0x1e0),
    ];
    assert_eq!(actual.len(), 13);
    assert_eq!(actual, expected);
    for (state, bits) in [
        (LinkState::U0, 0x000), (LinkState::U1, 0x020), (LinkState::U2, 0x040),
        (LinkState::U3, 0x060), (LinkState::Disabled, 0x080), (LinkState::RxDetect, 0x0a0),
        (LinkState::Inactive, 0x0c0), (LinkState::Polling, 0x0e0),
        (LinkState::Recovery, 0x100), (LinkState::HotReset, 0x120),
        (LinkState::Compliance, 0x140), (LinkState::Test, 0x160), (LinkState::Resume, 0x1e0),
    ] {
        assert_eq!(LinkState::decode(bits), Ok(state));
    }
    assert_eq!(LinkState::decode(0x180), Err(PortScError::ReservedLinkState { value: 12 }));
}

/// xhci-hub.c:446-:452 preserves only RO and RWS bits.
#[test]
fn neutral_drops_every_action_and_change_bit() {
    assert_eq!(neutral(0xffff_ffff), 0x4e00_ffe9);
    assert_eq!(neutral(PORT_RESET | PORT_CSC | PORT_LINK_STROBE), 0);
}

/// xhci-hub.c:804-:808 clears old PLS and sets LWS plus requested state.
#[test]
fn link_state_write_is_neutral_and_strobed() {
    assert_eq!(set_link_state(0xffff_ffff, LinkState::U3), 0x4e01_fe69);
}

/// xhci-port.h:35-:36, xhci-hub.c:1030-:1036, xhci-port.h:148-:149.
#[test]
fn speed_and_lane_fields_round_trip_into_extended_status() {
    assert_eq!(with_speed_id(0, 5), Ok(0x1400));
    assert_eq!(speed_id(0x1400), 5);
    assert_eq!(with_speed_id(0, 16), Err(PortScError::SpeedIdOutOfRange { value: 16, maximum: 15 }));
    assert_eq!(encode_portli_lanes(2, 4), Ok(0x0042_0000));
    assert_eq!(extended_port_status(0x1400, 0x0042_0000), 0x4255);
    assert_eq!(encode_portli_lanes(16, 1), Err(PortScError::LaneCountOutOfRange { field: "RX", value: 16, maximum: 15 }));
    assert_eq!(encode_portli_lanes(1, 16), Err(PortScError::LaneCountOutOfRange { field: "TX", value: 16, maximum: 15 }));
}
