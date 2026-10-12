// SPDX-License-Identifier: GPL-2.0-only
//! Every register / DPCD constant of this crate against the VALUE the pinned Linux header evaluates to (tests/common/chdr.rs),
//! plus a hand-typed literal pin for the families a header oracle cannot express. Both directions matter: the oracle catches a
//! mistyped constant, the literals catch an oracle that went missing (the oracle tests SKIP when the Linux tree is absent).
mod common;
use common::chdr::Header;
use i915_display_link_core::dpdefs as d;
use i915_display_link_core::regs as r;

fn check(rel: &str, rows: &[(&str, u64, &str)]) {
    let path = format!("{rel}");
    let Some(h) = Header::load(&path) else {
        eprintln!("SKIP: {path} not on this host (set I915_LINUX_REF)");
        return;
    };
    let mut bad = vec![];
    for (c_name, ours, rust_name) in rows {
        match h.eval(c_name) {
            Some(v) if v == *ours => {}
            Some(v) => bad.push(format!("{rust_name}: crate {ours:#x}, header {c_name} = {v:#x}")),
            None => bad.push(format!("{rust_name}: header macro {c_name} did not evaluate")),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

#[test]
fn constants_match_drm_dp_h() {
    check(
        "include/drm/display/drm_dp.h",
        &[
            ("DP_AUX_I2C_WRITE", d::DP_AUX_I2C_WRITE as u64, "DP_AUX_I2C_WRITE"),
            ("DP_AUX_I2C_READ", d::DP_AUX_I2C_READ as u64, "DP_AUX_I2C_READ"),
            ("DP_AUX_I2C_WRITE_STATUS_UPDATE", d::DP_AUX_I2C_WRITE_STATUS_UPDATE as u64, "DP_AUX_I2C_WRITE_STATUS_UPDATE"),
            ("DP_AUX_I2C_MOT", d::DP_AUX_I2C_MOT as u64, "DP_AUX_I2C_MOT"),
            ("DP_AUX_NATIVE_WRITE", d::DP_AUX_NATIVE_WRITE as u64, "DP_AUX_NATIVE_WRITE"),
            ("DP_AUX_NATIVE_READ", d::DP_AUX_NATIVE_READ as u64, "DP_AUX_NATIVE_READ"),
            ("DP_AUX_NATIVE_REPLY_ACK", d::DP_AUX_NATIVE_REPLY_ACK as u64, "DP_AUX_NATIVE_REPLY_ACK"),
            ("DP_AUX_NATIVE_REPLY_NACK", d::DP_AUX_NATIVE_REPLY_NACK as u64, "DP_AUX_NATIVE_REPLY_NACK"),
            ("DP_AUX_NATIVE_REPLY_DEFER", d::DP_AUX_NATIVE_REPLY_DEFER as u64, "DP_AUX_NATIVE_REPLY_DEFER"),
            ("DP_AUX_NATIVE_REPLY_MASK", d::DP_AUX_NATIVE_REPLY_MASK as u64, "DP_AUX_NATIVE_REPLY_MASK"),
            ("DP_AUX_HDCP_AKSV", d::DP_AUX_HDCP_AKSV as u64, "DP_AUX_HDCP_AKSV"),
            ("DP_DPCD_REV", d::DP_DPCD_REV as u64, "DP_DPCD_REV"),
            ("DP_DPCD_REV_11", d::DP_DPCD_REV_11 as u64, "DP_DPCD_REV_11"),
            ("DP_DPCD_REV_12", d::DP_DPCD_REV_12 as u64, "DP_DPCD_REV_12"),
            ("DP_DPCD_REV_13", d::DP_DPCD_REV_13 as u64, "DP_DPCD_REV_13"),
            ("DP_DPCD_REV_14", d::DP_DPCD_REV_14 as u64, "DP_DPCD_REV_14"),
            ("DP_MAX_LINK_RATE", d::DP_MAX_LINK_RATE as u64, "DP_MAX_LINK_RATE"),
            ("DP_MAX_LANE_COUNT", d::DP_MAX_LANE_COUNT as u64, "DP_MAX_LANE_COUNT"),
            ("DP_MAX_LANE_COUNT_MASK", d::DP_MAX_LANE_COUNT_MASK as u64, "DP_MAX_LANE_COUNT_MASK"),
            ("DP_TPS3_SUPPORTED", d::DP_TPS3_SUPPORTED as u64, "DP_TPS3_SUPPORTED"),
            ("DP_ENHANCED_FRAME_CAP", d::DP_ENHANCED_FRAME_CAP as u64, "DP_ENHANCED_FRAME_CAP"),
            ("DP_MAX_DOWNSPREAD", d::DP_MAX_DOWNSPREAD as u64, "DP_MAX_DOWNSPREAD"),
            ("DP_TPS4_SUPPORTED", d::DP_TPS4_SUPPORTED as u64, "DP_TPS4_SUPPORTED"),
            ("DP_DOWNSTREAMPORT_PRESENT", d::DP_DOWNSTREAMPORT_PRESENT as u64, "DP_DOWNSTREAMPORT_PRESENT"),
            ("DP_DWN_STRM_PORT_PRESENT", d::DP_DWN_STRM_PORT_PRESENT as u64, "DP_DWN_STRM_PORT_PRESENT"),
            ("DP_TRAINING_AUX_RD_INTERVAL", d::DP_TRAINING_AUX_RD_INTERVAL as u64, "DP_TRAINING_AUX_RD_INTERVAL"),
            ("DP_TRAINING_AUX_RD_MASK", d::DP_TRAINING_AUX_RD_MASK as u64, "DP_TRAINING_AUX_RD_MASK"),
            ("DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT", d::DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT as u64, "DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT"),
            ("DP_SUPPORTED_LINK_RATES", d::DP_SUPPORTED_LINK_RATES as u64, "DP_SUPPORTED_LINK_RATES"),
            ("DP_MAX_SUPPORTED_RATES", d::DP_MAX_SUPPORTED_RATES as u64, "DP_MAX_SUPPORTED_RATES"),
            ("DP_RECEIVER_CAP_SIZE", d::DP_RECEIVER_CAP_SIZE as u64, "DP_RECEIVER_CAP_SIZE"),
            ("DP_LINK_STATUS_SIZE", d::DP_LINK_STATUS_SIZE as u64, "DP_LINK_STATUS_SIZE"),
            ("DP_DP13_DPCD_REV", d::DP_DP13_DPCD_REV as u64, "DP_DP13_DPCD_REV"),
            ("DP_LINK_BW_SET", d::DP_LINK_BW_SET as u64, "DP_LINK_BW_SET"),
            ("DP_LINK_RATE_TABLE", d::DP_LINK_RATE_TABLE as u64, "DP_LINK_RATE_TABLE"),
            ("DP_LINK_BW_1_62", d::DP_LINK_BW_1_62 as u64, "DP_LINK_BW_1_62"),
            ("DP_LINK_BW_2_7", d::DP_LINK_BW_2_7 as u64, "DP_LINK_BW_2_7"),
            ("DP_LINK_BW_5_4", d::DP_LINK_BW_5_4 as u64, "DP_LINK_BW_5_4"),
            ("DP_LINK_BW_8_1", d::DP_LINK_BW_8_1 as u64, "DP_LINK_BW_8_1"),
            ("DP_LANE_COUNT_SET", d::DP_LANE_COUNT_SET as u64, "DP_LANE_COUNT_SET"),
            ("DP_LANE_COUNT_MASK", d::DP_LANE_COUNT_MASK as u64, "DP_LANE_COUNT_MASK"),
            ("DP_LANE_COUNT_ENHANCED_FRAME_EN", d::DP_LANE_COUNT_ENHANCED_FRAME_EN as u64, "DP_LANE_COUNT_ENHANCED_FRAME_EN"),
            ("DP_TRAINING_PATTERN_SET", d::DP_TRAINING_PATTERN_SET as u64, "DP_TRAINING_PATTERN_SET"),
            ("DP_TRAINING_PATTERN_DISABLE", d::DP_TRAINING_PATTERN_DISABLE as u64, "DP_TRAINING_PATTERN_DISABLE"),
            ("DP_TRAINING_PATTERN_1", d::DP_TRAINING_PATTERN_1 as u64, "DP_TRAINING_PATTERN_1"),
            ("DP_TRAINING_PATTERN_2", d::DP_TRAINING_PATTERN_2 as u64, "DP_TRAINING_PATTERN_2"),
            ("DP_TRAINING_PATTERN_3", d::DP_TRAINING_PATTERN_3 as u64, "DP_TRAINING_PATTERN_3"),
            ("DP_TRAINING_PATTERN_4", d::DP_TRAINING_PATTERN_4 as u64, "DP_TRAINING_PATTERN_4"),
            ("DP_LINK_SCRAMBLING_DISABLE", d::DP_LINK_SCRAMBLING_DISABLE as u64, "DP_LINK_SCRAMBLING_DISABLE"),
            ("DP_TRAINING_LANE0_SET", d::DP_TRAINING_LANE0_SET as u64, "DP_TRAINING_LANE0_SET"),
            ("DP_TRAIN_VOLTAGE_SWING_MASK", d::DP_TRAIN_VOLTAGE_SWING_MASK as u64, "DP_TRAIN_VOLTAGE_SWING_MASK"),
            ("DP_TRAIN_VOLTAGE_SWING_SHIFT", d::DP_TRAIN_VOLTAGE_SWING_SHIFT as u64, "DP_TRAIN_VOLTAGE_SWING_SHIFT"),
            ("DP_TRAIN_MAX_SWING_REACHED", d::DP_TRAIN_MAX_SWING_REACHED as u64, "DP_TRAIN_MAX_SWING_REACHED"),
            ("DP_TRAIN_VOLTAGE_SWING_LEVEL_0", d::DP_TRAIN_VOLTAGE_SWING_LEVEL_0 as u64, "DP_TRAIN_VOLTAGE_SWING_LEVEL_0"),
            ("DP_TRAIN_VOLTAGE_SWING_LEVEL_1", d::DP_TRAIN_VOLTAGE_SWING_LEVEL_1 as u64, "DP_TRAIN_VOLTAGE_SWING_LEVEL_1"),
            ("DP_TRAIN_VOLTAGE_SWING_LEVEL_2", d::DP_TRAIN_VOLTAGE_SWING_LEVEL_2 as u64, "DP_TRAIN_VOLTAGE_SWING_LEVEL_2"),
            ("DP_TRAIN_VOLTAGE_SWING_LEVEL_3", d::DP_TRAIN_VOLTAGE_SWING_LEVEL_3 as u64, "DP_TRAIN_VOLTAGE_SWING_LEVEL_3"),
            ("DP_TRAIN_PRE_EMPHASIS_MASK", d::DP_TRAIN_PRE_EMPHASIS_MASK as u64, "DP_TRAIN_PRE_EMPHASIS_MASK"),
            ("DP_TRAIN_PRE_EMPH_LEVEL_0", d::DP_TRAIN_PRE_EMPH_LEVEL_0 as u64, "DP_TRAIN_PRE_EMPH_LEVEL_0"),
            ("DP_TRAIN_PRE_EMPH_LEVEL_1", d::DP_TRAIN_PRE_EMPH_LEVEL_1 as u64, "DP_TRAIN_PRE_EMPH_LEVEL_1"),
            ("DP_TRAIN_PRE_EMPH_LEVEL_2", d::DP_TRAIN_PRE_EMPH_LEVEL_2 as u64, "DP_TRAIN_PRE_EMPH_LEVEL_2"),
            ("DP_TRAIN_PRE_EMPH_LEVEL_3", d::DP_TRAIN_PRE_EMPH_LEVEL_3 as u64, "DP_TRAIN_PRE_EMPH_LEVEL_3"),
            ("DP_TRAIN_PRE_EMPHASIS_SHIFT", d::DP_TRAIN_PRE_EMPHASIS_SHIFT as u64, "DP_TRAIN_PRE_EMPHASIS_SHIFT"),
            ("DP_TRAIN_MAX_PRE_EMPHASIS_REACHED", d::DP_TRAIN_MAX_PRE_EMPHASIS_REACHED as u64, "DP_TRAIN_MAX_PRE_EMPHASIS_REACHED"),
            ("DP_DOWNSPREAD_CTRL", d::DP_DOWNSPREAD_CTRL as u64, "DP_DOWNSPREAD_CTRL"),
            ("DP_MSA_TIMING_PAR_IGNORE_EN", d::DP_MSA_TIMING_PAR_IGNORE_EN as u64, "DP_MSA_TIMING_PAR_IGNORE_EN"),
            ("DP_SET_ANSI_8B10B", d::DP_SET_ANSI_8B10B as u64, "DP_SET_ANSI_8B10B"),
            ("DP_LINK_RATE_SET", d::DP_LINK_RATE_SET as u64, "DP_LINK_RATE_SET"),
            ("DP_LANE0_1_STATUS", d::DP_LANE0_1_STATUS as u64, "DP_LANE0_1_STATUS"),
            ("DP_LANE_CR_DONE", d::DP_LANE_CR_DONE as u64, "DP_LANE_CR_DONE"),
            ("DP_LANE_CHANNEL_EQ_DONE", d::DP_LANE_CHANNEL_EQ_DONE as u64, "DP_LANE_CHANNEL_EQ_DONE"),
            ("DP_LANE_SYMBOL_LOCKED", d::DP_LANE_SYMBOL_LOCKED as u64, "DP_LANE_SYMBOL_LOCKED"),
            ("DP_CHANNEL_EQ_BITS", d::DP_CHANNEL_EQ_BITS as u64, "DP_CHANNEL_EQ_BITS"),
            ("DP_LANE_ALIGN_STATUS_UPDATED", d::DP_LANE_ALIGN_STATUS_UPDATED as u64, "DP_LANE_ALIGN_STATUS_UPDATED"),
            ("DP_INTERLANE_ALIGN_DONE", d::DP_INTERLANE_ALIGN_DONE as u64, "DP_INTERLANE_ALIGN_DONE"),
            ("DP_ADJUST_REQUEST_LANE0_1", d::DP_ADJUST_REQUEST_LANE0_1 as u64, "DP_ADJUST_REQUEST_LANE0_1"),
            ("DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT", d::DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT as u64, "DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT"),
            ("DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT", d::DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT as u64, "DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT"),
            ("DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT", d::DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT as u64, "DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT"),
            ("DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT", d::DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT as u64, "DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT"),
            ("DP_SET_POWER", d::DP_SET_POWER as u64, "DP_SET_POWER"),
            ("DP_SET_POWER_D0", d::DP_SET_POWER_D0 as u64, "DP_SET_POWER_D0"),
            ("DP_SET_POWER_D3", d::DP_SET_POWER_D3 as u64, "DP_SET_POWER_D3"),
            ("DP_EDP_DPCD_REV", d::DP_EDP_DPCD_REV as u64, "DP_EDP_DPCD_REV"),
            ("DP_EDP_14", d::DP_EDP_14 as u64, "DP_EDP_14"),
            ("DP_LINK_BW_10", d::DP_LINK_BW_10 as u64, "DP_LINK_BW_10"),
            ("DP_LINK_BW_13_5", d::DP_LINK_BW_13_5 as u64, "DP_LINK_BW_13_5"),
            ("DP_LINK_BW_20", d::DP_LINK_BW_20 as u64, "DP_LINK_BW_20"),
            ("DP_RECEIVER_ALPM_CAP", d::DP_RECEIVER_ALPM_CAP as u64, "DP_RECEIVER_ALPM_CAP"),
            ("EDP_DISPLAY_CTL_CAP_SIZE", d::EDP_DISPLAY_CTL_CAP_SIZE as u64, "EDP_DISPLAY_CTL_CAP_SIZE"),
        ],
    );
}

#[test]
fn constants_match_intel_dp_aux_regs_h() {
    check(
        "drivers/gpu/drm/i915/display/intel_dp_aux_regs.h",
        &[
            ("_DPA_AUX_CH_CTL", r::DPA_AUX_CH_CTL as u64, "DPA_AUX_CH_CTL"),
            ("_DPB_AUX_CH_CTL", r::DPB_AUX_CH_CTL as u64, "DPB_AUX_CH_CTL"),
            ("_DPA_AUX_CH_DATA1", r::DPA_AUX_CH_DATA1 as u64, "DPA_AUX_CH_DATA1"),
            ("_DPB_AUX_CH_DATA1", r::DPB_AUX_CH_DATA1 as u64, "DPB_AUX_CH_DATA1"),
            ("DP_AUX_CH_CTL_SEND_BUSY", r::DP_AUX_CH_CTL_SEND_BUSY as u64, "DP_AUX_CH_CTL_SEND_BUSY"),
            ("DP_AUX_CH_CTL_DONE", r::DP_AUX_CH_CTL_DONE as u64, "DP_AUX_CH_CTL_DONE"),
            ("DP_AUX_CH_CTL_INTERRUPT", r::DP_AUX_CH_CTL_INTERRUPT as u64, "DP_AUX_CH_CTL_INTERRUPT"),
            ("DP_AUX_CH_CTL_TIME_OUT_ERROR", r::DP_AUX_CH_CTL_TIME_OUT_ERROR as u64, "DP_AUX_CH_CTL_TIME_OUT_ERROR"),
            ("DP_AUX_CH_CTL_TIME_OUT_MASK", r::DP_AUX_CH_CTL_TIME_OUT_MASK as u64, "DP_AUX_CH_CTL_TIME_OUT_MASK"),
            ("DP_AUX_CH_CTL_TIME_OUT_400us", r::DP_AUX_CH_CTL_TIME_OUT_400US as u64, "DP_AUX_CH_CTL_TIME_OUT_400US"),
            ("DP_AUX_CH_CTL_TIME_OUT_MAX", r::DP_AUX_CH_CTL_TIME_OUT_MAX as u64, "DP_AUX_CH_CTL_TIME_OUT_MAX"),
            ("DP_AUX_CH_CTL_RECEIVE_ERROR", r::DP_AUX_CH_CTL_RECEIVE_ERROR as u64, "DP_AUX_CH_CTL_RECEIVE_ERROR"),
            ("DP_AUX_CH_CTL_MESSAGE_SIZE_MASK", r::DP_AUX_CH_CTL_MESSAGE_SIZE_MASK as u64, "DP_AUX_CH_CTL_MESSAGE_SIZE_MASK"),
            ("DP_AUX_CH_CTL_AUX_AKSV_SELECT", r::DP_AUX_CH_CTL_AUX_AKSV_SELECT as u64, "DP_AUX_CH_CTL_AUX_AKSV_SELECT"),
            ("DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK", r::DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK as u64, "DP_AUX_CH_CTL_FW_SYNC_PULSE_SKL_MASK"),
            ("DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK", r::DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK as u64, "DP_AUX_CH_CTL_SYNC_PULSE_SKL_MASK"),
        ],
    );
}

#[test]
fn constants_match_intel_display_regs_h() {
    check(
        "drivers/gpu/drm/i915/display/intel_display_regs.h",
        &[
            ("_DP_TP_CTL_A", r::DP_TP_CTL_A as u64, "DP_TP_CTL_A"),
            ("_DP_TP_CTL_B", r::DP_TP_CTL_B as u64, "DP_TP_CTL_B"),
            ("_DP_TP_STATUS_A", r::DP_TP_STATUS_A as u64, "DP_TP_STATUS_A"),
            ("_DP_TP_STATUS_B", r::DP_TP_STATUS_B as u64, "DP_TP_STATUS_B"),
            ("DP_TP_CTL_ENABLE", r::DP_TP_CTL_ENABLE as u64, "DP_TP_CTL_ENABLE"),
            ("DP_TP_CTL_MODE_MASK", r::DP_TP_CTL_MODE_MASK as u64, "DP_TP_CTL_MODE_MASK"),
            ("DP_TP_CTL_MODE_SST", r::DP_TP_CTL_MODE_SST as u64, "DP_TP_CTL_MODE_SST"),
            ("DP_TP_CTL_MODE_MST", r::DP_TP_CTL_MODE_MST as u64, "DP_TP_CTL_MODE_MST"),
            ("DP_TP_CTL_ENHANCED_FRAME_ENABLE", r::DP_TP_CTL_ENHANCED_FRAME_ENABLE as u64, "DP_TP_CTL_ENHANCED_FRAME_ENABLE"),
            ("DP_TP_CTL_LINK_TRAIN_MASK", r::DP_TP_CTL_LINK_TRAIN_MASK as u64, "DP_TP_CTL_LINK_TRAIN_MASK"),
            ("DP_TP_CTL_LINK_TRAIN_PAT1", r::DP_TP_CTL_LINK_TRAIN_PAT1 as u64, "DP_TP_CTL_LINK_TRAIN_PAT1"),
            ("DP_TP_CTL_LINK_TRAIN_PAT2", r::DP_TP_CTL_LINK_TRAIN_PAT2 as u64, "DP_TP_CTL_LINK_TRAIN_PAT2"),
            ("DP_TP_CTL_LINK_TRAIN_PAT3", r::DP_TP_CTL_LINK_TRAIN_PAT3 as u64, "DP_TP_CTL_LINK_TRAIN_PAT3"),
            ("DP_TP_CTL_LINK_TRAIN_PAT4", r::DP_TP_CTL_LINK_TRAIN_PAT4 as u64, "DP_TP_CTL_LINK_TRAIN_PAT4"),
            ("DP_TP_CTL_LINK_TRAIN_IDLE", r::DP_TP_CTL_LINK_TRAIN_IDLE as u64, "DP_TP_CTL_LINK_TRAIN_IDLE"),
            ("DP_TP_CTL_LINK_TRAIN_NORMAL", r::DP_TP_CTL_LINK_TRAIN_NORMAL as u64, "DP_TP_CTL_LINK_TRAIN_NORMAL"),
            ("DP_TP_STATUS_IDLE_DONE", r::DP_TP_STATUS_IDLE_DONE as u64, "DP_TP_STATUS_IDLE_DONE"),
            ("_DDI_BUF_TRANS_A", r::DDI_BUF_TRANS_A as u64, "DDI_BUF_TRANS_A"),
            ("_DDI_BUF_TRANS_B", r::DDI_BUF_TRANS_B as u64, "DDI_BUF_TRANS_B"),
            ("DDI_BUF_BALANCE_LEG_ENABLE", r::DDI_BUF_BALANCE_LEG_ENABLE as u64, "DDI_BUF_BALANCE_LEG_ENABLE"),
            ("DISPIO_CR_TX_BMU_CR0", r::DISPIO_CR_TX_BMU_CR0.offset() as u64, "DISPIO_CR_TX_BMU_CR0"),
            ("PCH_RAWCLK_FREQ", r::PCH_RAWCLK_FREQ.offset() as u64, "PCH_RAWCLK_FREQ"),
            ("RAWCLK_FREQ_MASK", r::RAWCLK_FREQ_MASK as u64, "RAWCLK_FREQ_MASK"),
        ],
    );
}

#[test]
fn constants_match_intel_pps_regs_h() {
    check(
        "drivers/gpu/drm/i915/display/intel_pps_regs.h",
        &[
            ("PPS_BASE", r::PPS_BASE as u64, "PPS_BASE"),
            ("PCH_PPS_BASE", r::PCH_PPS_BASE as u64, "PCH_PPS_BASE"),
            ("_PP_STATUS", r::PP_STATUS_OFF as u64, "PP_STATUS_OFF"),
            ("_PP_OFF_DELAYS", r::PP_OFF_DELAYS_OFF as u64, "PP_OFF_DELAYS_OFF"),
            ("_PP_CONTROL", r::PP_CONTROL_OFF as u64, "PP_CONTROL_OFF"),
            ("_PP_ON_DELAYS", r::PP_ON_DELAYS_OFF as u64, "PP_ON_DELAYS_OFF"),
            ("_PP_DIVISOR", r::PP_DIVISOR_OFF as u64, "PP_DIVISOR_OFF"),
            ("PP_ON", r::PP_ON as u64, "PP_ON"),
            ("PP_READY", r::PP_READY as u64, "PP_READY"),
            ("PP_SEQUENCE_MASK", r::PP_SEQUENCE_MASK as u64, "PP_SEQUENCE_MASK"),
            ("PP_SEQUENCE_NONE", r::PP_SEQUENCE_NONE as u64, "PP_SEQUENCE_NONE"),
            ("PP_SEQUENCE_POWER_UP", r::PP_SEQUENCE_POWER_UP as u64, "PP_SEQUENCE_POWER_UP"),
            ("PP_SEQUENCE_POWER_DOWN", r::PP_SEQUENCE_POWER_DOWN as u64, "PP_SEQUENCE_POWER_DOWN"),
            ("PP_CYCLE_DELAY_ACTIVE", r::PP_CYCLE_DELAY_ACTIVE as u64, "PP_CYCLE_DELAY_ACTIVE"),
            ("PP_SEQUENCE_STATE_MASK", r::PP_SEQUENCE_STATE_MASK as u64, "PP_SEQUENCE_STATE_MASK"),
            ("PP_SEQUENCE_STATE_OFF_IDLE", r::PP_SEQUENCE_STATE_OFF_IDLE as u64, "PP_SEQUENCE_STATE_OFF_IDLE"),
            ("PP_SEQUENCE_STATE_ON_IDLE", r::PP_SEQUENCE_STATE_ON_IDLE as u64, "PP_SEQUENCE_STATE_ON_IDLE"),
            ("PANEL_UNLOCK_MASK", r::PANEL_UNLOCK_MASK as u64, "PANEL_UNLOCK_MASK"),
            ("PANEL_UNLOCK_REGS", r::PANEL_UNLOCK_REGS as u64, "PANEL_UNLOCK_REGS"),
            ("BXT_POWER_CYCLE_DELAY_MASK", r::BXT_POWER_CYCLE_DELAY_MASK as u64, "BXT_POWER_CYCLE_DELAY_MASK"),
            ("EDP_FORCE_VDD", r::EDP_FORCE_VDD as u64, "EDP_FORCE_VDD"),
            ("EDP_BLC_ENABLE", r::EDP_BLC_ENABLE as u64, "EDP_BLC_ENABLE"),
            ("PANEL_POWER_RESET", r::PANEL_POWER_RESET as u64, "PANEL_POWER_RESET"),
            ("PANEL_POWER_ON", r::PANEL_POWER_ON as u64, "PANEL_POWER_ON"),
            ("PANEL_PORT_SELECT_MASK", r::PANEL_PORT_SELECT_MASK as u64, "PANEL_PORT_SELECT_MASK"),
            ("PANEL_PORT_SELECT_DPA", r::PANEL_PORT_SELECT_DPA as u64, "PANEL_PORT_SELECT_DPA"),
            ("PANEL_POWER_UP_DELAY_MASK", r::PANEL_POWER_UP_DELAY_MASK as u64, "PANEL_POWER_UP_DELAY_MASK"),
            ("PANEL_LIGHT_ON_DELAY_MASK", r::PANEL_LIGHT_ON_DELAY_MASK as u64, "PANEL_LIGHT_ON_DELAY_MASK"),
            ("PANEL_POWER_DOWN_DELAY_MASK", r::PANEL_POWER_DOWN_DELAY_MASK as u64, "PANEL_POWER_DOWN_DELAY_MASK"),
            ("PANEL_LIGHT_OFF_DELAY_MASK", r::PANEL_LIGHT_OFF_DELAY_MASK as u64, "PANEL_LIGHT_OFF_DELAY_MASK"),
            ("PP_REFERENCE_DIVIDER_MASK", r::PP_REFERENCE_DIVIDER_MASK as u64, "PP_REFERENCE_DIVIDER_MASK"),
            ("PANEL_POWER_CYCLE_DELAY_MASK", r::PANEL_POWER_CYCLE_DELAY_MASK as u64, "PANEL_POWER_CYCLE_DELAY_MASK"),
        ],
    );
}

#[test]
fn constants_match_intel_backlight_regs_h() {
    check(
        "drivers/gpu/drm/i915/display/intel_backlight_regs.h",
        &[
            ("BLC_PWM_CPU_CTL2", r::BLC_PWM_CPU_CTL2.offset() as u64, "BLC_PWM_CPU_CTL2"),
            ("BLM_PWM_ENABLE", r::BLM_PWM_ENABLE as u64, "BLM_PWM_ENABLE"),
            ("BLC_PWM_PCH_CTL1", r::BLC_PWM_PCH_CTL1.offset() as u64, "BLC_PWM_PCH_CTL1"),
            ("BLM_PCH_PWM_ENABLE", r::BLM_PCH_PWM_ENABLE as u64, "BLM_PCH_PWM_ENABLE"),
            ("BLM_PCH_OVERRIDE_ENABLE", r::BLM_PCH_OVERRIDE_ENABLE as u64, "BLM_PCH_OVERRIDE_ENABLE"),
            ("BLM_PCH_POLARITY", r::BLM_PCH_POLARITY as u64, "BLM_PCH_POLARITY"),
            ("BLC_PWM_PCH_CTL2", r::BLC_PWM_PCH_CTL2.offset() as u64, "BLC_PWM_PCH_CTL2"),
            ("BACKLIGHT_DUTY_CYCLE_MASK", r::BACKLIGHT_DUTY_CYCLE_MASK as u64, "BACKLIGHT_DUTY_CYCLE_MASK"),
        ],
    );
}

#[test]
fn constants_match_i915_reg_h() {
    check(
        "drivers/gpu/drm/i915/i915_reg.h",
        &[
            ("SOUTH_CHICKEN1", r::SOUTH_CHICKEN1.offset() as u64, "SOUTH_CHICKEN1"),
            ("SPT_PWM_GRANULARITY", r::SPT_PWM_GRANULARITY as u64, "SPT_PWM_GRANULARITY"),
        ],
    );
}
