// SPDX-License-Identifier: GPL-2.0-only
//! Block 43, LFP backlight control data. `parse_lfp_backlight()` — intel_bios.c:1014-1099;
//! `struct bdb_lfp_backlight` — intel_vbt_defs.h:1324-1335.

use crate::bdb::{BlockId, Vbt};
use crate::error::VbtError;
use crate::panel::PANEL_COUNT;

/// `BDB_BACKLIGHT_TYPE_NONE 0` — intel_vbt_defs.h:1300.
pub const BDB_BACKLIGHT_TYPE_NONE: u8 = 0;
/// `BDB_BACKLIGHT_TYPE_PWM 2` — intel_vbt_defs.h:1301.
pub const BDB_BACKLIGHT_TYPE_PWM: u8 = 2;
/// sizeof(struct lfp_backlight_data_entry): bits u8, pwm_freq_hz u16, min_brightness, i2c_address, i2c_command
/// — intel_vbt_defs.h:1303-1311 (6; `entry_size` must equal it, intel_bios.c:1027).
pub const LFP_BACKLIGHT_ENTRY_SIZE: u8 = 6;

// struct bdb_lfp_backlight offsets: entry_size @0, data[16] @1 (6 each), level[16] @97, backlight_control[16] @113,
// brightness_level[16] @129 (4 each), brightness_min_level[16] @193 (4 each), brightness_precision_bits[16] @257,
// hdr_dpcd_refresh_timeout u16[16] @273 — intel_vbt_defs.h:1324-1335.
const OFF_DATA: usize = 1;
const OFF_LEVEL: usize = 97;
const OFF_CONTROL: usize = 113;
const OFF_BRIGHTNESS_LEVEL: usize = 129;
const OFF_BRIGHTNESS_MIN_LEVEL: usize = 193;
const OFF_PRECISION_BITS: usize = 257;
const OFF_HDR_DPCD: usize = 273;
/// Smallest block `parse_lfp_backlight()` can read at any version (entry_size + data[16] + level[16]).
pub const BACKLIGHT_MIN_SIZE: usize = OFF_CONTROL;

/// Decoded backlight description for one panel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backlight {
    /// `entry->type == BDB_BACKLIGHT_TYPE_PWM`. When false, only `present` is meaningful.
    pub present: bool,
    pub entry_type: u8,
    /// BDB 191+: backlight_control.type (else INTEL_BACKLIGHT_DISPLAY_DDI, expressed as `None`).
    pub control_type: Option<u8>,
    pub controller: u8,
    pub pwm_freq_hz: u16,
    pub active_low_pwm: bool,
    pub min_brightness: u16,
    pub brightness_precision_bits: u8,
    /// Units of 100 ms (BDB 239+, rounded up), else Linux's 30 default.
    pub hdr_dpcd_refresh_timeout: u32,
}

/// `parse_lfp_backlight()`; `Ok(None)` if the block is absent.
pub fn parse_lfp_backlight(vbt: &Vbt, panel_type: u8) -> Result<Option<Backlight>, VbtError> {
    if usize::from(panel_type) >= PANEL_COUNT {
        return Err(VbtError::PanelTypeOutOfRange { panel_type });
    }
    let Some(b) = vbt.find_block(BlockId::LfpBacklight as u8)? else { return Ok(None) };
    b.require(BACKLIGHT_MIN_SIZE)?;
    if b.u8_at(0) != LFP_BACKLIGHT_ENTRY_SIZE {
        return Err(VbtError::UnsupportedBacklightEntrySize { size: b.u8_at(0) });
    }
    let p = usize::from(panel_type);
    let e = OFF_DATA + p * LFP_BACKLIGHT_ENTRY_SIZE as usize;
    let bits = b.u8_at(e); // type:2, active_low_pwm:1, i2c_pin:3, i2c_speed:2
    let entry_type = bits & 3;
    let mut out = Backlight {
        present: entry_type == BDB_BACKLIGHT_TYPE_PWM,
        entry_type,
        control_type: None,
        controller: 0,
        pwm_freq_hz: 0,
        active_low_pwm: false,
        min_brightness: 0,
        brightness_precision_bits: 0,
        hdr_dpcd_refresh_timeout: 30,
    };
    if !out.present {
        return Ok(Some(out)); // "PWM backlight not present in VBT" — intel_bios.c:1039
    }
    if vbt.version >= 191 {
        let m = b.u8_at(OFF_CONTROL + p); // type:4, controller:4
        out.control_type = Some(m & 0xf);
        out.controller = m >> 4;
    }
    out.pwm_freq_hz = b.u16_at(e + 1);
    out.active_low_pwm = bits & (1 << 2) != 0;
    if vbt.version >= 234 {
        let level = b.u16_at(OFF_BRIGHTNESS_LEVEL + p * 4);
        let mut min_level = b.u16_at(OFF_BRIGHTNESS_MIN_LEVEL + p * 4);
        let prec = b.u8_at(OFF_PRECISION_BITS + p);
        let scale = if vbt.version >= 236 { prec == 16 } else { level > 255 };
        if scale {
            min_level /= 255;
        }
        out.min_brightness = min_level;
        out.brightness_precision_bits = prec;
    } else {
        let _level = b.u8_at(OFF_LEVEL + p);
        out.min_brightness = u16::from(b.u8_at(e + 3));
    }
    if vbt.version >= 239 {
        out.hdr_dpcd_refresh_timeout = (u32::from(b.u16_at(OFF_HDR_DPCD + p * 2)) + 99) / 100;
    }
    Ok(Some(out))
}
