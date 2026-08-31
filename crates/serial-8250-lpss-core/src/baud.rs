// SPDX-License-Identifier: GPL-2.0-only
//! DesignWare integer/fractional baud divisor arithmetic from Linux `8250_dwlib.c:71-:94`.
//!
//! Copyright 2011 Picochip, Jamie Iles; Copyright 2013 Intel Corporation and the Synopsys
//! DesignWare 8250 authors.

/// The integer DLL/DLH divisor and the value for `DW_UART_DLF`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divisor {
    pub integer: u32,
    pub fractional: u32,
}

/// A named refusal to construct a divisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivisorError {
    /// `baud * 16` cannot be represented.
    BaudBaseOverflow { baud: u32, maximum_baud: u32 },
    /// A zero baud would divide by zero. Linux's LPSS B0 fallback belongs in `lpss::clock_plan`.
    BaudIsZero { baud: u32 },
    /// `rem << dlf_size` in Linux is defined only for widths below 32.
    FractionWidthOutOfRange { dlf_size: u8, maximum: u8 },
    /// The integer latch would be zero, so this clock cannot generate the requested baud.
    UartClockBelowBaudBase { uartclk_hz: u32, baud_base_hz: u32 },
}

/// Port of `dw8250_get_divisor` (`8250_dwlib.c:83-:94`).
///
/// Linux computes `quot = uartclk / (baud * 16)` and
/// `frac = DIV_ROUND_CLOSEST((uartclk % (baud * 16)) << dlf_size, baud * 16)`.
pub fn divisor(uartclk_hz: u32, baud: u32, dlf_size: u8) -> Result<Divisor, DivisorError> {
    if baud == 0 {
        return Err(DivisorError::BaudIsZero { baud });
    }
    if dlf_size > 31 {
        return Err(DivisorError::FractionWidthOutOfRange {
            dlf_size,
            maximum: 31,
        });
    }
    let baud_base = baud.checked_mul(16).ok_or(DivisorError::BaudBaseOverflow {
        baud,
        maximum_baud: u32::MAX / 16,
    })?;
    if uartclk_hz < baud_base {
        return Err(DivisorError::UartClockBelowBaudBase {
            uartclk_hz,
            baud_base_hz: baud_base,
        });
    }

    let integer = uartclk_hz / baud_base;
    let remainder = uartclk_hz % baud_base;
    let numerator = (remainder as u64) << dlf_size;
    let fractional = ((numerator + baud_base as u64 / 2) / baud_base as u64) as u32;
    Ok(Divisor {
        integer,
        fractional,
    })
}
