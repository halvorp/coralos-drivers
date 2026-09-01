// SPDX-License-Identifier: GPL-2.0-only
//! Hub descriptor field decoding.
//!
//! Ported from Linux `drivers/usb/core/hub.c` (`get_hub_descriptor`, hub.c:410-:441 and
//! `hub_configure`, hub.c:1488-:1643) and `include/uapi/linux/usb/ch11.h` (ch11.h:207-:220,
//! :246-:247, :253-:276, :300-:303).
//!
//! Copyright 1999 Linus Torvalds, Johannes Erdfelt, and Gregory P. Smith.
//! Copyright 2001 Brad Hards and the Linux USB core authors.

/// Fixed bytes preceding the variable USB 2 hub bitmaps (ch11.h:246).
pub const HUB_NONVAR_SIZE: usize = 7;
/// Complete SuperSpeed hub descriptor length (ch11.h:247).
pub const SS_HUB_SIZE: usize = 12;

/// Logical power-switching field mask (ch11.h:207).
pub const CHAR_LPSM: u16 = 0x0003;
/// Compound-device flag (ch11.h:212).
pub const CHAR_COMPOUND: u16 = 0x0004;
/// Over-current protection field mask (ch11.h:214).
pub const CHAR_OCPM: u16 = 0x0018;
/// Transaction-translator think-time field mask (ch11.h:219).
pub const CHAR_TTTT: u16 = 0x0060;
/// Per-port indicator flag (ch11.h:220).
pub const CHAR_PORT_INDICATORS: u16 = 0x0080;

/// The three Linux power-switching interpretations (hub.c:1554-:1565).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSwitching {
    Ganged,
    Individual,
    None,
}

/// The three Linux over-current interpretations (hub.c:1567-:1578).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverCurrentProtection {
    Global,
    Individual,
    None,
}

/// Refusal to accept a hub descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorError {
    /// `bNbrPorts == 0` is refused by hub.c:1501-:1504.
    HubHasNoPorts,
    /// `bNbrPorts > maxchild` is refused by hub.c:1497-:1500.
    HubHasTooManyPorts { ports: u8, maximum: u8 },
    /// Linux requires the DeviceRemovable bytes computed at hub.c:431-:436.
    MissingDeviceRemovable { received: usize, required: usize },
    /// A SuperSpeed descriptor must have exactly `USB_DT_SS_HUB_SIZE` bytes (hub.c:426-:428).
    InvalidSuperSpeedLength { received: usize, required: usize },
    /// The fixed descriptor fields cannot be decoded from fewer than seven bytes (ch11.h:253-:260).
    MissingFixedFields { received: usize, required: usize },
}

/// Decoded fixed hub descriptor fields used by enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HubDescriptor {
    pub ports: u8,
    pub characteristics: u16,
    pub power_on_to_good_ms: u16,
    pub controller_current_ma: u8,
}

impl HubDescriptor {
    /// Decode the fixed seven-byte prefix. Multi-byte fields are little-endian (ch11.h:253-:260).
    pub fn decode(bytes: &[u8]) -> Result<Self, DescriptorError> {
        if bytes.len() < HUB_NONVAR_SIZE {
            return Err(DescriptorError::MissingFixedFields {
                received: bytes.len(),
                required: HUB_NONVAR_SIZE,
            });
        }
        Ok(Self {
            ports: bytes[2],
            characteristics: u16::from_le_bytes([bytes[3], bytes[4]]),
            // bPwrOn2PwrGood is in two-millisecond units (hub.c:1645-:1646).
            power_on_to_good_ms: bytes[5] as u16 * 2,
            controller_current_ma: bytes[6],
        })
    }

    /// Validate Linux's nonzero and maximum port-count rules (hub.c:1497-:1504).
    pub fn validate_ports(self, maximum: u8) -> Result<Self, DescriptorError> {
        if self.ports == 0 {
            return Err(DescriptorError::HubHasNoPorts);
        }
        if self.ports > maximum {
            return Err(DescriptorError::HubHasTooManyPorts {
                ports: self.ports,
                maximum,
            });
        }
        Ok(self)
    }

    /// Decode the logical power switching mode (hub.c:1554-:1565; ch11.h:207-:210).
    pub fn power_switching(self) -> PowerSwitching {
        match self.characteristics & CHAR_LPSM {
            0x0000 => PowerSwitching::Ganged,
            0x0001 => PowerSwitching::Individual,
            0x0002 | 0x0003 => PowerSwitching::None,
            _ => unreachable!(),
        }
    }

    /// Decode the over-current protection mode (hub.c:1567-:1578; ch11.h:214-:217).
    pub fn over_current(self) -> OverCurrentProtection {
        match self.characteristics & CHAR_OCPM {
            0x0000 => OverCurrentProtection::Global,
            0x0008 => OverCurrentProtection::Individual,
            0x0010 | 0x0018 => OverCurrentProtection::None,
            _ => unreachable!(),
        }
    }

    /// Whether the hub is part of a compound device (hub.c:1541; ch11.h:212).
    pub fn is_compound(self) -> bool {
        self.characteristics & CHAR_COMPOUND != 0
    }

    /// Whether per-port indicators exist (hub.c:1640-:1643; ch11.h:220).
    pub fn has_port_indicators(self) -> bool {
        self.characteristics & CHAR_PORT_INDICATORS != 0
    }

    /// TT think time in nanoseconds, using Linux's 666 ns per eight FS bit times
    /// (hub.c:1610-:1637; ch11.h:300-:303).
    pub fn tt_think_time_ns(self) -> u16 {
        match self.characteristics & CHAR_TTTT {
            0x0000 => 666,
            0x0020 => 1_332,
            0x0040 => 1_998,
            0x0060 => 2_664,
            _ => unreachable!(),
        }
    }
}

/// Number of USB 2 hub descriptor bytes needed through DeviceRemovable.
///
/// Linux computes `USB_DT_HUB_NONVAR_SIZE + bNbrPorts / 8 + 1` (hub.c:431-:433).
pub fn usb2_descriptor_required_len(ports: u8) -> usize {
    HUB_NONVAR_SIZE + ports as usize / 8 + 1
}

/// Validate the amount returned by GET_DESCRIPTOR (hub.c:426-:439).
pub fn validate_descriptor_length(
    superspeed: bool,
    ports: u8,
    received: usize,
) -> Result<(), DescriptorError> {
    if superspeed {
        if received != SS_HUB_SIZE {
            return Err(DescriptorError::InvalidSuperSpeedLength {
                received,
                required: SS_HUB_SIZE,
            });
        }
        return Ok(());
    }
    let required = usb2_descriptor_required_len(ports);
    if received < required {
        return Err(DescriptorError::MissingDeviceRemovable { received, required });
    }
    Ok(())
}

/// Whether a one-based USB 2 hub port is removable.
///
/// Linux indexes bit `port / 8`, bit `port % 8`, and interprets one as fixed/non-removable
/// (hub.c:1545-:1548). The caller supplies the DeviceRemovable bitmap beginning at descriptor
/// offset seven.
pub fn port_is_removable(bitmap: &[u8], port: u8) -> Result<bool, DescriptorError> {
    let byte = port as usize / 8;
    if port == 0 || byte >= bitmap.len() {
        return Err(DescriptorError::MissingDeviceRemovable {
            received: bitmap.len(),
            required: byte + 1,
        });
    }
    Ok(bitmap[byte] & (1 << (port % 8)) == 0)
}
