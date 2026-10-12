// SPDX-License-Identifier: GPL-2.0-only
//! Pipe / transcoder WRITE encoding for an eDP output on DDI A (display version 9): the register words `hsw_crtc_enable()` and the
//! DDI encoder hooks program from a chosen mode - timings, pipe source size, pipe misc, TRANSCONF, frame-start delay, the DP M/N
//! block, `TRANS_DDI_FUNC_CTL`, and the line-time watermark. Pure values; the sequencing is `i915-display-modeset-core`.
//!
//! Ported from Linux drivers/gpu/drm/i915/display at v7.0-rc6: intel_display.c `intel_set_transcoder_timings`,
//! `intel_set_pipe_src_size`, `bdw_set_pipe_misc`, `hsw_set_transconf`, `hsw_set_frame_start_delay`, `intel_set_m_n`,
//! `intel_cpu_transcoder_set_m1_n1`, `intel_link_compute_m_n`, `compute_m_n`, `skl_linetime_wm`, `hsw_set_linetime_wm`;
//! intel_ddi.c `intel_ddi_transcoder_func_reg_val_get`, `intel_ddi_enable_transcoder_func`, `intel_ddi_enable_transcoder_clock`;
//! intel_dp.c `intel_dp_effective_data_rate`, `intel_dp_link_symbol_clock`, `intel_dp_bw_fec_overhead`;
//! drm_dp_helper.c `drm_dp_max_dprx_data_rate`. Copyright (c) Intel Corporation.
//!
//! NOT PORTED (named refusals): interlaced modes, panel replay / VRR (`intel_vrr_*`), FEC/DSC, HDMI/analog/MST outputs of
//! `intel_ddi_transcoder_func_reg_val_get`, deep colour beyond 10 bpc, the Haswell eDP VTOTAL_B/C workaround, `TRANS_MULT` (not
//! written for the eDP transcoder).
use crate::regs::*;
use crate::regs_write::*;
use crate::skl_plane::RegWrites;
use crate::timing::ModeTimings;
use crate::types::{Ddi, Pipe, Transcoder};

/// `TRANS_VSYNCSHIFT_A` — intel_display_regs.h:406.
pub const TRANS_VSYNCSHIFT_A: u32 = 0x60028; // intel_display_regs.h:406

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModeError {
    Interlaced,
    /// A register field would overflow 16 bits ("timing register field").
    FieldOverflow { what: &'static str },
    /// `pipe_bpp` is not 18, 24 or 30.
    UnsupportedBpp { bpp: u32 },
    /// Only DDI A / transcoder EDP / pipe A is ported for the output register (`TRANS_DDI_EDP_INPUT_*`).
    NotEdpOnDdiA,
    /// Lane count not 1, 2 or 4.
    BadLaneCount { lanes: u32 },
    ZeroDivisor { what: &'static str },
}

impl ModeError {
    pub const fn name(&self) -> &'static str {
        match self {
            ModeError::Interlaced => "MODE-INTERLACED-NOT-PORTED",
            ModeError::FieldOverflow { .. } => "MODE-FIELD-OVERFLOW",
            ModeError::UnsupportedBpp { .. } => "MODE-UNSUPPORTED-BPP",
            ModeError::NotEdpOnDdiA => "MODE-NOT-EDP-ON-DDI-A",
            ModeError::BadLaneCount { .. } => "MODE-BAD-LANE-COUNT",
            ModeError::ZeroDivisor { .. } => "MODE-ZERO-DIVISOR",
        }
    }
}

fn pair(hi: u32, lo: u32, what: &'static str) -> Result<u32, ModeError> {
    // Each register field holds `value - 1`; the field is 16 bits wide.
    let (hi, lo) = (hi.checked_sub(1), lo.checked_sub(1));
    match (hi, lo) {
        (Some(h), Some(l)) if h <= 0xffff && l <= 0xffff => Ok((h << 16) | l),
        _ => Err(ModeError::FieldOverflow { what }),
    }
}

/// `intel_set_transcoder_timings()` for a progressive mode on display ver 9 — the seven writes in Linux's order: VSYNCSHIFT (0:
/// `DISPLAY_VER >= 4 && < 35`), HTOTAL, HBLANK, HSYNC, VTOTAL, VBLANK, VSYNC.
pub fn timing_writes(t: Transcoder, m: &ModeTimings) -> Result<RegWrites<7>, ModeError> {
    if m.interlaced {
        return Err(ModeError::Interlaced);
    }
    let mut w = RegWrites::new();
    w.push(trans_reg(t, TRANS_VSYNCSHIFT_A), 0);
    w.push(trans_reg(t, TRANS_HTOTAL_A), pair(m.htotal, m.hdisplay, "htotal")?);
    w.push(trans_reg(t, TRANS_HBLANK_A), pair(m.hblank_end, m.hblank_start, "hblank")?);
    w.push(trans_reg(t, TRANS_HSYNC_A), pair(m.hsync_end, m.hsync_start, "hsync")?);
    w.push(trans_reg(t, TRANS_VTOTAL_A), pair(m.vtotal, m.vdisplay, "vtotal")?);
    w.push(trans_reg(t, TRANS_VBLANK_A), pair(m.vblank_end, m.vblank_start, "vblank")?);
    w.push(trans_reg(t, TRANS_VSYNC_A), pair(m.vsync_end, m.vsync_start, "vsync")?);
    Ok(w)
}

/// `PIPESRC_WIDTH(w - 1) | PIPESRC_HEIGHT(h - 1)` — `intel_set_pipe_src_size`.
pub fn pipesrc_value(w: u32, h: u32) -> Result<u32, ModeError> {
    pair(w, h, "pipesrc")
}

/// `bdw_set_pipe_misc()` for RGB, no dither, display ver 9: only the BPC field.
pub fn pipe_misc_value(pipe_bpp: u32) -> Result<u32, ModeError> {
    match pipe_bpp {
        18 => Ok(PIPE_MISC_BPC_6),
        24 => Ok(PIPE_MISC_BPC_8),
        30 => Ok(PIPE_MISC_BPC_10),
        bpp => Err(ModeError::UnsupportedBpp { bpp }),
    }
}

/// `hsw_set_transconf()` for a MODEST modeset: "During modeset the pipe is still disabled and must remain so" — no ENABLE, no
/// dither (that is Haswell only), progressive (`TRANSCONF_INTERLACE_PF_PD_ILK`), RGB output.
pub const fn transconf_modeset_value() -> u32 {
    TRANSCONF_INTERLACE_PF_PD_ILK
}

/// `intel_enable_transcoder()`'s final write: the value read back from `TRANSCONF` ORed with `TRANSCONF_ENABLE`.
pub const fn transconf_enable_value(current: u32) -> u32 {
    current | TRANSCONF_ENABLE
}

/// `hsw_set_frame_start_delay()`: `intel_de_rmw(CHICKEN_TRANS, HSW_FRAME_START_DELAY_MASK, HSW_FRAME_START_DELAY(delay - 1))`.
pub const fn frame_start_delay_rmw(delay: u32) -> (u32, u32) {
    (HSW_FRAME_START_DELAY_MASK, ((delay - 1) << HSW_FRAME_START_DELAY_SHIFT) & HSW_FRAME_START_DELAY_MASK)
}

/// The mode flags `intel_ddi_transcoder_func_reg_val_get` reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncFlags {
    pub phsync: bool,
    pub pvsync: bool,
}

/// `intel_ddi_transcoder_func_reg_val_get()` for the DP SST output of DDI A on transcoder EDP, pipe A, no panel fitter forcing
/// ("`pch_pfit.force_thru`" false -> `TRANS_DDI_EDP_INPUT_A_ON`) — intel_ddi.c:512-623.
pub fn ddi_func_ctl_value(port: Ddi, t: Transcoder, pipe: Pipe, pipe_bpp: u32, sync: SyncFlags, lanes: u32) -> Result<u32, ModeError> {
    if port != Ddi::A || t != Transcoder::Edp || pipe != Pipe::A {
        return Err(ModeError::NotEdpOnDdiA);
    }
    let mut v = TRANS_DDI_FUNC_ENABLE | ((port as u32) << TRANS_DDI_PORT_SHIFT); // TRANS_DDI_SELECT_PORT (DISPLAY_VER < 12)
    v |= match pipe_bpp {
        18 => TRANS_DDI_BPC_6,
        24 => TRANS_DDI_BPC_8,
        30 => TRANS_DDI_BPC_10,
        bpp => return Err(ModeError::UnsupportedBpp { bpp }),
    };
    if sync.pvsync {
        v |= TRANS_DDI_PVSYNC;
    }
    if sync.phsync {
        v |= TRANS_DDI_PHSYNC;
    }
    v |= TRANS_DDI_EDP_INPUT_A_ON;
    v |= TRANS_DDI_MODE_SELECT_DP_SST;
    // DDI_PORT_WIDTH(lane_count): encode 3 -> 4 else n - 1 into DDI_PORT_WIDTH_MASK.
    if !matches!(lanes, 1 | 2 | 4) {
        return Err(ModeError::BadLaneCount { lanes });
    }
    v |= ddi_port_width(lanes);
    Ok(v)
}

/// `intel_link_compute_m_n()` output — intel_display_types.h `struct intel_link_m_n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkMn {
    pub tu: u32,
    pub data_m: u32,
    pub data_n: u32,
    pub link_m: u32,
    pub link_n: u32,
}

fn reduce_ratio(num: &mut u32, den: &mut u32) {
    // intel_reduce_m_n_ratio(): shift both until each fits DATA_LINK_M_N_MASK.
    while *num > DATA_LINK_M_N_MASK || *den > DATA_LINK_M_N_MASK {
        *num >>= 1;
        *den >>= 1;
    }
}

fn compute_m_n(m: u32, n: u32, constant_n: u32) -> Result<(u32, u32), ModeError> {
    if n == 0 {
        return Err(ModeError::ZeroDivisor { what: "compute_m_n" });
    }
    let mut ret_n = if constant_n != 0 { constant_n } else { core::cmp::min(n.next_power_of_two(), DATA_LINK_N_MAX) };
    let mut ret_m = ((m as u64 * ret_n as u64) / n as u64) as u32;
    reduce_ratio(&mut ret_m, &mut ret_n);
    Ok((ret_m, ret_n))
}

/// `intel_link_compute_m_n(bpp_x16, nlanes, pixel_clock, link_clock, bw_overhead = 1000000 (no FEC), ..)`.
/// `link_clock` is the port clock in 10 kbit/s units as Linux calls it (270000 for HBR = 2.7 Gb/s); the 8b/10b channel coding
/// efficiency is `drm_dp_bw_channel_coding_efficiency(false)` = 800000.
pub fn compute_link_mn(pixel_clock_khz: u32, link_clock: u32, lanes: u32, pipe_bpp: u32) -> Result<LinkMn, ModeError> {
    let bpp_x16 = pipe_bpp * 16;
    let overhead: u64 = 1_000_000; // intel_dp_bw_fec_overhead(false)
    // intel_dp_effective_data_rate(): DIV_ROUND_UP_ULL(pixel_clock * bpp_x16 * bw_overhead, 1000000 * 16 * 8)
    let data_m = ((pixel_clock_khz as u64 * bpp_x16 as u64 * overhead).div_ceil(1_000_000 * 16 * 8)) as u32;
    // drm_dp_max_dprx_data_rate(): DIV_ROUND_DOWN_ULL(link_rate * 10 * lanes * 800000, 1000000 * 8)
    let data_n = ((link_clock as u64 * 10 * lanes as u64 * 800_000) / (1_000_000 * 8)) as u32;
    // intel_dp_link_symbol_clock(): DIV_ROUND_CLOSEST(rate * 10, 10) for a non-UHBR rate.
    let link_symbol_clock = (link_clock * 10 + 5) / 10;
    let (dm, dn) = compute_m_n(data_m, data_n, 0x800_0000)?; // data: constant N 0x8000000
    let (lm, ln) = compute_m_n(pixel_clock_khz, link_symbol_clock, 0x8_0000)?; // link: constant N 0x80000
    Ok(LinkMn { tu: 64, data_m: dm, data_n: dn, link_m: lm, link_n: ln })
}

/// `intel_set_m_n()` via `intel_cpu_transcoder_set_m1_n1()`: data M (with TU size), data N, link M, and link N LAST ("On BDW+
/// writing LINK_N arms the double buffered update of all the M/N registers").
pub fn mn_writes(t: Transcoder, mn: &LinkMn) -> RegWrites<4> {
    let mut w = RegWrites::new();
    w.push(pipe_data_m1(t), (((mn.tu - 1) << TU_SIZE_SHIFT) & TU_SIZE_MASK) | mn.data_m);
    w.push(pipe_data_n1(t), mn.data_n);
    w.push(pipe_link_m1(t), mn.link_m);
    w.push(pipe_link_n1(t), mn.link_n);
    w
}

/// `skl_linetime_wm()` — `DIV_ROUND_UP(htotal * 1000 * 8, pixel_rate)` clamped to 0x1ff (no GLK/BXT IPC halving on KBL).
pub fn linetime_wm(htotal: u32, pixel_rate_khz: u32) -> Result<u32, ModeError> {
    if pixel_rate_khz == 0 {
        return Err(ModeError::ZeroDivisor { what: "pixel_rate" });
    }
    Ok(core::cmp::min((htotal * 1000 * 8).div_ceil(pixel_rate_khz), 0x1ff))
}

/// `hsw_set_linetime_wm()`: `HSW_LINETIME(linetime) | HSW_IPS_LINETIME(ips_linetime)`; `hsw_crtc_supports_ips` is false on Kaby
/// Lake so `ips_linetime` is 0.
pub const fn linetime_reg_value(linetime: u32) -> u32 {
    (linetime & HSW_LINETIME_MASK) | (0 << HSW_IPS_LINETIME_SHIFT)
}
