// SPDX-License-Identifier: GPL-2.0-only
//! Port status/change-bit semantics and speed selection.
//!
//! Ported from Linux `drivers/usb/core/hub.c` (`hub_port_wait_reset`, hub.c:2984-:3045 and
//! `port_event`, hub.c:5764-:5822), `include/uapi/linux/usb/ch11.h` (ch11.h:123-:186), and
//! `include/uapi/linux/usb/ch9.h` (ch9.h:1201-:1207).
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.

pub mod status {
    pub const CONNECTION: u16 = 0x0001; // ch11.h:123
    pub const ENABLE: u16 = 0x0002; // ch11.h:124
    pub const SUSPEND: u16 = 0x0004; // ch11.h:125
    pub const OVERCURRENT: u16 = 0x0008; // ch11.h:126
    pub const RESET: u16 = 0x0010; // ch11.h:127
    pub const L1: u16 = 0x0020; // ch11.h:128
    pub const POWER: u16 = 0x0100; // ch11.h:130
    pub const LOW_SPEED: u16 = 0x0200; // ch11.h:131
    pub const HIGH_SPEED: u16 = 0x0400; // ch11.h:132
    pub const TEST: u16 = 0x0800; // ch11.h:133
    pub const INDICATOR: u16 = 0x1000; // ch11.h:134
    pub const LINK_STATE: u16 = 0x01e0; // ch11.h:141
    pub const SS_POWER: u16 = 0x0200; // ch11.h:142
    pub const SS_SPEED: u16 = 0x1c00; // ch11.h:143
    pub const SPEED_5GBPS: u16 = 0x0000; // ch11.h:144
    pub const SS_MASK: u16 = CONNECTION | ENABLE | OVERCURRENT | RESET; // ch11.h:147-:150
    pub const SS_U0: u16 = 0x0000; // ch11.h:155
    pub const SS_U1: u16 = 0x0020; // ch11.h:156
    pub const SS_U2: u16 = 0x0040; // ch11.h:157
    pub const SS_U3: u16 = 0x0060; // ch11.h:158
    pub const SS_DISABLED: u16 = 0x0080; // ch11.h:159
    pub const RX_DETECT: u16 = 0x00a0; // ch11.h:160
    pub const SS_INACTIVE: u16 = 0x00c0; // ch11.h:162
    pub const POLLING: u16 = 0x00e0; // ch11.h:163
    pub const RECOVERY: u16 = 0x0100; // ch11.h:164
    pub const HOT_RESET: u16 = 0x0120; // ch11.h:165
    pub const COMPLIANCE_MODE: u16 = 0x0140; // ch11.h:166
    pub const LOOPBACK: u16 = 0x0160; // ch11.h:167
}

pub mod change {
    pub const CONNECTION: u16 = 0x0001; // ch11.h:174
    pub const ENABLE: u16 = 0x0002; // ch11.h:175
    pub const SUSPEND: u16 = 0x0004; // ch11.h:176
    pub const OVERCURRENT: u16 = 0x0008; // ch11.h:177
    pub const RESET: u16 = 0x0010; // ch11.h:178
    pub const L1: u16 = 0x0020; // ch11.h:179
    /// USB 3 aliases the USB 2 L1-change bit as warm-reset change (ch11.h:179, :184).
    pub const BH_RESET: u16 = 0x0020; // ch11.h:184
    pub const LINK_STATE: u16 = 0x0040; // ch11.h:185
    pub const CONFIG_ERROR: u16 = 0x0080; // ch11.h:186
}

/// Linux USB speed ordering (ch9.h:1201-:1207).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UsbSpeed {
    Unknown,
    Low,
    Full,
    High,
    Wireless,
    Super,
    SuperPlus,
}

/// Error returned when status cannot name a valid enabled attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedError {
    PortDisconnected,
    PortNotEnabled,
}

/// Named change bits, in Linux's handling order (hub.c:5764-:5822).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChangeBit {
    pub name: &'static str,
    pub mask: u16,
}

/// Linux names nine semantic change causes. L1 and BH reset share the same wire bit but are
/// protocol-generation-specific interpretations (ch11.h:174-:186).
pub const CHANGE_BITS: [ChangeBit; 9] = [
    ChangeBit { name: "connection", mask: change::CONNECTION }, // hub.c:5764
    ChangeBit { name: "enable", mask: change::ENABLE }, // hub.c:5769
    ChangeBit { name: "suspend", mask: change::SUSPEND }, // hub.c:5813 (USB2 interpretation)
    ChangeBit { name: "over-current", mask: change::OVERCURRENT }, // hub.c:5787
    ChangeBit { name: "reset", mask: change::RESET }, // hub.c:5803
    ChangeBit { name: "l1", mask: change::L1 }, // ch11.h:179
    ChangeBit { name: "warm-reset", mask: change::BH_RESET }, // hub.c:5807
    ChangeBit { name: "link-state", mask: change::LINK_STATE }, // hub.c:5813
    ChangeBit { name: "config-error", mask: change::CONFIG_ERROR }, // hub.c:5818
];

/// Decoded status predicates. Status and change are deliberately separate words (ch11.h:115-:186).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortStatus {
    pub raw: u16,
}

impl PortStatus {
    pub fn connected(self) -> bool {
        self.raw & status::CONNECTION != 0
    }

    pub fn enabled(self) -> bool {
        self.raw & status::ENABLE != 0
    }

    pub fn resetting(self) -> bool {
        self.raw & status::RESET != 0
    }

    /// USB 2 and USB 3 use different power bits (hub.c:3237-:3245).
    pub fn powered(self, superspeed_hub: bool) -> bool {
        let mask = if superspeed_hub { status::SS_POWER } else { status::POWER };
        self.raw & mask != 0
    }
}

/// Decode speed after reset using Linux's exact precedence (hub.c:3035-:3045).
///
/// SuperSpeedPlus rate determination is supplied by the caller because parsing the xHCI/SSP
/// capability is outside this crate's scope.
pub fn speed_from_status(
    portstatus: u16,
    superspeed_hub: bool,
    has_ssp_rate: bool,
) -> Result<UsbSpeed, SpeedError> {
    let status_word = PortStatus { raw: portstatus };
    if !status_word.connected() {
        return Err(SpeedError::PortDisconnected);
    }
    if !status_word.enabled() {
        return Err(SpeedError::PortNotEnabled);
    }
    if has_ssp_rate {
        Ok(UsbSpeed::SuperPlus)
    } else if superspeed_hub {
        Ok(UsbSpeed::Super)
    } else if portstatus & status::HIGH_SPEED != 0 {
        Ok(UsbSpeed::High)
    } else if portstatus & status::LOW_SPEED != 0 {
        Ok(UsbSpeed::Low)
    } else {
        Ok(UsbSpeed::Full)
    }
}

/// A SuperSpeed Inactive or Compliance Mode link requires warm reset (hub.c:2940-:2951).
pub fn warm_reset_required(superspeed_hub: bool, marked: bool, portstatus: u16) -> bool {
    if !superspeed_hub {
        return false;
    }
    if marked {
        return true;
    }
    let state = portstatus & status::LINK_STATE;
    state == status::SS_INACTIVE || state == status::COMPLIANCE_MODE
}
