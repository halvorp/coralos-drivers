// SPDX-License-Identifier: GPL-2.0-only
//! Block 27, the eDP VBT block: panel power sequencing (T1..T12), colour depth, fast-link parameters, the
//! vswing/pre-emphasis table select, PWM delays. `parse_edp()` — intel_bios.c:1416-1548;
//! `struct bdb_edp` — intel_vbt_defs.h:1090-1113.

use crate::bdb::{BlockId, Vbt};
use crate::error::VbtError;
use crate::panel::{panel_bits, PANEL_COUNT};

/// `EDP_18BPP 0` — intel_vbt_defs.h:1044.
pub const EDP_18BPP: u32 = 0;
/// `EDP_24BPP 1` — intel_vbt_defs.h:1045.
pub const EDP_24BPP: u32 = 1;
/// `EDP_30BPP 2` — intel_vbt_defs.h:1046.
pub const EDP_30BPP: u32 = 2;
/// `EDP_RATE_1_62 0` — intel_vbt_defs.h:1047.
pub const EDP_RATE_1_62: u8 = 0;
/// `EDP_RATE_2_7 1` — intel_vbt_defs.h:1048.
pub const EDP_RATE_2_7: u8 = 1;
/// `EDP_RATE_5_4 2` — intel_vbt_defs.h:1049.
pub const EDP_RATE_5_4: u8 = 2;
/// `EDP_LANE_1 0` — intel_vbt_defs.h:1050.
pub const EDP_LANE_1: u8 = 0;
/// `EDP_LANE_2 1` — intel_vbt_defs.h:1051.
pub const EDP_LANE_2: u8 = 1;
/// `EDP_LANE_4 3` — intel_vbt_defs.h:1052.
pub const EDP_LANE_4: u8 = 3;
/// `EDP_PREEMPHASIS_NONE 0` — intel_vbt_defs.h:1053.
pub const EDP_PREEMPHASIS_NONE: u8 = 0;
/// `EDP_PREEMPHASIS_3_5dB 1` — intel_vbt_defs.h:1054.
pub const EDP_PREEMPHASIS_3_5DB: u8 = 1;
/// `EDP_PREEMPHASIS_6dB 2` — intel_vbt_defs.h:1055.
pub const EDP_PREEMPHASIS_6DB: u8 = 2;
/// `EDP_PREEMPHASIS_9_5dB 3` — intel_vbt_defs.h:1056.
pub const EDP_PREEMPHASIS_9_5DB: u8 = 3;
/// `EDP_VSWING_0_4V 0` — intel_vbt_defs.h:1057.
pub const EDP_VSWING_0_4V: u8 = 0;
/// `EDP_VSWING_0_6V 1` — intel_vbt_defs.h:1058.
pub const EDP_VSWING_0_6V: u8 = 1;
/// `EDP_VSWING_0_8V 2` — intel_vbt_defs.h:1059.
pub const EDP_VSWING_0_8V: u8 = 2;
/// `EDP_VSWING_1_2V 3` — intel_vbt_defs.h:1060.
pub const EDP_VSWING_1_2V: u8 = 3;

// struct bdb_edp field offsets (packed), derived from intel_vbt_defs.h:1090-1113:
const OFF_POWER_SEQS: usize = 0; // edp_power_seq power_seqs[16], 10 bytes each (:1036-1042)
const OFF_COLOR_DEPTH: usize = 160; // u32
const OFF_FAST_LINK: usize = 164; // edp_fast_link_params[16], 2 bytes each
const OFF_SDRRS_MSA: usize = 196; // u32
const OFF_EDP_VSWING_PREEMPH: usize = 204; // u64 (173+)
const OFF_FAST_LINK_TRAINING_RATE: usize = 748; // u16[16] (224+)
const OFF_MAX_PORT_LINK_RATE: usize = 780; // u16[16] (244+)
const OFF_DSC_DISABLE: usize = 812; // u16 (251+)
/// Smallest eDP block that holds what every BDB version's `parse_edp()` reads unconditionally
/// (power_seqs, color_depth, fast_link_params, sdrrs_msa_timing_delay).
pub const EDP_MIN_SIZE: usize = OFF_SDRRS_MSA + 4;

/// `struct edp_power_seq` (intel_vbt_defs.h:1036) in 100 us units, as stored; `vbt_edp_to_pps_delays()`
/// (intel_bios.c:1406) maps t1_t3 -> power_up, t8 -> backlight_on, t9 -> backlight_off, t10 -> power_down,
/// t11_t12 -> power_cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerSeq {
    pub t1_t3: u16,
    pub t8: u16,
    pub t9: u16,
    pub t10: u16,
    pub t11_t12: u16,
}

/// Decoded eDP parameters for one panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EdpParams {
    pub power_seq: PowerSeq,
    /// 18/24/30; `None` for the reserved encoding 3 (Linux leaves bpp at 0).
    pub bpp: Option<u8>,
    /// fast-link port clock in kHz-of-link-rate units Linux uses (162000/270000/540000); `None` = unknown value.
    pub rate: Option<u32>,
    pub lanes: Option<u8>,
    /// `DP_TRAIN_PRE_EMPH_LEVEL_n` (n << 3) — include/drm/display/drm_dp.h:633-636; `None` = out of range (>3).
    pub preemphasis: Option<u8>,
    /// `DP_TRAIN_VOLTAGE_SWING_LEVEL_n` (n) — drm_dp.h:627-630; `None` = out of range (>3).
    pub vswing: Option<u8>,
    /// BDB 173+: low-power vswing table selected (`edp_vswing_preemph` nibble == 0); `None` before 173.
    pub low_vswing: Option<bool>,
    pub drrs_msa_timing_delay: u8,
    /// BDB 244+: `edp_max_port_link_rate * 20`.
    pub max_link_rate: Option<u32>,
    /// BDB 251+.
    pub dsc_disable: Option<bool>,
}

/// `parse_edp()` for `panel_type` (Ok(None) if the VBT has no eDP block).
pub fn parse_edp(vbt: &Vbt, panel_type: u8) -> Result<Option<EdpParams>, VbtError> {
    if usize::from(panel_type) >= PANEL_COUNT {
        return Err(VbtError::PanelTypeOutOfRange { panel_type });
    }
    let Some(b) = vbt.find_block(BlockId::Edp as u8)? else { return Ok(None) };
    b.require(EDP_MIN_SIZE)?;
    let p = usize::from(panel_type);
    let ps = OFF_POWER_SEQS + p * 10;
    let power_seq = PowerSeq {
        t1_t3: b.u16_at(ps),
        t8: b.u16_at(ps + 2),
        t9: b.u16_at(ps + 4),
        t10: b.u16_at(ps + 6),
        t11_t12: b.u16_at(ps + 8),
    };
    let bpp = match panel_bits(b.u32_at(OFF_COLOR_DEPTH), panel_type, 2) {
        EDP_18BPP => Some(18),
        EDP_24BPP => Some(24),
        EDP_30BPP => Some(30),
        _ => None,
    };
    // edp_fast_link_params: u8 {rate:4, lanes:4}, u8 {preemphasis:4, vswing:4}
    let fl0 = b.u8_at(OFF_FAST_LINK + p * 2);
    let fl1 = b.u8_at(OFF_FAST_LINK + p * 2 + 1);
    let (rate_f, lanes_f, pre_f, sw_f) = (fl0 & 0xf, fl0 >> 4, fl1 & 0xf, fl1 >> 4);
    let rate = if vbt.version >= 224 {
        Some(u32::from(b.u16_at(OFF_FAST_LINK_TRAINING_RATE + p * 2)) * 20)
    } else {
        match rate_f {
            EDP_RATE_1_62 => Some(162000),
            EDP_RATE_2_7 => Some(270000),
            EDP_RATE_5_4 => Some(540000),
            _ => None,
        }
    };
    let lanes = match lanes_f {
        EDP_LANE_1 => Some(1),
        EDP_LANE_2 => Some(2),
        EDP_LANE_4 => Some(4),
        _ => None,
    };
    let preemphasis = match pre_f {
        EDP_PREEMPHASIS_NONE..=EDP_PREEMPHASIS_9_5DB => Some(pre_f << 3),
        _ => None,
    };
    let vswing = match sw_f {
        EDP_VSWING_0_4V..=EDP_VSWING_1_2V => Some(sw_f),
        _ => None,
    };
    let low_vswing = if vbt.version >= 173 {
        Some(((b.u64_at(OFF_EDP_VSWING_PREEMPH) >> (p * 4)) & 0xf) == 0)
    } else {
        None
    };
    Ok(Some(EdpParams {
        power_seq,
        bpp,
        rate,
        lanes,
        preemphasis,
        vswing,
        low_vswing,
        drrs_msa_timing_delay: panel_bits(b.u32_at(OFF_SDRRS_MSA), panel_type, 2) as u8,
        max_link_rate: if vbt.version >= 244 { Some(u32::from(b.u16_at(OFF_MAX_PORT_LINK_RATE + p * 2)) * 20) } else { None },
        dsc_disable: if vbt.version >= 251 { Some((u32::from(b.u16_at(OFF_DSC_DISABLE)) >> panel_type) & 1 != 0) } else { None },
    }))
}
