// SPDX-License-Identifier: GPL-2.0-only
//! Typed descriptor views and named parse refusals.
//!
//! Ported from Linux `drivers/usb/core/config.c` and `include/uapi/linux/usb/ch9.h`.
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.

use core::fmt;

pub const HEADER_SIZE: u8 = 2; // include/uapi/linux/usb/ch9.h:276-279
pub const CONFIGURATION_SIZE: u8 = 9; // include/uapi/linux/usb/ch9.h:366
pub const INTERFACE_SIZE: u8 = 9; // include/uapi/linux/usb/ch9.h:410
pub const ENDPOINT_SIZE: u8 = 7; // include/uapi/linux/usb/ch9.h:430
pub const ENDPOINT_AUDIO_SIZE: u8 = 9; // include/uapi/linux/usb/ch9.h:431

pub const CONFIGURATION_TYPE: u8 = 0x02; // include/uapi/linux/usb/ch9.h:238
pub const INTERFACE_TYPE: u8 = 0x04; // include/uapi/linux/usb/ch9.h:240
pub const ENDPOINT_TYPE: u8 = 0x05; // include/uapi/linux/usb/ch9.h:241

/// Descriptor kinds this crate interprets, with Linux's names and literals.
pub const PARSED_DESCRIPTOR_TYPES: &[(&str, u8)] = &[
    ("CONFIGURATION", 0x02), // include/uapi/linux/usb/ch9.h:238
    ("INTERFACE", 0x04),     // include/uapi/linux/usb/ch9.h:240
    ("ENDPOINT", 0x05),      // include/uapi/linux/usb/ch9.h:241
];

/// The parser operation that refused attacker-controlled bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalSite {
    Header,
    Configuration,
    Interface,
    Endpoint,
    Walk,
}

impl RefusalSite {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Header => "descriptor header",
            Self::Configuration => "configuration descriptor",
            Self::Interface => "interface descriptor",
            Self::Endpoint => "endpoint descriptor",
            Self::Walk => "descriptor walk",
        }
    }
}

/// Why a descriptor operation refused its input. Every variant carries the offending value and
/// the relevant bound or expected value; no malformed byte stream collapses to a bare `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseError {
    Truncated {
        site: RefusalSite,
        available: usize,
        required: usize,
    },
    LengthBelowMinimum {
        site: RefusalSite,
        length: u8,
        minimum: u8,
    },
    LengthExceedsBuffer {
        site: RefusalSite,
        length: u8,
        available: usize,
    },
    UnexpectedType {
        site: RefusalSite,
        actual: u8,
        expected: u8,
    },
    EndpointZero {
        address: u8,
    },
}

impl ParseError {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Truncated { site, .. } => site.name(),
            Self::LengthBelowMinimum { site, .. } => site.name(),
            Self::LengthExceedsBuffer { site, .. } => site.name(),
            Self::UnexpectedType { site, .. } => site.name(),
            Self::EndpointZero { .. } => "endpoint address",
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Truncated {
                site,
                available,
                required,
            } => write!(
                f,
                "{} refused truncated input: available {}, required {}",
                site.name(),
                available,
                required
            ),
            Self::LengthBelowMinimum {
                site,
                length,
                minimum,
            } => write!(
                f,
                "{} refused bLength {} below minimum {}",
                site.name(),
                length,
                minimum
            ),
            Self::LengthExceedsBuffer {
                site,
                length,
                available,
            } => write!(
                f,
                "{} refused bLength {} exceeding available {}",
                site.name(),
                length,
                available
            ),
            Self::UnexpectedType {
                site,
                actual,
                expected,
            } => write!(
                f,
                "{} refused bDescriptorType {:#04x}, expected {:#04x}",
                site.name(),
                actual,
                expected
            ),
            Self::EndpointZero { address } => write!(
                f,
                "endpoint address refused endpoint zero in bEndpointAddress {:#04x}",
                address
            ),
        }
    }
}

/// The common `bLength`, `bDescriptorType` prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DescriptorHeader {
    pub length: u8,
    pub descriptor_type: u8,
}

/// Parse and validate the common descriptor header.
///
/// Linux validates that at least two bytes remain and then requires `2 <= bLength <= remaining`
/// before advancing (`config.c:706-718`).
pub fn parse_header(bytes: &[u8]) -> Result<DescriptorHeader, ParseError> {
    if bytes.len() < HEADER_SIZE as usize {
        return Err(ParseError::Truncated {
            site: RefusalSite::Header,
            available: bytes.len(),
            required: HEADER_SIZE as usize,
        });
    }
    let header = DescriptorHeader {
        length: bytes[0],
        descriptor_type: bytes[1],
    };
    if header.length < HEADER_SIZE {
        return Err(ParseError::LengthBelowMinimum {
            site: RefusalSite::Header,
            length: header.length,
            minimum: HEADER_SIZE,
        });
    }
    if header.length as usize > bytes.len() {
        return Err(ParseError::LengthExceedsBuffer {
            site: RefusalSite::Header,
            length: header.length,
            available: bytes.len(),
        });
    }
    Ok(header)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigurationDescriptor {
    pub total_length: u16,
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub configuration_string: u8,
    pub attributes: u8,
    pub max_power: u8,
}

/// Parse the fixed configuration descriptor. Linux requires type 2, length at least 9, and a
/// `bLength` that fits the supplied buffer (`config.c:675-685`).
pub fn parse_configuration(bytes: &[u8]) -> Result<ConfigurationDescriptor, ParseError> {
    require_fixed_prefix(bytes, RefusalSite::Configuration, CONFIGURATION_SIZE)?;
    require_type(bytes[1], CONFIGURATION_TYPE, RefusalSite::Configuration)?;
    Ok(ConfigurationDescriptor {
        total_length: u16::from_le_bytes([bytes[2], bytes[3]]),
        num_interfaces: bytes[4],
        configuration_value: bytes[5],
        configuration_string: bytes[6],
        attributes: bytes[7],
        max_power: bytes[8],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterfaceDescriptor {
    pub number: u8,
    pub alternate_setting: u8,
    pub num_endpoints: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub interface_string: u8,
}

/// Parse the fixed interface descriptor (`config.c:565-570`, `config.c:721-730`).
pub fn parse_interface(bytes: &[u8]) -> Result<InterfaceDescriptor, ParseError> {
    require_fixed_prefix(bytes, RefusalSite::Interface, INTERFACE_SIZE)?;
    require_type(bytes[1], INTERFACE_TYPE, RefusalSite::Interface)?;
    Ok(InterfaceDescriptor {
        number: bytes[2],
        alternate_setting: bytes[3],
        num_endpoints: bytes[4],
        class: bytes[5],
        subclass: bytes[6],
        protocol: bytes[7],
        interface_string: bytes[8],
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointDescriptor {
    pub address: u8,
    pub attributes: u8,
    pub max_packet_size: u16,
    pub interval: u8,
    pub audio_extension: Option<[u8; 2]>,
}

/// Parse an endpoint descriptor. Linux accepts either the seven-byte standard form or the
/// nine-byte audio form and rejects endpoint number zero (`config.c:300-321`).
pub fn parse_endpoint(bytes: &[u8]) -> Result<EndpointDescriptor, ParseError> {
    require_fixed_prefix(bytes, RefusalSite::Endpoint, ENDPOINT_SIZE)?;
    require_type(bytes[1], ENDPOINT_TYPE, RefusalSite::Endpoint)?;
    let address = bytes[2] & 0x8f; // config.c:333-340; ch9.h:437-438
    if address & 0x0f == 0 {
        return Err(ParseError::EndpointZero { address: bytes[2] });
    }
    let audio_extension = if bytes[0] >= ENDPOINT_AUDIO_SIZE {
        Some([bytes[7], bytes[8]])
    } else {
        None
    };
    Ok(EndpointDescriptor {
        address,
        attributes: bytes[3],
        max_packet_size: u16::from_le_bytes([bytes[4], bytes[5]]),
        interval: bytes[6],
        audio_extension,
    })
}

fn require_fixed_prefix(bytes: &[u8], site: RefusalSite, minimum: u8) -> Result<(), ParseError> {
    if bytes.len() < HEADER_SIZE as usize {
        return Err(ParseError::Truncated {
            site,
            available: bytes.len(),
            required: HEADER_SIZE as usize,
        });
    }
    let length = bytes[0];
    if length < minimum {
        return Err(ParseError::LengthBelowMinimum {
            site,
            length,
            minimum,
        });
    }
    if length as usize > bytes.len() {
        return Err(ParseError::LengthExceedsBuffer {
            site,
            length,
            available: bytes.len(),
        });
    }
    Ok(())
}

fn require_type(actual: u8, expected: u8, site: RefusalSite) -> Result<(), ParseError> {
    if actual != expected {
        Err(ParseError::UnexpectedType {
            site,
            actual,
            expected,
        })
    } else {
        Ok(())
    }
}
