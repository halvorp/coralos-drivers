// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for `sdhci_target_timeout()` and `sdhci_calc_timeout()` from
//! `drivers/mmc/host/sdhci.c:904-:1025`. Original copyright (C) 2005-2008 Pierre Ossman and Linux
//! SDHCI/MMC authors.

use sdhci_ops_core::timeout::{
    decode_timeout_us, target_timeout_us, timeout_control, TimeoutControl, TimeoutError, TimeoutSpec,
    TIMEOUT_BASE_KUS,
};

/// `drivers/mmc/host/sdhci.c:1010` — `(1 << 13) * 1000`, in kHz·microseconds.
#[test]
fn timeout_base_is_pinned_to_the_linux_literal() {
    assert_eq!(TIMEOUT_BASE_KUS, 8_192_000);
}

/// sdhci.c:912-:930: command milliseconds become microseconds; data ns and cycles round up.
#[test]
fn target_timeout_units_and_rounding_match_linux() {
    assert_eq!(target_timeout_us(TimeoutSpec::Unspecified), None);
    assert_eq!(target_timeout_us(TimeoutSpec::CommandBusyMs(9)), Some(9_000));
    assert_eq!(
        target_timeout_us(TimeoutSpec::Data {
            timeout_ns: 1_001,
            timeout_clks: 3,
            clock_hz: 2_000_000,
        }),
        Some(4)
    ); // ceil(1001/1000)=2 plus ceil(3*1e6/2e6)=2
    assert_eq!(
        target_timeout_us(TimeoutSpec::Data {
            timeout_ns: 1_001,
            timeout_clks: 3,
            clock_hz: 0,
        }),
        Some(2)
    ); // sdhci.c:918: zero host clock omits timeout_clks
}

/// sdhci.c:1009-:1023: base = 8192*1000/1000 = 8192 us, then each count doubles. A 20 ms eMMC
/// timeout therefore needs count 2: 8192, 16384, 32768. Pin the divisor by value AND decode it.
#[test]
fn emmc_timeout_divisor_is_pinned_by_value_and_decode_round_trip() {
    let control = timeout_control(1_000, 0x0e, false, TimeoutSpec::CommandBusyMs(20)).unwrap();
    assert_eq!(control, TimeoutControl { count: 0x02, too_big: false });
    assert_eq!(decode_timeout_us(1_000, 0x02), Ok(32_768));
    let represented = decode_timeout_us(1_000, control.count).unwrap();
    assert!(represented >= 20_000);
    assert_eq!(decode_timeout_us(1_000, control.count - 1), Ok(16_384));
}

/// Exact boundary catches `<` versus `<=`: Linux does not increment when current == target.
#[test]
fn exact_timeout_boundary_keeps_the_lower_count() {
    assert_eq!(
        timeout_control(1_000, 0x0e, false, TimeoutSpec::CommandBusyMs(16)).unwrap(),
        TimeoutControl { count: 1, too_big: false }
    );
    assert_eq!(decode_timeout_us(1_000, 1), Ok(16_384));
}

/// sdhci.c:978-:994 and :1014-:1021: broken/unspecified chooses max; overlarge clamps and reports.
#[test]
fn maximum_paths_match_linux_and_are_distinguishable() {
    assert_eq!(
        timeout_control(0, 0x0e, true, TimeoutSpec::CommandBusyMs(1)).unwrap(),
        TimeoutControl { count: 0x0e, too_big: false }
    );
    assert_eq!(
        timeout_control(0, 0x0e, false, TimeoutSpec::Unspecified).unwrap(),
        TimeoutControl { count: 0x0e, too_big: false }
    );
    assert_eq!(
        timeout_control(1_000, 2, false, TimeoutSpec::CommandBusyMs(100)).unwrap(),
        TimeoutControl { count: 2, too_big: true }
    );
}

#[test]
fn invalid_timeout_fields_name_the_refused_value_and_bound() {
    let zero = TimeoutError::TimeoutClockIsZero { value_khz: 0, minimum_khz: 1 };
    assert_eq!(timeout_control(0, 0x0e, false, TimeoutSpec::CommandBusyMs(1)), Err(zero));
    assert_eq!(decode_timeout_us(0, 2), Err(zero));

    let wide_clock = TimeoutError::TimeoutClockExceedsLinuxBound {
        value_khz: 65_536,
        max_khz: 65_535,
    };
    assert_eq!(
        timeout_control(65_536, 0x0e, false, TimeoutSpec::CommandBusyMs(1)),
        Err(wide_clock)
    );
    assert_eq!(decode_timeout_us(65_536, 2), Err(wide_clock));
    assert_eq!(
        decode_timeout_us(1_000, 64),
        Err(TimeoutError::TimeoutCountExceedsShift { value: 64, max: 63 })
    );
}
