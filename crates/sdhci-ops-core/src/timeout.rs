// SPDX-License-Identifier: GPL-2.0-only
//! TIMEOUT_CONTROL divisor calculation and decoding.
//!
//! Ported from Linux `sdhci_target_timeout()` and `sdhci_calc_timeout()` in
//! `drivers/mmc/host/sdhci.c:904-:1025`; the register write is deliberately left to the caller.
//!
//! Original copyright (C) 2005-2008 Pierre Ossman, All Rights Reserved, and the Linux SDHCI/MMC
//! authors.

/// `current_timeout = (1 << 13) * 1000 / host->timeout_clk`
/// (`drivers/mmc/host/sdhci.c:1002-:1011`). `timeout_clk` is in kHz, result in microseconds.
pub const TIMEOUT_BASE_KUS: u64 = (1 << 13) * 1000;

/// Source of the target timeout consumed by `sdhci_target_timeout()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutSpec {
    /// No data and no busy timeout: Linux chooses the maximum control value (`sdhci.c:988-:994`).
    Unspecified,
    /// A no-data command's `busy_timeout`, in milliseconds (`sdhci.c:912-:914`).
    CommandBusyMs(u32),
    /// Data timeout in nanoseconds plus card-clock cycles (`sdhci.c:915-:930`).
    Data {
        timeout_ns: u32,
        timeout_clks: u32,
        clock_hz: u32,
    },
}

/// Why timeout calculation refused an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeoutError {
    TimeoutClockIsZero { value_khz: u32, minimum_khz: u32 },
    TimeoutClockExceedsLinuxBound { value_khz: u32, max_khz: u32 },
    TimeoutCountExceedsShift { value: u8, max: u8 },
}

/// TIMEOUT_CONTROL value and whether Linux had to clamp it to the host maximum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutControl {
    pub count: u8,
    pub too_big: bool,
}

/// Calculate the timeout requested by a command, in microseconds.
///
/// Data nanoseconds and clock cycles are each rounded up independently as Linux does
/// (`drivers/mmc/host/sdhci.c:915-:930`). A zero card clock omits the cycle term, matching
/// `if (host->clock && data->timeout_clks)` at :918.
pub const fn target_timeout_us(spec: TimeoutSpec) -> Option<u64> {
    match spec {
        TimeoutSpec::Unspecified => None,
        TimeoutSpec::CommandBusyMs(ms) => Some(ms as u64 * 1000),
        TimeoutSpec::Data {
            timeout_ns,
            timeout_clks,
            clock_hz,
        } => {
            let ns_us = (timeout_ns as u64 + 999) / 1000;
            let clocks_us = if clock_hz != 0 && timeout_clks != 0 {
                (1_000_000u64 * timeout_clks as u64 + clock_hz as u64 - 1)
                    / clock_hz as u64
            } else {
                0
            };
            Some(ns_us + clocks_us)
        }
    }
}

/// Calculate Linux's TIMEOUT_CONTROL exponent/divisor.
///
/// `broken_timeout_value` and an unspecified target both select `max_timeout_count` immediately
/// (`drivers/mmc/host/sdhci.c:978-:994`). Otherwise count zero represents the base timeout and each
/// increment doubles it (`sdhci.c:1009-:1023`).
pub const fn timeout_control(
    timeout_clk_khz: u32,
    max_timeout_count: u8,
    broken_timeout_value: bool,
    spec: TimeoutSpec,
) -> Result<TimeoutControl, TimeoutError> {
    if broken_timeout_value || matches!(spec, TimeoutSpec::Unspecified) {
        return Ok(TimeoutControl {
            count: max_timeout_count,
            too_big: false,
        });
    }
    if timeout_clk_khz == 0 {
        return Err(TimeoutError::TimeoutClockIsZero {
            value_khz: 0,
            minimum_khz: 1,
        });
    }
    // Linux relies on host->timeout_clk < 2^16 to keep a nonzero base (sdhci.c:1005-:1010).
    if timeout_clk_khz > 65_535 {
        return Err(TimeoutError::TimeoutClockExceedsLinuxBound {
            value_khz: timeout_clk_khz,
            max_khz: 65_535,
        });
    }

    let target = match target_timeout_us(spec) {
        Some(value) => value,
        None => 0,
    };
    let mut count: u16 = 0;
    let mut current = TIMEOUT_BASE_KUS / timeout_clk_khz as u64;
    while current < target {
        count += 1;
        current = current.saturating_mul(2);
        if count > max_timeout_count as u16 {
            return Ok(TimeoutControl {
                count: max_timeout_count,
                too_big: true,
            });
        }
    }

    Ok(TimeoutControl {
        count: count as u8,
        too_big: false,
    })
}

/// Decode a TIMEOUT_CONTROL count to the represented hardware timeout in microseconds.
pub const fn decode_timeout_us(
    timeout_clk_khz: u32,
    count: u8,
) -> Result<u64, TimeoutError> {
    if timeout_clk_khz == 0 {
        return Err(TimeoutError::TimeoutClockIsZero {
            value_khz: 0,
            minimum_khz: 1,
        });
    }
    // Linux's derivation assumes `host->timeout_clk < 2^16` (sdhci.c:1005-:1010).
    if timeout_clk_khz > 65_535 {
        return Err(TimeoutError::TimeoutClockExceedsLinuxBound {
            value_khz: timeout_clk_khz,
            max_khz: 65_535,
        });
    }
    if count > 63 {
        return Err(TimeoutError::TimeoutCountExceedsShift {
            value: count,
            max: 63,
        });
    }
    let base = TIMEOUT_BASE_KUS / timeout_clk_khz as u64;
    Ok(base.saturating_mul(1u64 << count))
}
