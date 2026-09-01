// SPDX-License-Identifier: GPL-2.0-only
//! PORTSC fields and pure encode/decode helpers.
//!
//! Ported from Linux `drivers/usb/host/xhci-hub.c` and
//! `drivers/usb/host/xhci-port.h`, by Sarah Sharp and the Linux xHCI authors.
//! Original copyright: Copyright (C) 2008 Intel Corp.

use core::fmt;

pub const PORT_CONNECT: u32 = 1 << 0; // xhci-port.h:5
pub const PORT_PE: u32 = 1 << 1; // xhci-port.h:7
pub const PORT_OC: u32 = 1 << 3; // xhci-port.h:10
pub const PORT_RESET: u32 = 1 << 4; // xhci-port.h:12
pub const PORT_PLS_MASK: u32 = 0xf << 5; // xhci-port.h:17
pub const PORT_POWER: u32 = 1 << 9; // xhci-port.h:33
pub const PORT_SPEED_MASK: u32 = 0xf << 10; // xhci-port.h:35
pub const PORT_LINK_STROBE: u32 = 1 << 16; // xhci-port.h:69
pub const PORT_CSC: u32 = 1 << 17; // xhci-port.h:71
pub const PORT_PEC: u32 = 1 << 18; // xhci-port.h:73
pub const PORT_WRC: u32 = 1 << 19; // xhci-port.h:79
pub const PORT_OCC: u32 = 1 << 20; // xhci-port.h:81
pub const PORT_RC: u32 = 1 << 21; // xhci-port.h:83
pub const PORT_PLC: u32 = 1 << 22; // xhci-port.h:97
pub const PORT_CEC: u32 = 1 << 23; // xhci-port.h:99
pub const PORT_CAS: u32 = 1 << 24; // xhci-port.h:108
pub const PORT_WKCONN_E: u32 = 1 << 25; // xhci-port.h:110
pub const PORT_WKDISC_E: u32 = 1 << 26; // xhci-port.h:112
pub const PORT_WKOC_E: u32 = 1 << 27; // xhci-port.h:114
pub const PORT_DEV_REMOVE: u32 = 1 << 30; // xhci-port.h:117
pub const PORT_WR: u32 = 1 << 31; // xhci-port.h:119

pub const XHCI_PORT_RO: u32 = (1 << 0) | (1 << 3) | (0xf << 10) | (1 << 30); // xhci-hub.c:399
pub const XHCI_PORT_RWS: u32 = (0xf << 5) | (1 << 9) | (0x3 << 14) | (0x7 << 25); // xhci-hub.c:405
pub const XHCI_PORT_RW1S: u32 = 1 << 4; // xhci-hub.c:410
pub const XHCI_PORT_RW1CS: u32 = (1 << 1) | (0x7f << 17); // xhci-hub.c:418
pub const XHCI_PORT_RW: u32 = 1 << 16; // xhci-hub.c:423
pub const XHCI_PORT_RZ: u32 = (1 << 2) | (1 << 24) | (0xf << 28); // xhci-hub.c:428

/// The 13 link-state encodings named by Linux in `xhci_portsc_link_state()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LinkState {
    U0 = 0,
    U1 = 1,
    U2 = 2,
    U3 = 3,
    Disabled = 4,
    RxDetect = 5,
    Inactive = 6,
    Polling = 7,
    Recovery = 8,
    HotReset = 9,
    Compliance = 10,
    Test = 11,
    Resume = 15,
}

impl LinkState {
    /// Linux PORTSC PLS encoding, shifted into bits 8:5 (xhci-port.h:17-:30).
    pub const fn bits(self) -> u32 {
        (self as u32) << 5
    }

    /// Decode PORTSC.PLS (xhci-port.h:17-:30), naming reserved encodings as a refusal.
    pub const fn decode(portsc: u32) -> Result<Self, PortScError> {
        let encoded = ((portsc & PORT_PLS_MASK) >> 5) as u8;
        match encoded {
            0 => Ok(Self::U0),
            1 => Ok(Self::U1),
            2 => Ok(Self::U2),
            3 => Ok(Self::U3),
            4 => Ok(Self::Disabled),
            5 => Ok(Self::RxDetect),
            6 => Ok(Self::Inactive),
            7 => Ok(Self::Polling),
            8 => Ok(Self::Recovery),
            9 => Ok(Self::HotReset),
            10 => Ok(Self::Compliance),
            11 => Ok(Self::Test),
            15 => Ok(Self::Resume),
            value => Err(PortScError::ReservedLinkState { value }),
        }
    }
}

/// Named refusals from PORTSC field encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortScError {
    /// PLS values 12 through 14 are not named by Linux (xhci-port.h:18-:30).
    ReservedLinkState { value: u8 },
    /// Speed IDs occupy four bits (xhci-port.h:35).
    SpeedIdOutOfRange { value: u8, maximum: u8 },
    /// Lane counts occupy four bits (xhci-port.h:148-:149).
    LaneCountOutOfRange { field: &'static str, value: u8, maximum: u8 },
}

impl fmt::Display for PortScError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReservedLinkState { value } => write!(f, "PORTSC PLS refused reserved link state {value}"),
            Self::SpeedIdOutOfRange { value, maximum } => write!(f, "PORTSC speed ID {value} exceeds maximum {maximum}"),
            Self::LaneCountOutOfRange { field, value, maximum } => write!(f, "PORTLI {field} lane count {value} exceeds maximum {maximum}"),
        }
    }
}

/// `xhci_port_state_to_neutral()` (xhci-hub.c:430-:452).
pub const fn neutral(state: u32) -> u32 {
    (state & XHCI_PORT_RO) | (state & XHCI_PORT_RWS)
}

/// Encode a PLS write with LWS set, as `xhci_set_link_state()` does (xhci-hub.c:804-:808).
pub const fn set_link_state(state: u32, link_state: LinkState) -> u32 {
    (neutral(state) & !PORT_PLS_MASK) | PORT_LINK_STROBE | link_state.bits()
}

/// Decode the four-bit port speed ID used by `DEV_PORT_SPEED` (xhci-hub.c:1030-:1033;
/// xhci-port.h:35-:36).
pub const fn speed_id(portsc: u32) -> u8 {
    ((portsc & PORT_SPEED_MASK) >> 10) as u8
}

/// Encode a speed ID into an otherwise supplied PORTSC value (xhci-port.h:35).
pub const fn with_speed_id(portsc: u32, speed: u8) -> Result<u32, PortScError> {
    if speed > 0xf {
        Err(PortScError::SpeedIdOutOfRange { value: speed, maximum: 0xf })
    } else {
        Ok((portsc & !PORT_SPEED_MASK) | ((speed as u32) << 10))
    }
}

/// Build USB 3.1 `dwExtPortStatus` from PORTSC and PORTLI (xhci-hub.c:1025-:1038).
pub const fn extended_port_status(raw_portsc: u32, portli: u32) -> u32 {
    let speed = speed_id(raw_portsc) as u32;
    let rx_lanes = (portli >> 16) & 0xf; // xhci-port.h:148
    let tx_lanes = (portli >> 20) & 0xf; // xhci-port.h:149
    speed | (speed << 4) | (rx_lanes << 8) | (tx_lanes << 12)
}

/// Encode PORTLI lane-count fields used by `extended_port_status` (xhci-port.h:148-:149).
pub const fn encode_portli_lanes(rx: u8, tx: u8) -> Result<u32, PortScError> {
    if rx > 0xf {
        return Err(PortScError::LaneCountOutOfRange { field: "RX", value: rx, maximum: 0xf });
    }
    if tx > 0xf {
        return Err(PortScError::LaneCountOutOfRange { field: "TX", value: tx, maximum: 0xf });
    }
    Ok(((rx as u32) << 16) | ((tx as u32) << 20))
}
