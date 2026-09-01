// SPDX-License-Identifier: GPL-2.0-only
//! Linux's legacy endpoint `pipe` cookie, encoded and decoded without hardware access.
//!
//! Ported from Linux `include/linux/usb.h:1987-:2042`.
//!
//! Copyright (C) the Linux USB core authors.

use crate::request::Direction;

pub const DIRECTION_MASK: u32 = 0x80; // include/linux/usb.h:1993,2008
pub const DEVICE_MASK: u32 = 0x7f00; // include/linux/usb.h:1995,2011
pub const ENDPOINT_MASK: u32 = 0x0007_8000; // include/linux/usb.h:1996,2012
pub const TYPE_MASK: u32 = 0xc000_0000; // include/linux/usb.h:1997,2014

/// Pipe type values. Linux warns these are not USB endpoint transfer-type values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PipeType {
    Isochronous = 0, // include/linux/usb.h:2003
    Interrupt = 1, // include/linux/usb.h:2004
    Control = 2, // include/linux/usb.h:2005
    Bulk = 3, // include/linux/usb.h:2006
}

/// A named Linux pipe-type encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NamedPipeType {
    pub name: &'static str,
    pub value: u8,
}

/// All four pipe types Linux defines, in source order.
pub const PIPE_TYPES: &[NamedPipeType] = &[
    NamedPipeType { name: "ISOCHRONOUS", value: 0 }, // include/linux/usb.h:2003
    NamedPipeType { name: "INTERRUPT", value: 1 }, // include/linux/usb.h:2004
    NamedPipeType { name: "CONTROL", value: 2 }, // include/linux/usb.h:2005
    NamedPipeType { name: "BULK", value: 3 }, // include/linux/usb.h:2006
];

/// A refusal to create a pipe whose value would escape Linux's allocated fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeError {
    DeviceAddressOutOfRange { value: u8, maximum: u8 },
    EndpointOutOfRange { value: u8, maximum: u8 },
}

/// Encode Linux's pipe cookie (`__create_pipe` plus type and direction).
pub const fn encode(
    device_address: u8,
    endpoint: u8,
    direction: Direction,
    pipe_type: PipeType,
) -> Result<u32, PipeError> {
    if device_address > 0x7f {
        return Err(PipeError::DeviceAddressOutOfRange { value: device_address, maximum: 0x7f });
    }
    if endpoint > 0x0f {
        return Err(PipeError::EndpointOutOfRange { value: endpoint, maximum: 0x0f });
    }
    Ok((((pipe_type as u32) << 30) & TYPE_MASK)
        | (((endpoint as u32) << 15) & ENDPOINT_MASK)
        | (((device_address as u32) << 8) & DEVICE_MASK)
        | (direction as u32 & DIRECTION_MASK))
}

/// Encode `usb_sndctrlpipe`.
pub const fn send_control(device_address: u8, endpoint: u8) -> Result<u32, PipeError> {
    encode(device_address, endpoint, Direction::Out, PipeType::Control)
}

/// Encode `usb_rcvctrlpipe`.
pub const fn receive_control(device_address: u8, endpoint: u8) -> Result<u32, PipeError> {
    encode(device_address, endpoint, Direction::In, PipeType::Control)
}

pub const fn direction(pipe: u32) -> Direction {
    if pipe & DIRECTION_MASK != 0 { Direction::In } else { Direction::Out }
}
pub const fn device_address(pipe: u32) -> u8 { ((pipe & DEVICE_MASK) >> 8) as u8 }
pub const fn endpoint(pipe: u32) -> u8 { ((pipe & ENDPOINT_MASK) >> 15) as u8 }
pub const fn pipe_type(pipe: u32) -> PipeType {
    match (pipe & TYPE_MASK) >> 30 {
        0 => PipeType::Isochronous,
        1 => PipeType::Interrupt,
        2 => PipeType::Control,
        _ => PipeType::Bulk,
    }
}
