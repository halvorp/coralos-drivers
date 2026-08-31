// SPDX-License-Identifier: GPL-2.0-only
//! FIFO threshold selection and encoding.
//!
//! Ported from Linux `drivers/spi/spi-pxa2xx.c:80-169,988,1138-1225` and
//! `include/linux/pxa2xx_ssp.h:85-94,201-207`.
//!
//! Copyright (C) 2003 Russell King
//! Copyright (C) 2005 Stephen Street / StreetFire Sound Labs
//! Copyright (C) 2013, 2021 Intel Corporation

/// One LPSS entry from Linux's `lpss_platforms[]` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LpssConfig {
    pub name: &'static str,
    pub private_offset: u32,
    pub general_register: Option<u32>,
    pub ssp_register: u32,
    pub cs_control_register: u32,
    pub capabilities_register: Option<u32>,
    pub rx_threshold: u16,
    pub tx_threshold_low: u16,
    pub tx_threshold_high: u16,
    pub cs_select_shift: u8,
    pub cs_select_mask: u32,
    pub cs_clock_stays_gated: bool,
}

/// The six entries in Linux `lpss_platforms[]`, kept in enum order.
///
/// `spi-pxa2xx.c:99-169` requires this order to match the LPSS portion of
/// `enum pxa_ssp_type` in `include/linux/pxa2xx_ssp.h:225-231`.
pub const LPSS_PLATFORMS: [LpssConfig; 6] = [
    // spi-pxa2xx.c:100-169
    LpssConfig {
        name: "LPSS_LPT_SSP",         // spi-pxa2xx.c:101
        private_offset: 0x800,        // spi-pxa2xx.c:102
        general_register: Some(0x08), // spi-pxa2xx.c:103
        ssp_register: 0x0c,           // spi-pxa2xx.c:104
        cs_control_register: 0x18,    // spi-pxa2xx.c:105
        capabilities_register: None,  // spi-pxa2xx.c:106
        rx_threshold: 64,             // spi-pxa2xx.c:107
        tx_threshold_low: 160,        // spi-pxa2xx.c:108
        tx_threshold_high: 224,       // spi-pxa2xx.c:109
        cs_select_shift: 0,           // spi-pxa2xx.c:101-110, implicit zero
        cs_select_mask: 0,            // spi-pxa2xx.c:101-110, implicit zero
        cs_clock_stays_gated: false,  // spi-pxa2xx.c:101-110, implicit zero
    },
    LpssConfig {
        name: "LPSS_BYT_SSP",         // spi-pxa2xx.c:111
        private_offset: 0x400,        // spi-pxa2xx.c:112
        general_register: Some(0x08), // spi-pxa2xx.c:113
        ssp_register: 0x0c,           // spi-pxa2xx.c:114
        cs_control_register: 0x18,    // spi-pxa2xx.c:115
        capabilities_register: None,  // spi-pxa2xx.c:116
        rx_threshold: 64,             // spi-pxa2xx.c:117
        tx_threshold_low: 160,        // spi-pxa2xx.c:118
        tx_threshold_high: 224,       // spi-pxa2xx.c:119
        cs_select_shift: 0,           // spi-pxa2xx.c:111-120, implicit zero
        cs_select_mask: 0,            // spi-pxa2xx.c:111-120, implicit zero
        cs_clock_stays_gated: false,  // spi-pxa2xx.c:111-120, implicit zero
    },
    LpssConfig {
        name: "LPSS_BSW_SSP",         // spi-pxa2xx.c:121
        private_offset: 0x400,        // spi-pxa2xx.c:122
        general_register: Some(0x08), // spi-pxa2xx.c:123
        ssp_register: 0x0c,           // spi-pxa2xx.c:124
        cs_control_register: 0x18,    // spi-pxa2xx.c:125
        capabilities_register: None,  // spi-pxa2xx.c:126
        rx_threshold: 64,             // spi-pxa2xx.c:127
        tx_threshold_low: 160,        // spi-pxa2xx.c:128
        tx_threshold_high: 224,       // spi-pxa2xx.c:129
        cs_select_shift: 2,           // spi-pxa2xx.c:130
        cs_select_mask: 1 << 2,       // spi-pxa2xx.c:131
        cs_clock_stays_gated: false,  // spi-pxa2xx.c:121-132, implicit zero
    },
    LpssConfig {
        name: "LPSS_SPT_SSP",        // spi-pxa2xx.c:133
        private_offset: 0x200,       // spi-pxa2xx.c:134
        general_register: None,      // spi-pxa2xx.c:135
        ssp_register: 0x20,          // spi-pxa2xx.c:136
        cs_control_register: 0x24,   // spi-pxa2xx.c:137
        capabilities_register: None, // spi-pxa2xx.c:138
        rx_threshold: 1,             // spi-pxa2xx.c:139
        tx_threshold_low: 32,        // spi-pxa2xx.c:140
        tx_threshold_high: 56,       // spi-pxa2xx.c:141
        cs_select_shift: 0,          // spi-pxa2xx.c:133-142, implicit zero
        cs_select_mask: 0,           // spi-pxa2xx.c:133-142, implicit zero
        cs_clock_stays_gated: false, // spi-pxa2xx.c:133-142, implicit zero
    },
    LpssConfig {
        name: "LPSS_BXT_SSP",              // spi-pxa2xx.c:143
        private_offset: 0x200,             // spi-pxa2xx.c:144
        general_register: None,            // spi-pxa2xx.c:145
        ssp_register: 0x20,                // spi-pxa2xx.c:146
        cs_control_register: 0x24,         // spi-pxa2xx.c:147
        capabilities_register: Some(0xfc), // spi-pxa2xx.c:148
        rx_threshold: 1,                   // spi-pxa2xx.c:149
        tx_threshold_low: 16,              // spi-pxa2xx.c:150
        tx_threshold_high: 48,             // spi-pxa2xx.c:151
        cs_select_shift: 8,                // spi-pxa2xx.c:152
        cs_select_mask: 3 << 8,            // spi-pxa2xx.c:153
        cs_clock_stays_gated: true,        // spi-pxa2xx.c:154
    },
    LpssConfig {
        name: "LPSS_CNL_SSP",              // spi-pxa2xx.c:156
        private_offset: 0x200,             // spi-pxa2xx.c:157
        general_register: None,            // spi-pxa2xx.c:158
        ssp_register: 0x20,                // spi-pxa2xx.c:159
        cs_control_register: 0x24,         // spi-pxa2xx.c:160
        capabilities_register: Some(0xfc), // spi-pxa2xx.c:161
        rx_threshold: 1,                   // spi-pxa2xx.c:162
        tx_threshold_low: 32,              // spi-pxa2xx.c:163
        tx_threshold_high: 56,             // spi-pxa2xx.c:164
        cs_select_shift: 8,                // spi-pxa2xx.c:165
        cs_select_mask: 3 << 8,            // spi-pxa2xx.c:166
        cs_clock_stays_gated: true,        // spi-pxa2xx.c:167
    },
];

/// Why a threshold word was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdError {
    RxThresholdOutOfRange {
        threshold: u16,
        minimum: u16,
        maximum: u16,
    },
    TxLowThresholdOutOfRange {
        threshold: u16,
        minimum: u16,
        maximum: u16,
    },
    TxHighThresholdOutOfRange {
        threshold: u16,
        minimum: u16,
        maximum: u16,
    },
}

/// Why LPSS table selection was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    PlatformIndexOutOfRange { index: usize, platform_count: usize },
}

/// Fetch one Linux LPSS table entry by its enum-relative index.
pub fn lpss_config(index: usize) -> Result<&'static LpssConfig, ConfigError> {
    LPSS_PLATFORMS
        .get(index)
        .ok_or(ConfigError::PlatformIndexOutOfRange {
            index,
            platform_count: 6, // spi-pxa2xx.c:100-169
        })
}

/// Encode the ordinary SSCR1 RX/TX threshold fields.
///
/// Both fields encode `threshold - 1` and accept levels 1..16
/// (`include/linux/pxa2xx_ssp.h:91-94`).
pub fn encode_sscr1_thresholds(rx: u16, tx: u16) -> Result<u32, ThresholdError> {
    if !(1..=16).contains(&rx) {
        return Err(ThresholdError::RxThresholdOutOfRange {
            threshold: rx,
            minimum: 1,
            maximum: 16,
        });
    }
    if !(1..=16).contains(&tx) {
        return Err(ThresholdError::TxLowThresholdOutOfRange {
            threshold: tx,
            minimum: 1,
            maximum: 16,
        });
    }
    Ok((u32::from(rx - 1) << 10) | (u32::from(tx - 1) << 6))
}

/// Encode LPSS SSIRF's RX threshold (`SSIRF_RxThresh`).
pub fn encode_lpss_rx_threshold(rx: u16) -> Result<u16, ThresholdError> {
    if !(1..=256).contains(&rx) {
        return Err(ThresholdError::RxThresholdOutOfRange {
            threshold: rx,
            minimum: 1,
            maximum: 256,
        });
    }
    Ok(rx - 1)
}

/// Encode LPSS SSITF low/high TX thresholds.
///
/// Linux places `(low - 1)` at bit 8 and `(high - 1)` at bit 0
/// (`include/linux/pxa2xx_ssp.h:203-204`).
pub fn encode_lpss_tx_thresholds(low: u16, high: u16) -> Result<u16, ThresholdError> {
    if !(1..=256).contains(&low) {
        return Err(ThresholdError::TxLowThresholdOutOfRange {
            threshold: low,
            minimum: 1,
            maximum: 256,
        });
    }
    if !(1..=256).contains(&high) {
        return Err(ThresholdError::TxHighThresholdOutOfRange {
            threshold: high,
            minimum: 1,
            maximum: 256,
        });
    }
    Ok(((low - 1) << 8) | (high - 1))
}
