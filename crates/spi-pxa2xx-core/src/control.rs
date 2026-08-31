// SPDX-License-Identifier: GPL-2.0-only
//! SSCR0/SSCR1 control-word encoding for Intel LPSS SPI.
//!
//! Ported from Linux `drivers/spi/spi-pxa2xx.c:292-306,1192-1233` and
//! definitions in `include/linux/pxa2xx_ssp.h:49-60,70-74`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use crate::regs::{sscr0, sscr1};

/// Why an SSCR control word was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlError {
    /// Linux advertises `SPI_BPW_RANGE_MASK(4, 32)` for LPSS.
    BitsPerWordBelowMinimum { bits: u8, minimum: u8 },
    /// Linux advertises `SPI_BPW_RANGE_MASK(4, 32)` for LPSS.
    BitsPerWordAboveMaximum { bits: u8, maximum: u8 },
    /// LPSS uses a 12-bit SCR field.
    ClockDividerAboveMaximum { divider: u16, maximum: u16 },
}

/// SPI mode inputs understood by the driver's setup path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SpiMode {
    pub cpha: bool,
    pub cpol: bool,
    pub loopback: bool,
}

/// Encode LPSS SSCR0 exactly as `pxa2xx_configure_sscr0` does.
///
/// For 4..16 bits DSS is `bits - 1`. For 17..32 bits Linux sets EDSS and
/// stores `bits - 16 - 1` in DSS (`spi-pxa2xx.c:301-304`). `divider` is the
/// unshifted SCR value returned by the clock math.
pub fn encode_sscr0(divider: u16, bits: u8) -> Result<u32, ControlError> {
    if bits < 4 {
        return Err(ControlError::BitsPerWordBelowMinimum { bits, minimum: 4 });
    }
    if bits > 32 {
        return Err(ControlError::BitsPerWordAboveMaximum { bits, maximum: 32 });
    }
    if divider > 0x0fff {
        return Err(ControlError::ClockDividerAboveMaximum {
            divider,
            maximum: 0x0fff,
        });
    }

    let dss_bits = if bits > 16 { bits - 16 } else { bits };
    let extended = if bits > 16 { sscr0::EDSS } else { 0 };
    Ok(((divider as u32) << sscr0::SCR_SHIFT) | sscr0::MOTOROLA | (dss_bits as u32 - 1) | extended)
}

/// Encode SSCR1's CPHA, CPOL and loopback mode bits.
///
/// This is the final host-mode part of Linux `setup`: CPHA maps to SPH, CPOL
/// maps to SPO, and SPI_LOOP maps to LBM (`spi-pxa2xx.c:1228-1233`).
pub fn encode_sscr1_mode(mode: SpiMode) -> u32 {
    (if mode.cpha { sscr1::SPH } else { 0 })
        | (if mode.cpol { sscr1::SPO } else { 0 })
        | (if mode.loopback { sscr1::LBM } else { 0 })
}
