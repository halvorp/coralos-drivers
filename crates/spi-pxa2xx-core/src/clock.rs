// SPDX-License-Identifier: GPL-2.0-only
//! LPSS SSP clock-divider and actual bit-rate arithmetic.
//!
//! Ported from Linux `drivers/spi/spi-pxa2xx.c:888-920,1013-1023,1360-1369`.
//!
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

/// Why a requested bit rate was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockError {
    SourceClockBelowMinimum {
        source_hz: u32,
        minimum_hz: u32,
    },
    RequestedRateBelowMinimum {
        requested_hz: u32,
        minimum_hz: u32,
    },
    /// LPSS's SCR field is 12 bits (`spi-pxa2xx.c:902`).
    DividerAboveMaximum {
        divider: u16,
        maximum: u16,
    },
}

const fn div_round_up(numerator: u32, denominator: u32) -> u32 {
    numerator / denominator + if numerator % denominator != 0 { 1 } else { 0 }
}

/// Slowest rate representable by the LPSS 12-bit divider.
///
/// Linux sets this to `DIV_ROUND_UP(max_speed_hz, 4096)` for non-PXA25x
/// controllers (`spi-pxa2xx.c:1365-1367`).
pub fn minimum_rate_hz(source_hz: u32) -> Result<u32, ClockError> {
    if source_hz == 0 {
        return Err(ClockError::SourceClockBelowMinimum {
            source_hz,
            minimum_hz: 1,
        });
    }
    Ok(div_round_up(source_hz, 4096))
}

/// Compute the unshifted LPSS SCR value.
///
/// This is Linux's non-PXA25x branch:
/// `(DIV_ROUND_UP(ssp_clk, min(ssp_clk, rate)) - 1) & 0xfff`
/// (`spi-pxa2xx.c:893,901-902`). Requests above the source clock therefore
/// produce divider zero; too-slow requests are named rather than silently
/// wrapping through the Linux mask.
pub fn divider(source_hz: u32, requested_hz: u32) -> Result<u16, ClockError> {
    if source_hz == 0 {
        return Err(ClockError::SourceClockBelowMinimum {
            source_hz,
            minimum_hz: 1,
        });
    }
    if requested_hz == 0 {
        return Err(ClockError::RequestedRateBelowMinimum {
            requested_hz,
            minimum_hz: 1,
        });
    }
    let minimum_hz = div_round_up(source_hz, 4096);
    if requested_hz < minimum_hz {
        return Err(ClockError::RequestedRateBelowMinimum {
            requested_hz,
            minimum_hz,
        });
    }
    let rate = requested_hz.min(source_hz);
    Ok((div_round_up(source_hz, rate) - 1) as u16)
}

/// Return the actual SCLK produced by an unshifted LPSS SCR value.
///
/// Linux reports `max_speed_hz / (1 + SCR)` (`spi-pxa2xx.c:1015-1018`).
pub fn actual_rate_hz(source_hz: u32, divider: u16) -> Result<u32, ClockError> {
    if source_hz == 0 {
        return Err(ClockError::SourceClockBelowMinimum {
            source_hz,
            minimum_hz: 1,
        });
    }
    if divider > 0x0fff {
        return Err(ClockError::DividerAboveMaximum {
            divider,
            maximum: 0x0fff,
        });
    }
    Ok(source_hz / (u32::from(divider) + 1))
}
