// SPDX-License-Identifier: GPL-2.0-only
//! Frozen LPSS platform and FIFO vectors from `drivers/spi/spi-pxa2xx.c` and
//! `include/linux/pxa2xx_ssp.h`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

use spi_pxa2xx_core::fifo::{
    encode_lpss_rx_threshold, encode_lpss_tx_thresholds, encode_sscr1_thresholds, lpss_config,
    ConfigError, ThresholdError, LPSS_PLATFORMS,
};

#[test]
fn all_six_linux_lpss_entries_and_names_are_pinned() {
    // spi-pxa2xx.c:100-169. This expected list is literal and independent of the production table.
    assert_eq!(LPSS_PLATFORMS.len(), 6);
    let names: Vec<&str> = LPSS_PLATFORMS.iter().map(|entry| entry.name).collect();
    assert_eq!(
        names,
        vec![
            "LPSS_LPT_SSP",
            "LPSS_BYT_SSP",
            "LPSS_BSW_SSP",
            "LPSS_SPT_SSP",
            "LPSS_BXT_SSP",
            "LPSS_CNL_SSP",
        ]
    );
}

#[test]
fn every_linux_lpss_table_literal_is_frozen() {
    // spi-pxa2xx.c:101-169. Tuples are offset, general, SSP, CS, capabilities,
    // RX, TX-low, TX-high, CS shift, CS mask, clock-gating quirk.
    let got: Vec<_> = LPSS_PLATFORMS
        .iter()
        .map(|p| {
            (
                p.private_offset,
                p.general_register,
                p.ssp_register,
                p.cs_control_register,
                p.capabilities_register,
                p.rx_threshold,
                p.tx_threshold_low,
                p.tx_threshold_high,
                p.cs_select_shift,
                p.cs_select_mask,
                p.cs_clock_stays_gated,
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            (
                0x800,
                Some(0x08),
                0x0c,
                0x18,
                None,
                64,
                160,
                224,
                0,
                0,
                false
            ),
            (
                0x400,
                Some(0x08),
                0x0c,
                0x18,
                None,
                64,
                160,
                224,
                0,
                0,
                false
            ),
            (
                0x400,
                Some(0x08),
                0x0c,
                0x18,
                None,
                64,
                160,
                224,
                2,
                0x4,
                false
            ),
            (0x200, None, 0x20, 0x24, None, 1, 32, 56, 0, 0, false),
            (
                0x200,
                None,
                0x20,
                0x24,
                Some(0xfc),
                1,
                16,
                48,
                8,
                0x300,
                true
            ),
            (
                0x200,
                None,
                0x20,
                0x24,
                Some(0xfc),
                1,
                32,
                56,
                8,
                0x300,
                true
            ),
        ]
    );
}

#[test]
fn table_lookup_has_a_vector_for_present_and_absent_indices() {
    assert_eq!(lpss_config(2).map(|p| p.name), Ok("LPSS_BSW_SSP"));
    assert_eq!(
        lpss_config(6),
        Err(ConfigError::PlatformIndexOutOfRange {
            index: 6,
            platform_count: 6,
        })
    );
}

#[test]
fn sscr1_fifo_thresholds_encode_linux_minus_one_fields() {
    // pxa2xx_ssp.h:85-94 and spi-pxa2xx.c:988. Defaults 8/8:
    // RX (7 << 10) | TX (7 << 6) = 0x1dc0.
    assert_eq!(encode_sscr1_thresholds(8, 8), Ok(0x1dc0));
    assert_eq!(encode_sscr1_thresholds(1, 1), Ok(0));
    assert_eq!(encode_sscr1_thresholds(16, 16), Ok(0x3fc0));
}

#[test]
fn cherry_trail_lpss_threshold_words_match_linux_literals() {
    // BSW values spi-pxa2xx.c:127-129; formulas pxa2xx_ssp.h:203-207.
    assert_eq!(encode_lpss_rx_threshold(64), Ok(0x003f));
    assert_eq!(encode_lpss_tx_thresholds(160, 224), Ok(0x9fdf));
}

#[test]
fn threshold_refusals_name_field_value_and_bounds() {
    assert_eq!(
        encode_sscr1_thresholds(0, 8),
        Err(ThresholdError::RxThresholdOutOfRange {
            threshold: 0,
            minimum: 1,
            maximum: 16
        })
    );
    assert_eq!(
        encode_sscr1_thresholds(8, 17),
        Err(ThresholdError::TxLowThresholdOutOfRange {
            threshold: 17,
            minimum: 1,
            maximum: 16
        })
    );
    assert_eq!(
        encode_lpss_rx_threshold(257),
        Err(ThresholdError::RxThresholdOutOfRange {
            threshold: 257,
            minimum: 1,
            maximum: 256
        })
    );
    assert_eq!(
        encode_lpss_tx_thresholds(0, 1),
        Err(ThresholdError::TxLowThresholdOutOfRange {
            threshold: 0,
            minimum: 1,
            maximum: 256
        })
    );
    assert_eq!(
        encode_lpss_tx_thresholds(1, 257),
        Err(ThresholdError::TxHighThresholdOutOfRange {
            threshold: 257,
            minimum: 1,
            maximum: 256
        })
    );
}
