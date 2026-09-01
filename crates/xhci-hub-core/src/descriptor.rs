// SPDX-License-Identifier: GPL-2.0-only
//! USB2 and USB3 root-hub descriptor construction from caller-supplied PORTSC snapshots.
//!
//! Ported from Linux `drivers/usb/host/xhci-hub.c`, by Sarah Sharp and the Linux xHCI authors.
//! Descriptor constants are from Linux `include/uapi/linux/usb/ch11.h`.
//! Original copyright: Copyright (C) 2008 Intel Corp.

use core::fmt;

use crate::portsc::PORT_DEV_REMOVE;

pub const USB2_DESCRIPTOR_MAX_BYTES: usize = 15; // ch11.h:22/:270-:274, USB_MAXCHILDREN = 31
pub const USB3_DESCRIPTOR_BYTES: usize = 12; // ch11.h:247

/// A fixed-capacity USB2 descriptor; only `len` bytes are part of the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usb2HubDescriptor {
    pub bytes: [u8; USB2_DESCRIPTOR_MAX_BYTES],
    pub len: usize,
}

/// The fixed USB3 root-hub descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usb3HubDescriptor {
    pub bytes: [u8; USB3_DESCRIPTOR_BYTES],
}

/// Named descriptor construction refusals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DescriptorError {
    PortCountOutOfRange { value: usize, maximum: usize },
    MissingPortStatus { ports: usize, supplied: usize },
}

impl fmt::Display for DescriptorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PortCountOutOfRange { value, maximum } => write!(f, "hub descriptor port count {value} exceeds maximum {maximum}"),
            Self::MissingPortStatus { ports, supplied } => write!(f, "hub descriptor needs {ports} PORTSC values but caller supplied {supplied}"),
        }
    }
}

/// Common `wHubCharacteristics` (xhci-hub.c:256-:275).
pub const fn hub_characteristics(per_port_power: bool) -> u16 {
    let power = if per_port_power { 0x0001 } else { 0x0002 }; // ch11.h:209-:210
    power | 0x0008 // HUB_CHAR_INDV_PORT_OCPM, ch11.h:216
}

fn validate(ports: usize, statuses: &[u32], maximum: usize) -> Result<(), DescriptorError> {
    if ports > maximum {
        return Err(DescriptorError::PortCountOutOfRange { value: ports, maximum });
    }
    if statuses.len() < ports {
        return Err(DescriptorError::MissingPortStatus { ports, supplied: statuses.len() });
    }
    Ok(())
}

/// Construct Linux's USB2 root-hub descriptor (xhci-hub.c:278-:331).
pub fn usb2_hub_descriptor(ports: usize, per_port_power: bool, statuses: &[u32]) -> Result<Usb2HubDescriptor, DescriptorError> {
    validate(ports, statuses, 31)?; // USB_MAXCHILDREN, ch11.h:279
    let bitmap_bytes = 1 + ports / 8; // xhci-hub.c:293
    let len = 7 + 2 * bitmap_bytes; // xhci-hub.c:294
    let mut bytes = [0xff; USB2_DESCRIPTOR_MAX_BYTES];
    bytes[0] = len as u8;
    bytes[1] = 0x29; // USB_DT_HUB, ch11.h:244
    bytes[2] = ports as u8;
    let chars = hub_characteristics(per_port_power).to_le_bytes();
    bytes[3] = chars[0];
    bytes[4] = chars[1];
    bytes[5] = 10; // xhci-hub.c:295
    bytes[6] = 0; // bHubContrCurrent, xhci-hub.c:261

    // DeviceRemovable starts at byte 7. PortPwrCtrlMask follows it and remains 0xff.
    for byte in &mut bytes[7..7 + bitmap_bytes] {
        *byte = 0;
    }
    for (i, portsc) in statuses[..ports].iter().enumerate() {
        if portsc & PORT_DEV_REMOVE != 0 {
            bytes[7 + (i + 1) / 8] |= 1 << ((i + 1) % 8); // xhci-hub.c:306-:310
        }
    }
    Ok(Usb2HubDescriptor { bytes, len })
}

/// Construct Linux's USB3 root-hub descriptor (xhci-hub.c:333-:365).
pub fn usb3_hub_descriptor(ports: usize, per_port_power: bool, statuses: &[u32]) -> Result<Usb3HubDescriptor, DescriptorError> {
    validate(ports, statuses, 15)?; // DeviceRemovable is u16 with bit zero reserved, xhci-hub.c:338/:357
    let mut bytes = [0u8; USB3_DESCRIPTOR_BYTES];
    bytes[0] = 12; // USB_DT_SS_HUB_SIZE, ch11.h:247
    bytes[1] = 0x2a; // USB_DT_SS_HUB, ch11.h:245
    bytes[2] = ports as u8;
    let chars = hub_characteristics(per_port_power).to_le_bytes();
    bytes[3] = chars[0];
    bytes[4] = chars[1];
    bytes[5] = 50; // xhci-hub.c:348
    bytes[6] = 0; // bHubContrCurrent, xhci-hub.c:261
    bytes[7] = 0; // bHubHdrDecLat, xhci-hub.c:353
    // bytes 8:10 wHubDelay remain zero, xhci-hub.c:354.
    let mut removable = 0u16;
    for (i, portsc) in statuses[..ports].iter().enumerate() {
        if portsc & PORT_DEV_REMOVE != 0 {
            removable |= 1 << (i + 1); // xhci-hub.c:356-:361
        }
    }
    let removable = removable.to_le_bytes();
    bytes[10] = removable[0];
    bytes[11] = removable[1];
    Ok(Usb3HubDescriptor { bytes })
}
