// SPDX-License-Identifier: GPL-2.0-only
//! Transcoder timing registers -> the mode they describe, and the DP M/N -> pixel clock math.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display.c (`intel_get_transcoder_timings`,
//! `intel_get_pipe_src_size`, `intel_get_m_n`, `intel_dotclock_calculate`, `intel_crtc_dotclock`),
//! drivers/gpu/drm/i915/display/intel_dp.c (`intel_dp_link_symbol_size`) and drivers/gpu/drm/drm_modes.c
//! (`drm_mode_vrefresh`). Copyright (c) Intel Corporation.

use crate::regs::*;

/// `REG_FIELD_GET(mask, v)` for a contiguous mask.
pub const fn field_get(mask: u32, v: u32) -> u32 {
    (v & mask) >> mask.trailing_zeros()
}

/// The five timing registers of a transcoder, as read (TRANS_HTOTAL, HBLANK, HSYNC, VTOTAL, VBLANK, VSYNC).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawTimings {
    pub htotal: u32,
    pub hblank: u32,
    pub hsync: u32,
    pub vtotal: u32,
    pub vblank: u32,
    pub vsync: u32,
}

/// `adjusted_mode->crtc_*`: every field is the register field PLUS ONE, as `intel_get_transcoder_timings` computes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModeTimings {
    pub hdisplay: u32,
    pub htotal: u32,
    pub hblank_start: u32,
    pub hblank_end: u32,
    pub hsync_start: u32,
    pub hsync_end: u32,
    pub vdisplay: u32,
    pub vtotal: u32,
    pub vblank_start: u32,
    pub vblank_end: u32,
    pub vsync_start: u32,
    pub vsync_end: u32,
    pub interlaced: bool,
}

/// `intel_get_transcoder_timings` (intel_display.c:2852-2893) for a non-DSI transcoder. `interlaced` is the
/// `TRANSCONF & TRANSCONF_INTERLACE_MASK_HSW` test of `intel_pipe_is_interlaced` (intel_display.c:2835-2850).
pub fn decode_timings(raw: &RawTimings, interlaced: bool) -> ModeTimings {
    let mut m = ModeTimings {
        hdisplay: field_get(HACTIVE_MASK, raw.htotal) + 1,  // intel_display.c:2861
        htotal: field_get(HTOTAL_MASK, raw.htotal) + 1,     // intel_display.c:2862
        hblank_start: field_get(HBLANK_START_MASK, raw.hblank) + 1, // intel_display.c:2867
        hblank_end: field_get(HBLANK_END_MASK, raw.hblank) + 1,     // intel_display.c:2868
        hsync_start: field_get(HSYNC_START_MASK, raw.hsync) + 1,    // intel_display.c:2872
        hsync_end: field_get(HSYNC_END_MASK, raw.hsync) + 1,        // intel_display.c:2873
        vdisplay: field_get(VACTIVE_MASK, raw.vtotal) + 1,          // intel_display.c:2876
        vtotal: field_get(VTOTAL_MASK, raw.vtotal) + 1,             // intel_display.c:2877
        vblank_start: field_get(VBLANK_START_MASK, raw.vblank) + 1, // intel_display.c:2883
        vblank_end: field_get(VBLANK_END_MASK, raw.vblank) + 1,     // intel_display.c:2884
        vsync_start: field_get(VSYNC_START_MASK, raw.vsync) + 1,    // intel_display.c:2887
        vsync_end: field_get(VSYNC_END_MASK, raw.vsync) + 1,        // intel_display.c:2888
        interlaced: false,
    };
    if interlaced {
        m.interlaced = true; // DRM_MODE_FLAG_INTERLACE, intel_display.c:2891
        m.vtotal += 1; // intel_display.c:2892
        m.vblank_end += 1; // intel_display.c:2893
    }
    m
}

/// `TRANSCONF & TRANSCONF_INTERLACE_MASK_HSW` (intel_display.c:2843-2846; display version >= 9 takes this branch).
pub const fn transconf_interlaced(transconf: u32) -> bool {
    transconf & TRANSCONF_INTERLACE_MASK_HSW != 0
}

/// PIPESRC -> (width, height): both fields plus one (`intel_get_pipe_src_size`, intel_display.c:2937-2951).
pub const fn decode_pipe_src(v: u32) -> (u32, u32) {
    (field_get(PIPESRC_WIDTH_MASK, v) + 1, field_get(PIPESRC_HEIGHT_MASK, v) + 1)
}

/// The DP data/link M/N of a transcoder (`struct intel_link_m_n`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkMn {
    pub data_m: u32,
    pub data_n: u32,
    pub link_m: u32,
    pub link_n: u32,
    /// Transfer-unit size, `TU_SIZE` field plus one (intel_display.c:3351).
    pub tu: u32,
}

/// `intel_get_m_n` (intel_display.c:3341-3352) from the four registers' raw words.
pub const fn decode_m_n(data_m: u32, data_n: u32, link_m: u32, link_n: u32) -> LinkMn {
    LinkMn {
        link_m: link_m & DATA_LINK_M_N_MASK, // intel_display.c:3347
        link_n: link_n & DATA_LINK_M_N_MASK, // intel_display.c:3348
        data_m: data_m & DATA_LINK_M_N_MASK, // intel_display.c:3349
        data_n: data_n & DATA_LINK_M_N_MASK, // intel_display.c:3350
        tu: field_get(TU_SIZE_MASK, data_m) + 1, // intel_display.c:3351
    }
}

/// `intel_dp_link_symbol_size` for the 8b/10b rates a Kaby Lake link can carry (intel_dp.c:155-158): 10 bits/symbol.
/// UHBR (128b/132b, 32) needs display version 14 and cannot occur here.
pub const LINK_SYMBOL_SIZE_8B10B: u64 = 10; // intel_dp.c:157

/// `intel_dotclock_calculate` (intel_display.c:4063-4084): kHz, rounded up, 0 when `link_n` is 0.
/// `port_clock_khz` is Linux's `port_clock` for the link, 540000 for HBR2 (`link_freq`, 10 kbit/s units): `link_freq * 10`
/// is divided by the 10-bit symbol size, so the two factors of ten cancel.
pub fn dotclock_from_m_n(port_clock_khz: u32, mn: &LinkMn) -> u32 {
    if mn.link_n == 0 {
        return 0; // intel_display.c:4079
    }
    let num = mn.link_m as u64 * port_clock_khz as u64 * 10; // mul_u32_u32(link_m, link_freq * 10), :4082
    let den = mn.link_n as u64 * LINK_SYMBOL_SIZE_8B10B; // link_n * intel_dp_link_symbol_size(link_freq), :4083
    num.div_ceil(den) as u32 // DIV_ROUND_UP_ULL
}

/// `drm_mode_vrefresh` (drm_modes.c:1287-1308): whole Hz, rounded to nearest. 0 if the totals are 0.
pub fn vrefresh_hz(clock_khz: u32, t: &ModeTimings) -> u32 {
    match vrefresh_terms(clock_khz, t) {
        Some((num, den)) => ((num * 1000 + den / 2) / den) as u32,
        None => 0,
    }
}

/// The same, in millihertz (59940 for 59.94 Hz) - the unit the log needs to tell 59.94 from 60.
pub fn vrefresh_millihz(clock_khz: u32, t: &ModeTimings) -> u32 {
    match vrefresh_terms(clock_khz, t) {
        Some((num, den)) => ((num * 1_000_000 + den / 2) / den) as u32,
        None => 0,
    }
}

/// `(num, den)` of `drm_mode_vrefresh` before its `* 1000`, or `None` for a mode with no totals.
fn vrefresh_terms(clock_khz: u32, t: &ModeTimings) -> Option<(u64, u64)> {
    if t.htotal == 0 || t.vtotal == 0 {
        return None; // drm_modes.c:1291
    }
    let num = clock_khz as u64 * if t.interlaced { 2 } else { 1 }; // drm_modes.c:1294-1295
    let den = t.htotal as u64 * t.vtotal as u64; // drm_modes.c:1304
    Some((num, den))
}
