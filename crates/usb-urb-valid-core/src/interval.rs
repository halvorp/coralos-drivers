// SPDX-License-Identifier: GPL-2.0-only
//! Periodic interval exponent conversion, speed/type bounds, and power-of-two normalization.
//!
//! Ported from Linux `include/linux/usb.h:1741-1790`,
//! `drivers/usb/core/devio.c:1900-1907`, and `drivers/usb/core/urb.c:535-584`.
//!
//! Copyright (C) the Linux USB core and Linux USB API authors.

use crate::{Speed, TransferType};

/// Why an interval was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalError {
    /// Periodic URBs require a positive interval (`drivers/usb/core/urb.c:546-548`).
    NotPositive { interval: i32, minimum: i32 },
    /// SuperSpeed accepts at most 2^(16-1) microframes (`drivers/usb/core/urb.c:552-557`).
    AboveMaximum {
        interval: u32,
        maximum: u32,
        speed: Speed,
        transfer_type: TransferType,
    },
}

/// Convert an endpoint descriptor's interval to the URB's linear units.
///
/// For interrupt endpoints this mirrors `usb_fill_int_urb`: high-speed and faster descriptors are
/// logarithmic, clamped to exponent 1..=16, while full/low-speed descriptors are already linear
/// (`include/linux/usb.h:1760-1787`). Isochronous descriptor conversion is provided separately by
/// [`decode_iso_descriptor_interval`].
pub const fn decode_descriptor_interval(
    speed: Speed,
    transfer_type: TransferType,
    descriptor_interval: u8,
) -> u32 {
    if matches!(transfer_type, TransferType::Interrupt)
        && matches!(speed, Speed::High | Speed::Super | Speed::SuperPlus)
    {
        decode_logarithmic_interval(descriptor_interval)
    } else {
        descriptor_interval as u32
    }
}

/// Convert an isochronous endpoint descriptor's logarithmic interval at any wired speed.
///
/// Linux tests ISO before speed and stores `1 << min(15, bInterval - 1)`
/// (`drivers/usb/core/devio.c:1900-1907`). Call this only for a descriptor already validated to
/// have nonzero `bInterval`, as Linux does.
pub const fn decode_iso_descriptor_interval(descriptor_interval: u8) -> u32 {
    if descriptor_interval == 0 {
        return 0;
    }
    let shift = descriptor_interval.saturating_sub(1);
    1u32 << if shift < 15 { shift } else { 15 }
}

const fn decode_logarithmic_interval(descriptor_interval: u8) -> u32 {
    let exponent = if descriptor_interval < 1 {
        1
    } else if descriptor_interval > 16 {
        16
    } else {
        descriptor_interval
    };
    1u32 << (exponent - 1)
}

/// Encode a linear URB interval as the descriptor exponent that decodes to Linux's normalized
/// power of two. Values above the descriptor range saturate at exponent 16.
///
/// The `+ 1` is load-bearing: Linux decodes with `1 << (interval - 1)`
/// (`include/linux/usb.h:1781-1785`). Omitting it doubles every requested polling rate.
pub const fn encode_descriptor_exponent(linear_interval: u32) -> u8 {
    if linear_interval <= 1 {
        1
    } else {
        let floor_log2 = 31 - linear_interval.leading_zeros();
        let exponent = floor_log2 + 1;
        if exponent > 16 {
            16
        } else {
            exponent as u8
        }
    }
}

/// Linux's submission-time interval normalization.
///
/// Interrupt and isochronous intervals are rounded DOWN to a power of two after applying the
/// speed/type-specific limits (`drivers/usb/core/urb.c:543-584`). Control and bulk intervals are
/// untouched because Linux's periodic switch does not enter for them.
pub const fn normalize_interval(
    speed: Speed,
    transfer_type: TransferType,
    interval: i32,
) -> Result<i32, IntervalError> {
    if !matches!(
        transfer_type,
        TransferType::Interrupt | TransferType::Isochronous
    ) {
        return Ok(interval);
    }
    if interval <= 0 {
        return Err(IntervalError::NotPositive {
            interval,
            minimum: 1,
        });
    }

    let requested = interval as u32;
    let maximum = match speed {
        Speed::Super | Speed::SuperPlus => {
            if requested > (1 << 15) {
                return Err(IntervalError::AboveMaximum {
                    interval: requested,
                    maximum: 1 << 15,
                    speed,
                    transfer_type,
                });
            }
            1 << 15
        }
        Speed::High => 1024 * 8,
        Speed::Full | Speed::Low => match transfer_type {
            TransferType::Interrupt => {
                if requested > 255 {
                    return Err(IntervalError::AboveMaximum {
                        interval: requested,
                        maximum: 255,
                        speed,
                        transfer_type,
                    });
                }
                128
            }
            TransferType::Isochronous => 1024,
            TransferType::Control | TransferType::Bulk => unreachable!(),
        },
    };

    // `1 << ilog2(interval)`, capped by `max` (drivers/usb/core/urb.c:582-583).
    let rounded = 1u32 << (31 - requested.leading_zeros());
    Ok(if rounded < maximum { rounded } else { maximum } as i32)
}
