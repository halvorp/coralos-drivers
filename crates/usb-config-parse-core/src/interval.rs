// SPDX-License-Identifier: GPL-2.0-only
//! Linux-compatible endpoint `bInterval` validation and encoding.
//!
//! Ported from Linux `drivers/usb/core/config.c:363-433` and speed names from
//! `include/uapi/linux/usb/ch9.h:1201-1208`.
//! Original Linux notice: "Released under the GPLv2 only." Copyright belongs to the Linux USB
//! core and Chapter 9 header authors and contributors.

use crate::decode::TransferType;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speed {
    Low,
    Full,
    High,
    Super,
    SuperPlus,
}

/// Wired speed names handled by Linux's interval switch.
pub const WIRED_SPEEDS: &[(&str, Speed)] = &[
    ("LOW", Speed::Low),              // include/uapi/linux/usb/ch9.h:1203
    ("FULL", Speed::Full),            // include/uapi/linux/usb/ch9.h:1203
    ("HIGH", Speed::High),            // include/uapi/linux/usb/ch9.h:1204
    ("SUPER", Speed::Super),          // include/uapi/linux/usb/ch9.h:1206
    ("SUPER_PLUS", Speed::SuperPlus), // include/uapi/linux/usb/ch9.h:1207
];

/// Interrupt interval quirk interpretation from `config.c:386-403`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptIntervalEncoding {
    Standard,
    LinearFrames,
    LinearMicroframes,
}

/// Result of Linux's `bInterval` range validation (`config.c:363-433`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub encoded: u8,
    pub replaced: bool,
}

/// Validate and encode `bInterval` for speed and transfer type.
///
/// Invalid full/low-speed interrupt values become 10 ms; invalid high-or-faster interrupt values
/// become `fls(raw * 8)` (or 7 for zero); invalid high-speed isochronous values become 7, and the
/// other isochronous speeds become 4 (`config.c:369-432`).
pub fn encode_interval(
    speed: Speed,
    transfer_type: TransferType,
    raw: u8,
    interrupt_encoding: InterruptIntervalEncoding,
) -> Interval {
    let (minimum, maximum, fallback) = match transfer_type {
        TransferType::Interrupt => match speed {
            Speed::High | Speed::Super | Speed::SuperPlus => {
                let standard = {
                    let n = fls((raw as u32) * 8);
                    if n == 0 {
                        7
                    } else {
                        n as u8
                    }
                };
                match interrupt_encoding {
                    InterruptIntervalEncoding::Standard => (1, 16, standard),
                    InterruptIntervalEncoding::LinearFrames => {
                        let n = (fls(raw as u32) as u8).saturating_add(3).clamp(1, 16);
                        (n, n, n)
                    }
                    InterruptIntervalEncoding::LinearMicroframes => {
                        let n = (fls(raw as u32) as u8).clamp(1, 16);
                        (n, n, n)
                    }
                }
            }
            Speed::Low | Speed::Full => (1, 255, 10),
        },
        TransferType::Isochronous => {
            let fallback = if speed == Speed::High { 7 } else { 4 };
            (1, 16, fallback)
        }
        TransferType::Control | TransferType::Bulk => (0, 255, 0),
    };

    if raw < minimum || raw > maximum {
        Interval {
            encoded: fallback,
            replaced: true,
        }
    } else {
        Interval {
            encoded: raw,
            replaced: false,
        }
    }
}

const fn fls(value: u32) -> u32 {
    u32::BITS - value.leading_zeros()
}
