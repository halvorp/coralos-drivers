// SPDX-License-Identifier: GPL-2.0-only
//! The pure link-training helpers of drm_dp_helper.c: lane status decode, clock-recovery / channel-EQ tests, the
//! sink's adjust requests, and the training-pattern read intervals.
//!
//! Ported from Linux drivers/gpu/drm/display/drm_dp_helper.c (v7.0-rc6): `dp_link_status` / `dp_get_lane_status`
//! (:77-90), `drm_dp_channel_eq_ok` (:92-110), `drm_dp_clock_recovery_ok` (:112-125), `drm_dp_get_adjust_request_voltage`
//! (:135-146), `drm_dp_get_adjust_request_pre_emphasis` (:148-160), `__8b10b_clock_recovery_delay_us` /
//! `__8b10b_channel_eq_delay_us` (:238-260), `__read_delay` (:296-349; DPRX, 8b/10b branch). Copyright (c) 2008-2009 Keith
//! Packard and the DRM authors.
use crate::dpdefs::*;

/// A 6-byte `DP_LANE0_1_STATUS..DP_ADJUST_REQUEST_LANE2_3` readout (`u8 link_status[DP_LINK_STATUS_SIZE]`).
pub type LinkStatus = [u8; DP_LINK_STATUS_SIZE];

/// `dp_link_status(link_status, r)` — drm_dp_helper.c:77-80: the register `r` out of the block that starts at 0x202.
const fn reg(ls: &LinkStatus, r: u32) -> u8 {
    ls[(r - DP_LANE0_1_STATUS) as usize]
}

/// `dp_get_lane_status()` — drm_dp_helper.c:82-90: two lanes per byte, low nibble first.
pub const fn lane_status(ls: &LinkStatus, lane: u32) -> u8 {
    let i = DP_LANE0_1_STATUS + (lane >> 1);
    let s = (lane & 1) * 4;
    (reg(ls, i) >> s) & 0xf
}

/// `drm_dp_channel_eq_ok()` — drm_dp_helper.c:92-110.
pub fn channel_eq_ok(ls: &LinkStatus, lane_count: u32) -> bool {
    if reg(ls, DP_LANE_ALIGN_STATUS_UPDATED) & DP_INTERLANE_ALIGN_DONE == 0 {
        return false;
    }
    for lane in 0..lane_count {
        if lane_status(ls, lane) & DP_CHANNEL_EQ_BITS != DP_CHANNEL_EQ_BITS {
            return false;
        }
    }
    true
}

/// `drm_dp_clock_recovery_ok()` — drm_dp_helper.c:112-125.
pub fn clock_recovery_ok(ls: &LinkStatus, lane_count: u32) -> bool {
    for lane in 0..lane_count {
        if lane_status(ls, lane) & DP_LANE_CR_DONE == 0 {
            return false;
        }
    }
    true
}

/// `drm_dp_get_adjust_request_voltage()` — drm_dp_helper.c:135-146: already shifted into `DP_TRAINING_LANEx_SET` position.
pub const fn adjust_request_voltage(ls: &LinkStatus, lane: u32) -> u8 {
    let i = DP_ADJUST_REQUEST_LANE0_1 + (lane >> 1);
    let s = if lane & 1 != 0 { DP_ADJUST_VOLTAGE_SWING_LANE1_SHIFT } else { DP_ADJUST_VOLTAGE_SWING_LANE0_SHIFT };
    ((reg(ls, i) >> s) & 0x3) << DP_TRAIN_VOLTAGE_SWING_SHIFT
}

/// `drm_dp_get_adjust_request_pre_emphasis()` — drm_dp_helper.c:148-160.
pub const fn adjust_request_pre_emphasis(ls: &LinkStatus, lane: u32) -> u8 {
    let i = DP_ADJUST_REQUEST_LANE0_1 + (lane >> 1);
    let s = if lane & 1 != 0 { DP_ADJUST_PRE_EMPHASIS_LANE1_SHIFT } else { DP_ADJUST_PRE_EMPHASIS_LANE0_SHIFT };
    ((reg(ls, i) >> s) & 0x3) << DP_TRAIN_PRE_EMPHASIS_SHIFT
}

/// `__8b10b_clock_recovery_delay_us()` — drm_dp_helper.c:238-248: 0 -> 100 us, else `rd_interval * 4 ms`.
pub const fn cr_delay_us_from_interval(rd_interval: u8) -> u32 {
    if rd_interval == 0 {
        100
    } else {
        rd_interval as u32 * 4 * 1000
    }
}

/// `__8b10b_channel_eq_delay_us()` — drm_dp_helper.c:250-260: 0 -> 400 us, else `rd_interval * 4 ms`.
pub const fn eq_delay_us_from_interval(rd_interval: u8) -> u32 {
    if rd_interval == 0 {
        400
    } else {
        rd_interval as u32 * 4 * 1000
    }
}

/// `drm_dp_read_clock_recovery_delay()` for the DPRX PHY, 8b/10b — drm_dp_helper.c:296-356: DPCD >= 1.4 is a fixed 100 us,
/// older reads `DP_TRAINING_AUX_RD_INTERVAL & DP_TRAINING_AUX_RD_MASK` (byte 0xe, inside the 15-byte cap block).
pub const fn clock_recovery_delay_us(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> u32 {
    if dpcd[DP_DPCD_REV as usize] >= DP_DPCD_REV_14 {
        100
    } else {
        cr_delay_us_from_interval(dpcd[DP_TRAINING_AUX_RD_INTERVAL as usize] & DP_TRAINING_AUX_RD_MASK)
    }
}

/// `drm_dp_read_channel_eq_delay()` for the DPRX PHY, 8b/10b — drm_dp_helper.c:296-363.
pub const fn channel_eq_delay_us(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> u32 {
    eq_delay_us_from_interval(dpcd[DP_TRAINING_AUX_RD_INTERVAL as usize] & DP_TRAINING_AUX_RD_MASK)
}
