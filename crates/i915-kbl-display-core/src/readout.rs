// SPDX-License-Identifier: GPL-2.0-only
//! The whole takeover readout: power wells, CDCLK, the four DPLLs, the five DDI buffers and per pipe the
//! transcoder, mode, PLL, link, planes and scaler - what Linux's `intel_modeset_readout_hw_state` assembles from
//! `hsw_get_pipe_config` + `intel_ddi_get_config` at driver load, without a single register write.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display.c (`hsw_get_pipe_config`, `hsw_get_transcoder_state`),
//! intel_ddi.c (`intel_ddi_get_config`, `intel_ddi_read_func_ctl`), intel_dpll_mgr.c, intel_cdclk.c,
//! skl_universal_plane.c and skl_scaler.c. Copyright (c) Intel Corporation.
//!
//! Input: a register-read closure `FnMut(u32) -> u32` taking a BAR0 byte offset. The closure is called ONLY for
//! registers whose power domain is on (see `power`), exactly as Linux gates every read on
//! `intel_display_power_get_if_enabled`.

use crate::cdclk::{decode_cdclk, CdclkState};
use crate::ddi::{decode_ddi_buf, DdiBuf};
use crate::dpll::*;
use crate::error::ReadoutError;
use crate::platform::DisplayCaps;
use crate::plane::*;
use crate::power::*;
use crate::regs::*;
use crate::timing::*;
use crate::transcoder::*;
use crate::types::{Ddi, Pipe, Transcoder};

/// A DDI port as found: buffer state (if its lanes are powered) and the clock routing in DPLL_CTRL2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DdiReadout {
    pub ddi: Ddi,
    /// `None` when the port's lane power domain is off (the read is skipped).
    pub buf: Option<DdiBuf>,
    pub clock: DdiClock,
}

/// Everything known about an active pipe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActivePipe {
    pub transcoder: Transcoder,
    pub func: FuncCtl,
    pub transconf: Transconf,
    /// eDP transcoder fed from pipe A with the panel fitter in the path (`TRANS_DDI_EDP_INPUT_A_ONOFF`).
    pub edp_force_thru: bool,
    pub timings: ModeTimings,
    /// PIPESRC: the source size the pipe scans out (the mode's hdisplay x vdisplay, intel_display.c:2310-2312).
    pub src_w: u32,
    pub src_h: u32,
    /// `TRANS_MULT + 1` for non-eDP transcoders, else 1 (intel_display.c:4033-4040).
    pub pixel_multiplier: u32,
    /// DP data/link M/N (only for DP modes).
    pub link_mn: Option<LinkMn>,
    /// The PLL id DPLL_CTRL2 routes the DDI's clock from; `None` when the select override is clear.
    pub pll: Option<u8>,
    /// Why the PLL gave no port clock, if it did not.
    pub pll_error: Option<ReadoutError>,
    /// The port clock in kHz (DP: 162000 / 270000 / 540000; HDMI/DVI: the pixel clock).
    pub port_clock_khz: Option<u32>,
    /// `crtc_clock` in kHz (`intel_crtc_dotclock`); `None` when no port clock is known.
    pub dotclock_khz: Option<u32>,
    /// HDMI/DVI with more than 8 bpc: Linux divides by `bpp/24` only when the sink is HDMI, which a register
    /// readout cannot tell, so the dotclock above may be that factor too high.
    pub dotclock_deep_colour_unknown: bool,
    /// The DDI buffer of `func.ddi`, if readable.
    pub ddi_buf: Option<DdiBuf>,
    /// DP only: the transcoder's lane count disagrees with its DDI buffer's own width field.
    pub lanes_disagree_with_buf: bool,
}

impl ActivePipe {
    /// Whole-Hz refresh rate (`drm_mode_vrefresh`) if the dotclock is known.
    pub fn refresh_hz(&self) -> Option<u32> {
        self.dotclock_khz.map(|c| vrefresh_hz(c, &self.timings))
    }
    pub fn refresh_millihz(&self) -> Option<u32> {
        self.dotclock_khz.map(|c| vrefresh_millihz(c, &self.timings))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipeState {
    /// The pipe's register block sits in a well that is off: nothing was read.
    NotPowered,
    /// No transcoder is enabled for the pipe.
    Disabled,
    /// A transcoder has TRANS_DDI_FUNC_ENABLE but TRANSCONF is off (Linux: pipe inactive).
    FuncOnPipeOff(Transcoder),
    Active(ActivePipe),
    /// A register held a value this crate refuses to guess about.
    Error(ReadoutError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipeReadout {
    pub pipe: Pipe,
    pub state: PipeState,
    /// The three universal planes, read whenever the pipe's block is powered (their registers keep leftovers when
    /// the pipe is off, which is exactly what a takeover wants to know about). `None` if not powered.
    pub planes: [Option<Result<PlaneState, ReadoutError>>; 3],
    /// The pipe's scalers (2 on A and B, 1 on C); unused slots `None`.
    pub scalers: [Option<ScalerState>; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReadoutState {
    pub power: PowerState,
    pub cdclk: CdclkState,
    pub dplls: [Result<DpllState, ReadoutError>; NUM_DPLLS],
    pub ddis: [DdiReadout; 5],
    pub pipes: [PipeReadout; 3],
}

fn read_power<R: FnMut(u32) -> u32>(rd: &mut R) -> PowerState {
    let bios = rd(HSW_PWR_WELL_CTL1);
    let drv = rd(HSW_PWR_WELL_CTL2);
    let mut wells = [decode_well(Well::Pw1, bios, drv); 7];
    for w in Well::ALL {
        wells[w as usize] = decode_well(w, bios, drv);
    }
    PowerState { wells, fuse_status: rd(SKL_FUSE_STATUS), dc_state: decode_dc_state(rd(DC_STATE_EN)) }
}

/// Read the display state the firmware left, writing nothing.
pub fn read_display_state<R: FnMut(u32) -> u32>(rd: &mut R, caps: &DisplayCaps) -> ReadoutState {
    let power = read_power(rd);

    let lcpll1 = rd(LCPLL1_CTL);
    let ctrl1 = rd(DPLL_CTRL1);
    let ctrl2 = rd(DPLL_CTRL2);
    let status = rd(DPLL_STATUS);
    let cdclk = decode_cdclk(lcpll1, ctrl1, rd(CDCLK_CTL));

    let dplls: [Result<DpllState, ReadoutError>; NUM_DPLLS] = core::array::from_fn(|i| {
        let id = i as u8;
        let ctl = if id == 0 { lcpll1 } else { rd(dpll_ctl(id)) };
        // DPLL0 has no CFGCR registers (intel_dpll_mgr.c:1357-1360: "DPLL 0 doesn't support HDMI mode")
        let (c1, c2) = if id >= 1 { (rd(dpll_cfgcr1(id)), rd(dpll_cfgcr2(id))) } else { (0, 0) };
        decode_dpll(id, ctl, status, ctrl1, c1, c2)
    });

    let mut ddis = [DdiReadout { ddi: Ddi::A, buf: None, clock: decode_ctrl2(Ddi::A, ctrl2) }; 5];
    for d in Ddi::ALL {
        let buf = if caps.has_port(d) && power.ddi_lanes_powered(d) { Some(decode_ddi_buf(rd(ddi_buf_ctl(d)))) } else { None };
        ddis[d.index()] = DdiReadout { ddi: d, buf, clock: decode_ctrl2(d, ctrl2) };
    }

    let mut pipes = [empty_pipe(Pipe::A); 3];
    for p in Pipe::ALL {
        pipes[p.index()] = if caps.has_pipe(p) { read_pipe(rd, caps, &power, &dplls, &ddis, p) } else { empty_pipe(p) };
    }

    ReadoutState { power, cdclk, dplls, ddis, pipes }
}

fn empty_pipe(pipe: Pipe) -> PipeReadout {
    PipeReadout { pipe, state: PipeState::NotPowered, planes: [None; 3], scalers: [None; 2] }
}

fn read_pipe<R: FnMut(u32) -> u32>(
    rd: &mut R,
    caps: &DisplayCaps,
    power: &PowerState,
    dplls: &[Result<DpllState, ReadoutError>; NUM_DPLLS],
    ddis: &[DdiReadout; 5],
    pipe: Pipe,
) -> PipeReadout {
    let mut out = empty_pipe(pipe);
    if !power.pipe_powered(pipe) {
        return out; // POWER_DOMAIN_PIPE off: hsw_get_pipe_config returns false before reading anything
    }
    for id in 0..3u32 {
        let r = |base: u32| plane_reg(base, pipe, id);
        let (ctl, stride, size, pos, off, surf) = (
            rd(r(PLANE_CTL_1_A)),
            rd(r(PLANE_STRIDE_1_A)),
            rd(r(PLANE_SIZE_1_A)),
            rd(r(PLANE_POS_1_A)),
            rd(r(PLANE_OFFSET_1_A)),
            rd(r(PLANE_SURF_1_A)),
        );
        out.planes[id as usize] = Some(decode_plane(id as u8, ctl, stride, size, pos, off, surf));
    }
    for id in 0..caps.scalers_of(pipe) as u32 {
        out.scalers[id as usize] = Some(decode_scaler(
            id as u8,
            rd(scaler_reg(PS_1A_CTRL, pipe, id)),
            rd(scaler_reg(PS_WIN_POS_1A, pipe, id)),
            rd(scaler_reg(PS_WIN_SZ_1A, pipe, id)),
        ));
    }
    out.state = pipe_state(rd, caps, power, dplls, ddis, pipe);
    out
}

fn pipe_state<R: FnMut(u32) -> u32>(
    rd: &mut R,
    caps: &DisplayCaps,
    power: &PowerState,
    dplls: &[Result<DpllState, ReadoutError>; NUM_DPLLS],
    ddis: &[DdiReadout; 5],
    pipe: Pipe,
) -> PipeState {
    let own = Transcoder::of_pipe(pipe);
    let edp_ctl = if caps.has_transcoder(Transcoder::Edp) && power.transcoder_powered(Transcoder::Edp) {
        rd(trans_ddi_func_ctl(Transcoder::Edp))
    } else {
        0
    };
    let own_ctl = if power.transcoder_powered(own) { rd(trans_ddi_func_ctl(own)) } else { 0 };
    let mask = match enabled_transcoders(pipe, edp_ctl, own_ctl) {
        Ok(m) => m,
        Err(e) => return PipeState::Error(e),
    };
    let t = match pick_transcoder(pipe, mask) {
        Ok(Some(t)) => t,
        Ok(None) => return PipeState::Disabled,
        Err(e) => return PipeState::Error(e),
    };
    let func_raw = if t == Transcoder::Edp { edp_ctl } else { own_ctl };
    let transconf = decode_transconf(rd(pipe_reg(t, TRANSACONF)));
    if !transconf.enabled {
        return PipeState::FuncOnPipeOff(t);
    }
    let func = match decode_func_ctl(t, func_raw) {
        Ok(Some(f)) => f,
        Ok(None) => return PipeState::Disabled, // cannot happen: the mask came from FUNC_ENABLE
        Err(e) => return PipeState::Error(e),
    };

    let raw = RawTimings {
        htotal: rd(trans_reg(t, TRANS_HTOTAL_A)),
        hblank: rd(trans_reg(t, TRANS_HBLANK_A)),
        hsync: rd(trans_reg(t, TRANS_HSYNC_A)),
        vtotal: rd(trans_reg(t, TRANS_VTOTAL_A)),
        vblank: rd(trans_reg(t, TRANS_VBLANK_A)),
        vsync: rd(trans_reg(t, TRANS_VSYNC_A)),
    };
    let timings = decode_timings(&raw, transconf.interlaced);
    let (src_w, src_h) = decode_pipe_src(rd(trans_reg(own, PIPEASRC)));
    let pixel_multiplier = if t == Transcoder::Edp { 1 } else { rd(trans_reg(t, TRANS_MULT_A)).wrapping_add(1) };

    let link_mn = if func.mode.is_dp() {
        Some(decode_m_n(
            rd(trans_reg(t, PIPEA_DATA_M1)),
            rd(trans_reg(t, PIPEA_DATA_N1)),
            rd(trans_reg(t, PIPEA_LINK_M1)),
            rd(trans_reg(t, PIPEA_LINK_N1)),
        ))
    } else {
        None
    };

    let mut pll = None;
    let mut pll_error = None;
    let mut port_clock_khz = None;
    let mut ddi_buf = None;
    if let Some(d) = func.ddi {
        let ro = &ddis[d.index()];
        ddi_buf = ro.buf;
        if ro.clock.select_override {
            let id = ro.clock.pll;
            pll = Some(id);
            match &dplls[id as usize] {
                Ok(st) if st.enabled => port_clock_khz = Some(st.port_clock_khz),
                Ok(_) => pll_error = Some(ReadoutError::PllNotEnabled { ddi: d.letter(), pll: id }),
                Err(e) => pll_error = Some(*e),
            }
        }
    }

    let mult = if pixel_multiplier == 0 { 1 } else { pixel_multiplier };
    let dotclock_khz = port_clock_khz.map(|pc| {
        let base = match &link_mn {
            Some(mn) => dotclock_from_m_n(pc, mn),
            None => pc, // intel_crtc_dotclock, the non-DP branch (intel_display.c:4097)
        };
        base / mult // intel_display.c:4103-4104
    });
    let lanes_disagree_with_buf = match (func.lanes, ddi_buf.and_then(|b| b.width_lanes)) {
        (Some(a), Some(b)) => func.mode.is_dp() && a != b,
        _ => false,
    };

    PipeState::Active(ActivePipe {
        transcoder: t,
        func,
        transconf,
        edp_force_thru: t == Transcoder::Edp && edp_force_thru(func_raw),
        timings,
        src_w,
        src_h,
        pixel_multiplier,
        link_mn,
        pll,
        pll_error,
        port_clock_khz,
        dotclock_khz,
        dotclock_deep_colour_unknown: !func.mode.is_dp() && func.bpp > 24,
        ddi_buf,
        lanes_disagree_with_buf,
    })
}
