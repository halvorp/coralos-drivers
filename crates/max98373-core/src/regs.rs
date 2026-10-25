// SPDX-License-Identifier: GPL-2.0-only
//! The registers of the MAX98373 this crate's plans touch, and their fields. Ported from Linux `sound/soc/codecs/max98373.h` (Copyright (c)
//! 2017 Maxim Integrated, `GPL-2.0-only`); every constant carries the FILE and the LINE it came from. The regmap is 16-bit register
//! addresses, 8-bit values (`max98373-i2c.c`, `max98373_regmap`: `reg_bits = 16`, `val_bits = 8`).

pub const R2000_SW_RESET: u16 = 0x2000; // max98373.h:7
pub const R2020_PCM_TX_HIZ_EN_1: u16 = 0x2020; // max98373.h:30
pub const R2021_PCM_TX_HIZ_EN_2: u16 = 0x2021; // max98373.h:31
pub const R2022_PCM_TX_SRC_1: u16 = 0x2022; // max98373.h:32
pub const R2023_PCM_TX_SRC_2: u16 = 0x2023; // max98373.h:33
pub const R2024_PCM_DATA_FMT_CFG: u16 = 0x2024; // max98373.h:34
pub const R2026_PCM_CLOCK_RATIO: u16 = 0x2026; // max98373.h:36
pub const R2027_PCM_SR_SETUP_1: u16 = 0x2027; // max98373.h:37
pub const R2028_PCM_SR_SETUP_2: u16 = 0x2028; // max98373.h:38
pub const R2029_PCM_TO_SPK_MONO_MIX_1: u16 = 0x2029; // max98373.h:39
pub const R202A_PCM_TO_SPK_MONO_MIX_2: u16 = 0x202A; // max98373.h:40
pub const R202B_PCM_RX_EN: u16 = 0x202B; // max98373.h:41
pub const R202C_PCM_TX_EN: u16 = 0x202C; // max98373.h:42
pub const R203D_AMP_DIG_VOL_CTRL: u16 = 0x203D; // max98373.h:51
pub const R203E_AMP_PATH_GAIN: u16 = 0x203E; // max98373.h:52
pub const R203F_AMP_DSP_CFG: u16 = 0x203F; // max98373.h:53
pub const R2043_AMP_EN: u16 = 0x2043; // max98373.h:57
pub const R2046_IV_SENSE_ADC_DSP_CFG: u16 = 0x2046; // max98373.h:58
pub const R2090_BDE_LVL_HOLD: u16 = 0x2090; // max98373.h:66
pub const R2091_BDE_GAIN_ATK_REL_RATE: u16 = 0x2091; // max98373.h:67
pub const R2097_BDE_L1_THRESH: u16 = 0x2097; // max98373.h:69
pub const R2098_BDE_L2_THRESH: u16 = 0x2098; // max98373.h:70
pub const R2099_BDE_L3_THRESH: u16 = 0x2099; // max98373.h:71
pub const R209A_BDE_L4_THRESH: u16 = 0x209A; // max98373.h:72
pub const R209B_BDE_THRESH_HYST: u16 = 0x209B; // max98373.h:73
pub const R20A8_BDE_L1_CFG_1: u16 = 0x20A8; // max98373.h:74
pub const R20A9_BDE_L1_CFG_2: u16 = 0x20A9; // max98373.h:75
pub const R20AA_BDE_L1_CFG_3: u16 = 0x20AA; // max98373.h:76
pub const R20AB_BDE_L2_CFG_1: u16 = 0x20AB; // max98373.h:77
pub const R20AC_BDE_L2_CFG_2: u16 = 0x20AC; // max98373.h:78
pub const R20AD_BDE_L2_CFG_3: u16 = 0x20AD; // max98373.h:79
pub const R20AE_BDE_L3_CFG_1: u16 = 0x20AE; // max98373.h:80
pub const R20AF_BDE_L3_CFG_2: u16 = 0x20AF; // max98373.h:81
pub const R20B0_BDE_L3_CFG_3: u16 = 0x20B0; // max98373.h:82
pub const R20B1_BDE_L4_CFG_1: u16 = 0x20B1; // max98373.h:83
pub const R20B2_BDE_L4_CFG_2: u16 = 0x20B2; // max98373.h:84
pub const R20B3_BDE_L4_CFG_3: u16 = 0x20B3; // max98373.h:85
pub const R20B5_BDE_EN: u16 = 0x20B5; // max98373.h:87
pub const R20D1_DHT_CFG: u16 = 0x20D1; // max98373.h:89
pub const R20D2_DHT_ATTACK_CFG: u16 = 0x20D2; // max98373.h:90
pub const R20D3_DHT_RELEASE_CFG: u16 = 0x20D3; // max98373.h:91
pub const R20D4_DHT_EN: u16 = 0x20D4; // max98373.h:92
pub const R20FE_DEVICE_AUTO_RESTART_CFG: u16 = 0x20FE; // max98373.h:96
pub const R20FF_GLOBAL_SHDN: u16 = 0x20FF; // max98373.h:97
pub const R21FF_REV_ID: u16 = 0x21FF; // max98373.h:98

// ---- fields (max98373.h) ----
pub const SOFT_RESET: u8 = 0x1; // max98373.h:207
pub const PCM_TX_CH_SRC_A_V_SHIFT: u8 = 0; // max98373.h:101
pub const PCM_TX_CH_SRC_A_I_SHIFT: u8 = 4; // max98373.h:102
pub const PCM_MODE_CFG_FORMAT_MASK: u8 = 0x7 << 3; // max98373.h:105
pub const PCM_MODE_CFG_FORMAT_SHIFT: u8 = 3; // max98373.h:106
pub const PCM_FORMAT_TDM_MODE0: u8 = 0x3; // max98373.h:110
pub const PCM_MODE_CFG_CHANSZ_MASK: u8 = 0x3 << 6; // max98373.h:113
pub const PCM_MODE_CFG_CHANSZ_16: u8 = 0x1 << 6; // max98373.h:114
pub const PCM_MODE_CFG_PCM_BCLKEDGE: u8 = 0x1 << 4; // max98373.h:119
pub const PCM_CLK_SETUP_BSEL_MASK: u8 = 0xF; // max98373.h:120
pub const PCM_SR_SET1_SR_MASK: u8 = 0xF; // max98373.h:123
pub const PCM_SR_SET1_SR_48000: u8 = 0x8; // max98373.h:132
pub const PCM_SR_SET2_SR_MASK: u8 = 0xF << 4; // max98373.h:137
pub const PCM_SR_SET2_SR_SHIFT: u8 = 4; // max98373.h:138
pub const PCM_SR_SET2_IVADC_SR_MASK: u8 = 0xF; // max98373.h:139
pub const PCM_TO_SPK_MONOMIX_CFG_MASK: u8 = 0x3 << 6; // max98373.h:142
pub const PCM_TO_SPK_MONOMIX_CFG_SHIFT: u8 = 6; // max98373.h:143
pub const PCM_TO_SPK_CH0_SRC_MASK: u8 = 0xF; // max98373.h:144
pub const SPK_EN_MASK: u8 = 0x1; // max98373.h:162
pub const GLOBAL_EN_MASK: u8 = 0x1; // max98373.h:204
