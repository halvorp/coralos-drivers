// SPDX-License-Identifier: GPL-2.0-only
//! Clock-divider vectors worked from Linux `drivers/spi/spi-pxa2xx.c:888-920`.
//!
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use spi_pxa2xx_core::clock::{actual_rate_hz, divider, minimum_rate_hz, ClockError};

#[test]
fn divider_rounds_up_so_the_bus_never_exceeds_the_request() {
    // spi-pxa2xx.c:901-902: DIV_ROUND_UP(ssp_clk, rate) - 1.
    assert_eq!(divider(100_000_000, 25_000_000), Ok(3));
    assert_eq!(actual_rate_hz(100_000_000, 3), Ok(25_000_000));

    // 100 MHz / 30 MHz rounds UP to divisor 4, encoded SCR 3; actual is 25 MHz.
    assert_eq!(divider(100_000_000, 30_000_000), Ok(3));
    assert_eq!(actual_rate_hz(100_000_000, 3), Ok(25_000_000));
}

#[test]
fn requests_above_source_are_clamped_as_linux_does() {
    // spi-pxa2xx.c:893 min_t(int, ssp_clk, rate), then :902.
    assert_eq!(divider(100_000_000, 200_000_000), Ok(0));
    assert_eq!(actual_rate_hz(100_000_000, 0), Ok(100_000_000));
}

#[test]
fn twelve_bit_minimum_rate_is_pinned() {
    // spi-pxa2xx.c:1365-1367: DIV_ROUND_UP(max_speed_hz, 4096).
    assert_eq!(minimum_rate_hz(100_000_000), Ok(24_415));
    // At exactly that advertised minimum: ceil(100_000_000 / 24_415) = 4096, SCR = 4095.
    assert_eq!(divider(100_000_000, 24_415), Ok(0x0fff));
    assert_eq!(actual_rate_hz(100_000_000, 0x0fff), Ok(24_414));
}

#[test]
fn impossible_rates_are_named_not_masked_or_divided_by_zero() {
    assert_eq!(
        minimum_rate_hz(0),
        Err(ClockError::SourceClockBelowMinimum {
            source_hz: 0,
            minimum_hz: 1,
        })
    );
    assert_eq!(
        divider(0, 1),
        Err(ClockError::SourceClockBelowMinimum {
            source_hz: 0,
            minimum_hz: 1,
        })
    );
    assert_eq!(
        divider(100_000_000, 0),
        Err(ClockError::RequestedRateBelowMinimum {
            requested_hz: 0,
            minimum_hz: 1,
        })
    );
    assert_eq!(
        actual_rate_hz(0, 0),
        Err(ClockError::SourceClockBelowMinimum {
            source_hz: 0,
            minimum_hz: 1,
        })
    );
    assert_eq!(
        actual_rate_hz(100_000_000, 0x1000),
        Err(ClockError::DividerAboveMaximum {
            divider: 0x1000,
            maximum: 0x0fff,
        })
    );
    assert_eq!(
        divider(100_000_000, 24_414),
        Err(ClockError::RequestedRateBelowMinimum {
            requested_hz: 24_414,
            minimum_hz: 24_415,
        })
    );
}
