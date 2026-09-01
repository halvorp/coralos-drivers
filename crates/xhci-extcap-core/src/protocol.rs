// SPDX-License-Identifier: GPL-2.0-only
//! Supported Protocol capability layout and protocol speed ID parsing.
//!
//! Ported from Linux `drivers/usb/host/xhci-ext-caps.h`.
//!
//! Copyright (C) 2008 Intel Corp. Original author: Sarah Sharp.

/// The three fixed DWORDs of `struct xhci_protocol_caps`, in Linux field order.
pub const PROTOCOL_CAP_FIELDS: [&str; 3] = ["revision", "name_string", "port_info"]; // xhci-ext-caps.h:95-99

/// The fixed portion of a Supported Protocol extended capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupportedProtocol {
    /// Major and minor revision fields from the capability header.
    pub revision: u32,
    /// Four-byte protocol name, typically `*b"USB "`.
    pub name_string: [u8; 4],
    /// Compatible-port range and protocol speed ID count.
    pub port_info: u32,
}

impl SupportedProtocol {
    /// Parse the three fixed DWORDs of Linux's `struct xhci_protocol_caps`.
    pub const fn parse(words: [u32; 3]) -> Self {
        Self {
            revision: words[0],                  // xhci-ext-caps.h:96
            name_string: words[1].to_le_bytes(), // xhci-ext-caps.h:97
            port_info: words[2],                 // xhci-ext-caps.h:98
        }
    }

    /// Protocol major revision.
    pub const fn major(self) -> u8 {
        ((self.revision >> 24) & 0xff) as u8 // xhci-ext-caps.h:101
    }

    /// Protocol minor revision.
    pub const fn minor(self) -> u8 {
        ((self.revision >> 16) & 0xff) as u8 // xhci-ext-caps.h:102
    }

    /// Number of protocol speed ID DWORDs following the fixed capability.
    pub const fn speed_id_count(self) -> u8 {
        ((self.port_info >> 28) & 0x0f) as u8 // xhci-ext-caps.h:103
    }

    /// One-based first compatible port number as encoded by xHCI.
    pub const fn compatible_port_offset(self) -> u8 {
        (self.port_info & 0xff) as u8 // xhci-ext-caps.h:104
    }

    /// Number of compatible ports.
    pub const fn compatible_port_count(self) -> u8 {
        ((self.port_info >> 8) & 0xff) as u8 // xhci-ext-caps.h:105
    }

    /// Select the protocol speed ID block following the three fixed DWORDs.
    pub fn speed_id_block<'a>(
        self,
        following_words: &'a [u32],
    ) -> Result<&'a [u32], SpeedIdBlockRefusal> {
        let required = self.speed_id_count() as usize; // xhci-ext-caps.h:103
        if following_words.len() < required {
            return Err(SpeedIdBlockRefusal::TooShort {
                available: following_words.len(),
                required,
            });
        }
        Ok(&following_words[..required])
    }

    /// Convert the xHCI one-based compatible-port range to a zero-based half-open array range.
    ///
    /// The refusal is named because offset zero has no corresponding xHCI port and silently
    /// subtracting one would wrap onto an unrelated array entry.
    pub const fn compatible_port_indices(self) -> Result<PortIndexRange, PortRangeRefusal> {
        let offset = self.compatible_port_offset();
        if offset == 0 {
            return Err(PortRangeRefusal::OffsetIsZero { offset, minimum: 1 });
        }
        let start = (offset - 1) as usize;
        let end = start + self.compatible_port_count() as usize;
        Ok(PortIndexRange { start, end })
    }
}

/// Zero-based half-open indices corresponding to an xHCI one-based compatible-port range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortIndexRange {
    pub start: usize,
    pub end: usize,
}

/// Why an encoded compatible-port range cannot safely index a zero-based array.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortRangeRefusal {
    /// xHCI port numbering starts at one; zero cannot be converted by subtracting one.
    OffsetIsZero { offset: u8, minimum: u8 },
}

/// Why the supplied words cannot contain the complete protocol speed ID block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeedIdBlockRefusal {
    TooShort { available: usize, required: usize },
}

/// One DWORD from the protocol speed ID block that follows the fixed capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolSpeedId(pub u32);

impl ProtocolSpeedId {
    pub const fn value(self) -> u8 {
        ((self.0 >> 0) & 0x0f) as u8 // xhci-ext-caps.h:107
    }

    pub const fn exponent(self) -> u8 {
        ((self.0 >> 4) & 0x03) as u8 // xhci-ext-caps.h:108
    }

    pub const fn protocol_type(self) -> u8 {
        ((self.0 >> 6) & 0x03) as u8 // xhci-ext-caps.h:109
    }

    pub const fn full_duplex(self) -> bool {
        ((self.0 >> 8) & 0x01) != 0 // xhci-ext-caps.h:110
    }

    pub const fn link_protocol(self) -> u8 {
        ((self.0 >> 14) & 0x03) as u8 // xhci-ext-caps.h:111
    }

    pub const fn mantissa(self) -> u16 {
        ((self.0 >> 16) & 0xffff) as u16 // xhci-ext-caps.h:112
    }
}
