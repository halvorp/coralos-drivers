// SPDX-License-Identifier: GPL-2.0-only
//! Hand-typed Linux literals for the AUX / DPCD / link-training constants. `tests/oracle.rs` checks every constant against the
//! header when the Linux tree is present; these pins hold the same values when it is not, and they are written from the DPCD
//! specification's own numbers (drm_dp.h), NOT through the constants under test (PORT_CONVENTIONS.md 3a.4 / 3a.6).
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::regs::*;

#[test]
fn dpcd_addresses() {
    assert_eq!(DP_DPCD_REV, 0x000);
    assert_eq!(DP_MAX_LINK_RATE, 0x001);
    assert_eq!(DP_MAX_LANE_COUNT, 0x002);
    assert_eq!(DP_MAX_DOWNSPREAD, 0x003);
    assert_eq!(DP_DOWNSTREAMPORT_PRESENT, 0x005);
    assert_eq!(DP_TRAINING_AUX_RD_INTERVAL, 0x00e);
    assert_eq!(DP_SUPPORTED_LINK_RATES, 0x010);
    assert_eq!(DP_RECEIVER_ALPM_CAP, 0x02e);
    assert_eq!(DP_LINK_BW_SET, 0x100);
    assert_eq!(DP_LANE_COUNT_SET, 0x101);
    assert_eq!(DP_TRAINING_PATTERN_SET, 0x102);
    assert_eq!(DP_TRAINING_LANE0_SET, 0x103);
    assert_eq!(DP_DOWNSPREAD_CTRL, 0x107);
    assert_eq!(DP_LINK_RATE_SET, 0x115);
    assert_eq!(DP_LANE0_1_STATUS, 0x202);
    assert_eq!(DP_LANE_ALIGN_STATUS_UPDATED, 0x204);
    assert_eq!(DP_ADJUST_REQUEST_LANE0_1, 0x206);
    assert_eq!(DP_SET_POWER, 0x600);
    assert_eq!(DP_EDP_DPCD_REV, 0x700);
    assert_eq!(DP_DP13_DPCD_REV, 0x2200);
    assert_eq!(DP_AUX_HDCP_AKSV, 0x68007);
    assert_eq!(DP_RECEIVER_CAP_SIZE, 15);
    assert_eq!(DP_LINK_STATUS_SIZE, 6);
    assert_eq!(DP_MAX_SUPPORTED_RATES, 8);
    assert_eq!(EDP_DISPLAY_CTL_CAP_SIZE, 5);
}

#[test]
fn dpcd_revisions_and_capability_bits() {
    assert_eq!([DP_DPCD_REV_11, DP_DPCD_REV_12, DP_DPCD_REV_13, DP_DPCD_REV_14], [0x11, 0x12, 0x13, 0x14]);
    assert_eq!(DP_EDP_14, 0x03);
    assert_eq!(DP_MAX_LANE_COUNT_MASK, 0x1f);
    assert_eq!(DP_TPS3_SUPPORTED, 0x40);
    assert_eq!(DP_ENHANCED_FRAME_CAP, 0x80);
    assert_eq!(DP_TPS4_SUPPORTED, 0x80);
    assert_eq!(DP_DWN_STRM_PORT_PRESENT, 0x01);
    assert_eq!(DP_TRAINING_AUX_RD_MASK, 0x7f);
    assert_eq!(DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT, 0x80);
    assert_eq!([DP_LINK_BW_1_62, DP_LINK_BW_2_7, DP_LINK_BW_5_4, DP_LINK_BW_8_1], [0x06, 0x0a, 0x14, 0x1e]);
    assert_eq!([DP_LINK_BW_10, DP_LINK_BW_13_5, DP_LINK_BW_20], [0x01, 0x04, 0x02]);
    assert_eq!(DP_LINK_RATE_TABLE, 0x00);
    assert_eq!(DP_LANE_COUNT_MASK, 0x0f);
    assert_eq!(DP_LANE_COUNT_ENHANCED_FRAME_EN, 0x80);
    assert_eq!(DP_MSA_TIMING_PAR_IGNORE_EN, 0x80);
    assert_eq!(DP_SET_ANSI_8B10B, 0x01);
    assert_eq!([DP_SET_POWER_D0, DP_SET_POWER_D3], [1, 2]);
}

#[test]
fn training_pattern_and_lane_settings() {
    assert_eq!([DP_TRAINING_PATTERN_DISABLE, DP_TRAINING_PATTERN_1, DP_TRAINING_PATTERN_2, DP_TRAINING_PATTERN_3, DP_TRAINING_PATTERN_4], [0, 1, 2, 3, 7]);
    assert_eq!(DP_LINK_SCRAMBLING_DISABLE, 0x20);
    assert_eq!(DP_TRAIN_VOLTAGE_SWING_MASK, 0x03);
    assert_eq!(DP_TRAIN_VOLTAGE_SWING_SHIFT, 0);
    assert_eq!(DP_TRAIN_MAX_SWING_REACHED, 0x04);
    assert_eq!([DP_TRAIN_VOLTAGE_SWING_LEVEL_0, DP_TRAIN_VOLTAGE_SWING_LEVEL_1, DP_TRAIN_VOLTAGE_SWING_LEVEL_2, DP_TRAIN_VOLTAGE_SWING_LEVEL_3], [0, 1, 2, 3]);
    assert_eq!(DP_TRAIN_PRE_EMPHASIS_MASK, 0x18);
    assert_eq!(DP_TRAIN_PRE_EMPHASIS_SHIFT, 3);
    assert_eq!([DP_TRAIN_PRE_EMPH_LEVEL_0, DP_TRAIN_PRE_EMPH_LEVEL_1, DP_TRAIN_PRE_EMPH_LEVEL_2, DP_TRAIN_PRE_EMPH_LEVEL_3], [0x00, 0x08, 0x10, 0x18]);
    assert_eq!(DP_TRAIN_MAX_PRE_EMPHASIS_REACHED, 0x20);
}

#[test]
fn link_status_bits_and_shifts() {
    assert_eq!((DP_LANE_CR_DONE, DP_LANE_CHANNEL_EQ_DONE, DP_LANE_SYMBOL_LOCKED), (0x01, 0x02, 0x04));
    assert_eq!(DP_CHANNEL_EQ_BITS, 0x07);
    assert_eq!(DP_INTERLANE_ALIGN_DONE, 0x01);
    assert_eq!((DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT, DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT), (0, 2));
    assert_eq!((DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT, DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT), (4, 6));
}

#[test]
fn aux_message_codes() {
    assert_eq!([DP_AUX_I2C_WRITE, DP_AUX_I2C_READ, DP_AUX_I2C_WRITE_STATUS_UPDATE, DP_AUX_I2C_MOT], [0, 1, 2, 4]);
    assert_eq!((DP_AUX_NATIVE_WRITE, DP_AUX_NATIVE_READ), (8, 9));
    assert_eq!([DP_AUX_NATIVE_REPLY_ACK, DP_AUX_NATIVE_REPLY_NACK, DP_AUX_NATIVE_REPLY_DEFER, DP_AUX_NATIVE_REPLY_MASK], [0, 1, 2, 3]);
}

#[test]
fn aux_channel_control_bits() {
    assert_eq!(DP_AUX_CH_CTL_SEND_BUSY, 0x8000_0000);
    assert_eq!(DP_AUX_CH_CTL_DONE, 0x4000_0000);
    assert_eq!(DP_AUX_CH_CTL_INTERRUPT, 0x2000_0000);
    assert_eq!(DP_AUX_CH_CTL_TIME_OUT_ERROR, 0x1000_0000);
    assert_eq!(DP_AUX_CH_CTL_TIME_OUT_MASK, 0x0c00_0000);
    assert_eq!(DP_AUX_CH_CTL_TIME_OUT_MAX, 0x0c00_0000);
    assert_eq!(DP_AUX_CH_CTL_TIME_OUT_400US, 0); // zero-valued: pinned by behaviour too
    assert_ne!(DP_AUX_CH_CTL_TIME_OUT_400US, DP_AUX_CH_CTL_TIME_OUT_MAX);
    assert_eq!(DP_AUX_CH_CTL_TIME_OUT_400US & DP_AUX_CH_CTL_TIME_OUT_MASK, 0);
    assert_eq!(DP_AUX_CH_CTL_RECEIVE_ERROR, 0x0200_0000);
    assert_eq!(DP_AUX_CH_CTL_MESSAGE_SIZE_MASK, 0x01f0_0000);
    assert_eq!(dp_aux_ch_ctl_message_size(20), 20 << 20);
    assert_eq!(dp_aux_ch_ctl_message_size(0x3f), 0x01f0_0000); // clipped to the 5-bit field
    assert_eq!(DP_AUX_CH_CTL_AUX_AKSV_SELECT, 0x8000);
    assert_eq!(DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK, 0x3e0);
    assert_eq!(DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK, 0x1f);
    assert_eq!(dp_aux_ch_ctl_fw_sync_pulse_skl(18), 17 << 5);
    assert_eq!(dp_aux_ch_ctl_sync_pulse_skl(32), 31);
}
