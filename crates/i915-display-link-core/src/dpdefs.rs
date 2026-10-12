// SPDX-License-Identifier: GPL-2.0-only
//! DisplayPort / DPCD constants, from include/drm/display/drm_dp.h and drm_dp_helper.h (v7.0-rc6).
//! `drm_dp.h` below means include/drm/display/drm_dp.h. Copyright (c) 2008 Keith Packard and the DRM authors.

// ---- AUX message request / reply (drm_dp.h:87-102) ----
pub const DP_AUX_I2C_WRITE: u8 = 0x0; // drm_dp.h:87
pub const DP_AUX_I2C_READ: u8 = 0x1; // drm_dp.h:88
pub const DP_AUX_I2C_WRITE_STATUS_UPDATE: u8 = 0x2; // drm_dp.h:89
pub const DP_AUX_I2C_MOT: u8 = 0x4; // drm_dp.h:90
pub const DP_AUX_NATIVE_WRITE: u8 = 0x8; // drm_dp.h:91
pub const DP_AUX_NATIVE_READ: u8 = 0x9; // drm_dp.h:92
pub const DP_AUX_NATIVE_REPLY_ACK: u8 = 0x0; // drm_dp.h:94
pub const DP_AUX_NATIVE_REPLY_NACK: u8 = 0x1; // drm_dp.h:95
pub const DP_AUX_NATIVE_REPLY_DEFER: u8 = 0x2; // drm_dp.h:96
pub const DP_AUX_NATIVE_REPLY_MASK: u8 = 0x3; // drm_dp.h:97
pub const DP_AUX_HDCP_AKSV: u32 = 0x68007; // drm_dp.h:1399

// ---- receiver capability field (drm_dp.h:107-181) ----
pub const DP_DPCD_REV: u32 = 0x000; // drm_dp.h:107
pub const DP_DPCD_REV_11: u8 = 0x11; // drm_dp.h:109
pub const DP_DPCD_REV_12: u8 = 0x12; // drm_dp.h:110
pub const DP_DPCD_REV_13: u8 = 0x13; // drm_dp.h:111
pub const DP_DPCD_REV_14: u8 = 0x14; // drm_dp.h:112
pub const DP_MAX_LINK_RATE: u32 = 0x001; // drm_dp.h:114
pub const DP_MAX_LANE_COUNT: u32 = 0x002; // drm_dp.h:116
pub const DP_MAX_LANE_COUNT_MASK: u8 = 0x1f; // drm_dp.h:117
pub const DP_TPS3_SUPPORTED: u8 = 1 << 6; // drm_dp.h:119
pub const DP_ENHANCED_FRAME_CAP: u8 = 1 << 7; // drm_dp.h:120
pub const DP_MAX_DOWNSPREAD: u32 = 0x003; // drm_dp.h:122
pub const DP_TPS4_SUPPORTED: u8 = 1 << 7; // drm_dp.h:126
pub const DP_DOWNSTREAMPORT_PRESENT: u32 = 0x005; // drm_dp.h:130
pub const DP_DWN_STRM_PORT_PRESENT: u8 = 1 << 0; // drm_dp.h:131
pub const DP_TRAINING_AUX_RD_INTERVAL: u32 = 0x00e; // drm_dp.h:172
pub const DP_TRAINING_AUX_RD_MASK: u8 = 0x7f; // drm_dp.h:173
pub const DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT: u8 = 1 << 7; // drm_dp.h:174
pub const DP_SUPPORTED_LINK_RATES: u32 = 0x010; // drm_dp.h:180
pub const DP_MAX_SUPPORTED_RATES: usize = 8; // drm_dp.h:181
/// `DP_RECEIVER_CAP_SIZE 0xf` — drm_dp.h:1691.
pub const DP_RECEIVER_CAP_SIZE: usize = 0xf;
/// `DP_LINK_STATUS_SIZE 6` — drm_dp.h:1688.
pub const DP_LINK_STATUS_SIZE: usize = 6;
pub const DP_DP13_DPCD_REV: u32 = 0x2200; // drm_dp.h:1184

// ---- link configuration (drm_dp.h:579-650) ----
pub const DP_LINK_BW_SET: u32 = 0x100; // drm_dp.h:579
pub const DP_LINK_RATE_TABLE: u8 = 0x00; // drm_dp.h:580
pub const DP_LINK_BW_1_62: u8 = 0x06; // drm_dp.h:581
pub const DP_LINK_BW_2_7: u8 = 0x0a; // drm_dp.h:582
pub const DP_LINK_BW_5_4: u8 = 0x14; // drm_dp.h:583
pub const DP_LINK_BW_8_1: u8 = 0x1e; // drm_dp.h:584
pub const DP_LANE_COUNT_SET: u32 = 0x101; // drm_dp.h:589
pub const DP_LANE_COUNT_MASK: u8 = 0x0f; // drm_dp.h:590
pub const DP_LANE_COUNT_ENHANCED_FRAME_EN: u8 = 1 << 7; // drm_dp.h:592
pub const DP_TRAINING_PATTERN_SET: u32 = 0x102; // drm_dp.h:594
pub const DP_TRAINING_PATTERN_DISABLE: u8 = 0; // drm_dp.h:595
pub const DP_TRAINING_PATTERN_1: u8 = 1; // drm_dp.h:596
pub const DP_TRAINING_PATTERN_2: u8 = 2; // drm_dp.h:597
pub const DP_TRAINING_PATTERN_3: u8 = 3; // drm_dp.h:599
pub const DP_TRAINING_PATTERN_4: u8 = 7; // drm_dp.h:600
pub const DP_LINK_SCRAMBLING_DISABLE: u8 = 1 << 5; // drm_dp.h:612
pub const DP_TRAINING_LANE0_SET: u32 = 0x103; // drm_dp.h:619
pub const DP_TRAIN_VOLTAGE_SWING_MASK: u8 = 0x3; // drm_dp.h:624
pub const DP_TRAIN_VOLTAGE_SWING_SHIFT: u8 = 0; // drm_dp.h:625
pub const DP_TRAIN_MAX_SWING_REACHED: u8 = 1 << 2; // drm_dp.h:626
pub const DP_TRAIN_VOLTAGE_SWING_LEVEL_0: u8 = 0 << 0; // drm_dp.h:627
pub const DP_TRAIN_VOLTAGE_SWING_LEVEL_1: u8 = 1 << 0; // drm_dp.h:628
pub const DP_TRAIN_VOLTAGE_SWING_LEVEL_2: u8 = 2 << 0; // drm_dp.h:629
pub const DP_TRAIN_VOLTAGE_SWING_LEVEL_3: u8 = 3 << 0; // drm_dp.h:630
pub const DP_TRAIN_PRE_EMPHASIS_MASK: u8 = 3 << 3; // drm_dp.h:632
pub const DP_TRAIN_PRE_EMPH_LEVEL_0: u8 = 0 << 3; // drm_dp.h:633
pub const DP_TRAIN_PRE_EMPH_LEVEL_1: u8 = 1 << 3; // drm_dp.h:634
pub const DP_TRAIN_PRE_EMPH_LEVEL_2: u8 = 2 << 3; // drm_dp.h:635
pub const DP_TRAIN_PRE_EMPH_LEVEL_3: u8 = 3 << 3; // drm_dp.h:636
pub const DP_TRAIN_PRE_EMPHASIS_SHIFT: u8 = 3; // drm_dp.h:638
pub const DP_TRAIN_MAX_PRE_EMPHASIS_REACHED: u8 = 1 << 5; // drm_dp.h:639
pub const DP_DOWNSPREAD_CTRL: u32 = 0x107; // drm_dp.h:643
pub const DP_MSA_TIMING_PAR_IGNORE_EN: u8 = 1 << 7; // drm_dp.h:646
pub const DP_SET_ANSI_8B10B: u8 = 1 << 0; // drm_dp.h:649
pub const DP_LINK_RATE_SET: u32 = 0x115; // drm_dp.h:702

// ---- link / sink status (drm_dp.h:797-831) ----
pub const DP_LANE0_1_STATUS: u32 = 0x202; // drm_dp.h:797
pub const DP_LANE_CR_DONE: u8 = 1 << 0; // drm_dp.h:799
pub const DP_LANE_CHANNEL_EQ_DONE: u8 = 1 << 1; // drm_dp.h:800
pub const DP_LANE_SYMBOL_LOCKED: u8 = 1 << 2; // drm_dp.h:801
/// `DP_CHANNEL_EQ_BITS` — drm_dp.h:803-805.
pub const DP_CHANNEL_EQ_BITS: u8 = DP_LANE_CR_DONE | DP_LANE_CHANNEL_EQ_DONE | DP_LANE_SYMBOL_LOCKED;
pub const DP_LANE_ALIGN_STATUS_UPDATED: u32 = 0x204; // drm_dp.h:807
pub const DP_INTERLANE_ALIGN_DONE: u8 = 1 << 0; // drm_dp.h:808
pub const DP_ADJUST_REQUEST_LANE0_1: u32 = 0x206; // drm_dp.h:822
pub const DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT: u8 = 0; // drm_dp.h:825
pub const DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT: u8 = 2; // drm_dp.h:827
pub const DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT: u8 = 4; // drm_dp.h:829
pub const DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT: u8 = 6; // drm_dp.h:831

// ---- power / eDP (drm_dp.h:1008-1019) ----
pub const DP_SET_POWER: u32 = 0x600; // drm_dp.h:1008
pub const DP_SET_POWER_D0: u8 = 0x1; // drm_dp.h:1009
pub const DP_SET_POWER_D3: u8 = 0x2; // drm_dp.h:1010
pub const DP_EDP_DPCD_REV: u32 = 0x700; // drm_dp.h:1015
pub const DP_EDP_14: u8 = 0x03; // drm_dp.h:1019

// ---- 2.0 UHBR link-bw codes and misc (drm_dp.h) ----
pub const DP_LINK_BW_10: u8 = 0x01; // drm_dp.h:585
pub const DP_LINK_BW_13_5: u8 = 0x04; // drm_dp.h:586
pub const DP_LINK_BW_20: u8 = 0x02; // drm_dp.h:587
pub const DP_RECEIVER_ALPM_CAP: u32 = 0x02e; // drm_dp.h:234
/// `EDP_DISPLAY_CTL_CAP_SIZE 5` — drm_dp.h:1695.
pub const EDP_DISPLAY_CTL_CAP_SIZE: usize = 5;
