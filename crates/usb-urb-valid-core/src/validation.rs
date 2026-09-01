// SPDX-License-Identifier: GPL-2.0-only
//! Per-transfer-type URB sanity checks and payload bounds.
//!
//! Ported from Linux `drivers/usb/core/urb.c:399-499`.
//!
//! Copyright (C) the Linux USB core authors.

use crate::{Direction, Speed, TransferType};

/// Linux `USB_DIR_IN`, used in a control setup request (`drivers/usb/core/urb.c:409`).
pub const USB_DIR_IN: u8 = 0x80; // include/uapi/linux/usb/ch9.h:46

/// The largest accepted transfer-buffer length (`drivers/usb/core/urb.c:497-499`).
pub const MAX_TRANSFER_BUFFER_LENGTH: u64 = i32::MAX as u64; // drivers/usb/core/urb.c:498

/// Caller-supplied control setup fields used by the URB checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSetup {
    pub request_type: u8,
    pub length: u16,
}

/// Caller-supplied descriptor fields needed for pure validation.
#[derive(Debug, Clone, Copy)]
pub struct UrbDescriptor<'a> {
    /// Endpoint descriptor type (`drivers/usb/core/urb.c:402`).
    pub transfer_type: TransferType,
    /// Type encoded in the URB pipe, compared against the endpoint at `urb.c:506-509`.
    pub pipe_transfer_type: TransferType,
    pub speed: Speed,
    pub endpoint_direction: Direction,
    /// The endpoint's base `wMaxPacketSize` payload bytes.
    pub max_packet_size: u32,
    /// High-speed isochronous transactions per microframe (normally 1..=3).
    pub high_speed_transactions: u8,
    /// SuperSpeed `1 + bMaxBurst` (`drivers/usb/core/urb.c:453-460`).
    pub superspeed_burst: u8,
    /// SuperSpeed `USB_SS_MULT(...)` multiplier (`drivers/usb/core/urb.c:457-460`).
    pub superspeed_mult: u8,
    /// SuperSpeedPlus companion override, `dwBytesPerInterval` (`urb.c:463-469`).
    pub superspeed_plus_bytes_per_interval: Option<u32>,
    /// eUSB2 double-bandwidth override, `dwBytesPerInterval` (`urb.c:471-476`).
    pub high_speed_double_bytes_per_interval: Option<u32>,
    pub setup: Option<ControlSetup>,
    pub transfer_buffer_length: u64,
    /// Linux's signed packet count (`drivers/usb/core/urb.c:479-481`).
    pub number_of_packets: i32,
    /// Caller-supplied packet descriptors. At least `number_of_packets` entries are required.
    pub iso_packet_lengths: &'a [i32],
    /// One length per scatter-gather entry (`include/linux/usb.h:1472-1474`).
    pub scatter_gather_lengths: &'a [u32],
    /// Linux skips SG alignment checks when the bus advertises this capability (`urb.c:488-494`).
    pub no_sg_constraint: bool,
}

/// Why Linux's pre-HCD sanity checks refuse a URB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    ControlSetupMissing {
        transfer_type: TransferType,
    },
    ControlLengthMismatch {
        setup_length: u16,
        transfer_buffer_length: u64,
    },
    DeviceNotConfigured {
        transfer_type: TransferType,
    },
    EndpointMaxPacketZero {
        transfer_type: TransferType,
        minimum: u32,
    },
    IsoPacketCountNotPositive {
        number_of_packets: i32,
        minimum: i32,
    },
    IsoPacketDescriptorsMissing {
        number_of_packets: usize,
        descriptors_supplied: usize,
    },
    IsoPacketLengthNegative {
        packet_index: usize,
        length: i32,
        minimum: i32,
    },
    IsoPacketLengthAboveMaximum {
        packet_index: usize,
        length: u32,
        maximum: u32,
    },
    MaximumPacketLengthAboveMaximum {
        calculated: u64,
        maximum: u32,
    },
    ScatterGatherLengthNotPacketAligned {
        entry_index: usize,
        length: u32,
        packet_size: u32,
    },
    TransferBufferLengthAboveMaximum {
        length: u64,
        maximum: u64,
    },
}

/// Values established by successful validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedUrb {
    /// Control direction comes from setup and a zero-length control transfer is OUT
    /// (`drivers/usb/core/urb.c:409-410`); other types use endpoint direction (:420-422).
    pub direction: Direction,
    /// Per-packet ISO bound after speed companion multipliers, or the endpoint max otherwise.
    pub maximum_packet_length: u32,
    /// Linux warns rather than rejects when the control pipe direction differs (:411-413).
    pub endpoint_direction_mismatch: bool,
    /// Linux warns rather than rejects when pipe and endpoint transfer types differ (:506-509).
    pub pipe_type_mismatch: bool,
}

/// Calculate Linux's ISO payload limit for one frame or microframe
/// (`drivers/usb/core/urb.c:450-477`).
///
/// Valid USB companion descriptors cannot overflow Linux's `int max`. Since this public API takes
/// decoded caller values, malformed values are named rather than silently clamped.
pub fn maximum_packet_length(descriptor: &UrbDescriptor<'_>) -> Result<u32, ValidationError> {
    let mut maximum = descriptor.max_packet_size as u64;
    if matches!(descriptor.transfer_type, TransferType::Isochronous) {
        if matches!(descriptor.speed, Speed::Super | Speed::SuperPlus) {
            maximum *= descriptor.superspeed_burst as u64;
            maximum *= descriptor.superspeed_mult as u64;
        }
        if matches!(descriptor.speed, Speed::SuperPlus) {
            if let Some(bytes) = descriptor.superspeed_plus_bytes_per_interval {
                maximum = bytes as u64;
            }
        }
        if matches!(descriptor.speed, Speed::High) {
            maximum = match descriptor.high_speed_double_bytes_per_interval {
                Some(bytes) => bytes as u64,
                None => maximum * descriptor.high_speed_transactions as u64,
            };
        }
    }
    if maximum > u32::MAX as u64 {
        return Err(ValidationError::MaximumPacketLengthAboveMaximum {
            calculated: maximum,
            maximum: u32::MAX,
        });
    }
    Ok(maximum as u32)
}

/// Validate the transfer-type rules, packet bounds, and `INT_MAX` buffer bound before HCD submit.
///
/// `device_configured` is deliberately supplied by the caller: this crate performs no device I/O.
pub fn validate(
    descriptor: &UrbDescriptor<'_>,
    device_configured: bool,
) -> Result<ValidatedUrb, ValidationError> {
    let direction = if matches!(descriptor.transfer_type, TransferType::Control) {
        let setup = descriptor
            .setup
            .ok_or(ValidationError::ControlSetupMissing {
                transfer_type: TransferType::Control,
            })?;
        if setup.length as u64 != descriptor.transfer_buffer_length {
            return Err(ValidationError::ControlLengthMismatch {
                setup_length: setup.length,
                transfer_buffer_length: descriptor.transfer_buffer_length,
            });
        }
        if setup.request_type & USB_DIR_IN == 0 || setup.length == 0 {
            Direction::Out
        } else {
            Direction::In
        }
    } else {
        if !device_configured {
            return Err(ValidationError::DeviceNotConfigured {
                transfer_type: descriptor.transfer_type,
            });
        }
        descriptor.endpoint_direction
    };

    let maximum = maximum_packet_length(descriptor)?;
    // eUSB2's double-bandwidth descriptor may supply the maximum despite base maxpacket being zero
    // (`drivers/usb/core/urb.c:436-443`).
    if descriptor.max_packet_size == 0 && descriptor.high_speed_double_bytes_per_interval.is_none()
    {
        return Err(ValidationError::EndpointMaxPacketZero {
            transfer_type: descriptor.transfer_type,
            minimum: 1,
        });
    }

    if matches!(descriptor.transfer_type, TransferType::Isochronous) {
        if descriptor.number_of_packets <= 0 {
            return Err(ValidationError::IsoPacketCountNotPositive {
                number_of_packets: descriptor.number_of_packets,
                minimum: 1,
            });
        }
        let number_of_packets = descriptor.number_of_packets as usize;
        if descriptor.iso_packet_lengths.len() < number_of_packets {
            return Err(ValidationError::IsoPacketDescriptorsMissing {
                number_of_packets,
                descriptors_supplied: descriptor.iso_packet_lengths.len(),
            });
        }
        for (packet_index, &length) in descriptor.iso_packet_lengths[..number_of_packets]
            .iter()
            .enumerate()
        {
            if length < 0 {
                return Err(ValidationError::IsoPacketLengthNegative {
                    packet_index,
                    length,
                    minimum: 0,
                });
            }
            if length as u32 > maximum {
                return Err(ValidationError::IsoPacketLengthAboveMaximum {
                    packet_index,
                    length: length as u32,
                    maximum,
                });
            }
        }
    } else if !descriptor.no_sg_constraint {
        // Every entry except the last must be divisible by maxpacket (`urb.c:488-494`).
        let checked_entries = descriptor.scatter_gather_lengths.len().saturating_sub(1);
        for (entry_index, &length) in descriptor.scatter_gather_lengths[..checked_entries]
            .iter()
            .enumerate()
        {
            if length % maximum != 0 {
                return Err(ValidationError::ScatterGatherLengthNotPacketAligned {
                    entry_index,
                    length,
                    packet_size: maximum,
                });
            }
        }
    }

    if descriptor.transfer_buffer_length > MAX_TRANSFER_BUFFER_LENGTH {
        return Err(ValidationError::TransferBufferLengthAboveMaximum {
            length: descriptor.transfer_buffer_length,
            maximum: MAX_TRANSFER_BUFFER_LENGTH,
        });
    }

    Ok(ValidatedUrb {
        direction,
        maximum_packet_length: maximum,
        endpoint_direction_mismatch: descriptor.endpoint_direction != direction,
        pipe_type_mismatch: descriptor.pipe_transfer_type != descriptor.transfer_type,
    })
}
