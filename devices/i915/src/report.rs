// SPDX-License-Identifier: GPL-2.0-only
//! The readout as log lines: one `[I915-KBL-READOUT …]` line per pipe, then the power wells, CDCLK, the DDIs and DPLLs
//! that are on, and any further plane or scaler that is enabled. These are the lines a Linux reconnaissance boot's
//! `i915_display_info` debugfs file is compared against.
use crate::log::{emit, Log};
use i915_kbl_display_core::cdclk::CdclkAnomaly;
use i915_kbl_display_core::plane::{fourcc_name, PlaneState, ScalerBinding};
use i915_kbl_display_core::power::{DcState, Well};
use i915_kbl_display_core::readout::{ActivePipe, PipeState, ReadoutState};
use i915_kbl_display_core::{Pipe, Transcoder};

fn b(v: bool) -> u8 {
    v as u8
}

fn trans_name(t: Transcoder) -> &'static str {
    t.name()
}

/// The pipe's main line. See the module note; the format is
/// `[I915-KBL-READOUT pipe=A active=1 1920x1080@60 trans=EDP ddi=A lanes=2 rate=2700 pll=0 plane=XR24 tiling=X stride=7680 surf=0x100000]`
/// where `rate` is the DisplayPort lane rate in Mbit/s (`-` for HDMI/DVI) and `plane` is the primary plane's DRM fourcc
/// (`off` when disabled).
pub fn pipe_line<L: Log>(log: &mut L, state: &ReadoutState, pipe: Pipe) {
    let p = &state.pipes[pipe.index()];
    let plane = match p.planes[0] {
        Some(Ok(pl)) if pl.enabled => Some(pl),
        _ => None,
    };
    match p.state {
        PipeState::Active(a) => {
            emit(log, format_args!("[I915-KBL-READOUT pipe={} active=1 {}x{}@{} trans={} ddi={} lanes={} rate={} pll={} {}]",
                pipe.letter(), a.src_w, a.src_h, OptNum(a.refresh_hz()), trans_name(a.transcoder), ddi_letter(&a), lanes(&a), rate(&a), pll(&a), plane_fields(plane)));
        }
        PipeState::Disabled => emit(log, format_args!("[I915-KBL-READOUT pipe={} active=0 state=disabled {}]", pipe.letter(), plane_fields(plane))),
        PipeState::NotPowered => emit(log, format_args!("[I915-KBL-READOUT pipe={} active=0 state=not-powered]", pipe.letter())),
        PipeState::FuncOnPipeOff(t) => {
            emit(log, format_args!("[I915-KBL-READOUT pipe={} active=0 state=func-on-pipe-off trans={}]", pipe.letter(), trans_name(t)))
        }
        PipeState::Error(e) => emit(log, format_args!("[I915-KBL-READOUT pipe={} active=0 state=error why={}]", pipe.letter(), e)),
    }
}

/// A number, or `?` when it is not known (a dotclock that no PLL gave).
struct OptNum(Option<u32>);
impl core::fmt::Display for OptNum {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(n) => write!(f, "{}", n),
            None => f.write_str("?"),
        }
    }
}

fn ddi_letter(a: &ActivePipe) -> char {
    a.func.ddi.map(|d| d.letter()).unwrap_or('-')
}

struct Lanes(Option<u32>);
impl core::fmt::Display for Lanes {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(n) => write!(f, "{}", n),
            None => f.write_str("-"),
        }
    }
}
fn lanes(a: &ActivePipe) -> Lanes {
    Lanes(a.func.lanes)
}

struct Rate<'a>(&'a ActivePipe);
impl core::fmt::Display for Rate<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let a = self.0;
        if !a.func.mode.is_dp() {
            return f.write_str("-");
        }
        match (a.port_clock_khz, a.pll_error) {
            (Some(k), _) => write!(f, "{}", k / 100),
            (None, Some(e)) => write!(f, "?({})", e.name()),
            (None, None) => f.write_str("?(no-pll)"),
        }
    }
}
fn rate(a: &ActivePipe) -> Rate<'_> {
    Rate(a)
}

struct Pll(Option<u8>);
impl core::fmt::Display for Pll {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(id) => write!(f, "{}", id),
            None => f.write_str("none"),
        }
    }
}
fn pll(a: &ActivePipe) -> Pll {
    Pll(a.pll)
}

struct PlaneFields(Option<PlaneState>);
impl core::fmt::Display for PlaneFields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Some(p) = self.0 else { return f.write_str("plane=off") };
        let fourcc = p.format.map(|fm| fourcc_name(fm, p.rgb_order, p.alpha)).unwrap_or("?");
        let tiling = p.tiling.map(|t| t.name()).unwrap_or("?");
        write!(f, "plane={} tiling={} ", fourcc, tiling)?;
        match p.stride_bytes {
            Some(s) => write!(f, "stride={} ", s)?,
            None => write!(f, "stride=? ")?,
        }
        write!(f, "size={}x{} surf={:#x}", p.width, p.height, p.surf)
    }
}
fn plane_fields(p: Option<PlaneState>) -> PlaneFields {
    PlaneFields(p)
}

/// The timing line of an active pipe: the full mode, the clock it implies and the DP link M/N.
pub fn timing_line<L: Log>(log: &mut L, pipe: Pipe, a: &ActivePipe) {
    let t = &a.timings;
    emit(log, format_args!(
        "[I915-KBL-READOUT-TIMING pipe={} trans={} h={}/{} hs={}-{} hb={}-{} v={}/{} vs={}-{} vb={}-{} dot_khz={} mhz={} il={} mult={} bpp={} hsync={} vsync={} src={}x{}{}]",
        pipe.letter(), trans_name(a.transcoder), t.hdisplay, t.htotal, t.hsync_start, t.hsync_end, t.hblank_start, t.hblank_end,
        t.vdisplay, t.vtotal, t.vsync_start, t.vsync_end, t.vblank_start, t.vblank_end,
        OptNum(a.dotclock_khz), OptNum(a.refresh_millihz()), b(t.interlaced), a.pixel_multiplier, a.func.bpp,
        if a.func.hsync_positive { '+' } else { '-' }, if a.func.vsync_positive { '+' } else { '-' }, a.src_w, a.src_h,
        MnFields(a.link_mn)));
    if a.lanes_disagree_with_buf {
        emit(log, format_args!("[I915-KBL-READOUT-WARN pipe={} lanes-transcoder-vs-ddi-buffer-disagree]", pipe.letter()));
    }
    if a.dotclock_deep_colour_unknown {
        emit(log, format_args!("[I915-KBL-READOUT-WARN pipe={} dotclock-assumes-8bpc-hdmi-deep-colour-unknown]", pipe.letter()));
    }
    if let Some(e) = a.pll_error {
        emit(log, format_args!("[I915-KBL-READOUT-WARN pipe={} pll-error {}]", pipe.letter(), e));
    }
}

struct MnFields(Option<i915_kbl_display_core::timing::LinkMn>);
impl core::fmt::Display for MnFields {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            Some(m) => write!(f, " dm={}/{} lm={}/{} tu={}", m.data_m, m.data_n, m.link_m, m.link_n, m.tu),
            None => Ok(()),
        }
    }
}

/// Power wells, DC state and fuses.
pub fn power_line<L: Log>(log: &mut L, state: &ReadoutState) {
    let p = &state.power;
    let dc = match p.dc_state.state {
        DcState::Off => "off",
        DcState::UpToDc5 => "dc5",
        DcState::UpToDc6 => "dc6",
        DcState::Reserved => "reserved",
    };
    emit(log, format_args!(
        "[I915-KBL-READOUT-POWER pw1={} pw2={} misc_io={} ddi_ae={} ddi_b={} ddi_c={} ddi_d={} fuse={:#010x} dc_state={} dc9={} ctl_bios={} ctl_drv={}]",
        b(p.well(Well::Pw1).enabled), b(p.well(Well::Pw2).enabled), b(p.well(Well::MiscIo).enabled), b(p.well(Well::DdiAE).enabled),
        b(p.well(Well::DdiB).enabled), b(p.well(Well::DdiC).enabled), b(p.well(Well::DdiD).enabled), p.fuse_status, dc,
        b(p.dc_state.dc9), WellBits(state, true), WellBits(state, false)));
}

/// "PW1:rs ..." request/state bits per well of one register, compactly: r = request, s = state.
struct WellBits<'a>(&'a ReadoutState, bool);
impl core::fmt::Display for WellBits<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for (i, w) in Well::ALL.iter().enumerate() {
            let s = self.0.power.well(*w);
            let (r, st) = if self.1 { (s.bios_req, s.bios_state) } else { (s.drv_req, s.drv_state) };
            if i > 0 {
                f.write_str(",")?;
            }
            write!(f, "{}{}", if r { 'r' } else { '-' }, if st { 's' } else { '-' })?;
        }
        Ok(())
    }
}

/// CDCLK.
pub fn cdclk_line<L: Log>(log: &mut L, state: &ReadoutState) {
    let c = &state.cdclk;
    emit(log, format_args!("[I915-KBL-READOUT-CDCLK cdclk_khz={} vco_khz={} ref_khz={} voltage={} decimal={:#x} anomaly={}]",
        c.cdclk_khz, c.vco_khz, c.ref_khz, c.voltage_level, c.decimal, Anomaly(c.anomaly)));
}

struct Anomaly(CdclkAnomaly);
impl core::fmt::Display for Anomaly {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            CdclkAnomaly::None => f.write_str("none"),
            CdclkAnomaly::Dpll0NotLocked => f.write_str("dpll0-not-locked"),
            CdclkAnomaly::Dpll0Ctrl1Unexpected { ctrl1 } => write!(f, "dpll0-ctrl1-unexpected:{:#x}", ctrl1),
            CdclkAnomaly::Dpll0LinkRateReserved { field } => write!(f, "dpll0-link-rate-reserved:{}", field),
        }
    }
}

/// One line per DDI whose buffer is on or whose clock is routed, and one per PLL that is enabled.
pub fn ddi_and_pll_lines<L: Log>(log: &mut L, state: &ReadoutState) {
    for d in state.ddis.iter() {
        let on = d.buf.map(|x| x.enabled).unwrap_or(false);
        if !on && !d.clock.select_override {
            continue;
        }
        match d.buf {
            Some(bf) => emit(log, format_args!(
                "[I915-KBL-READOUT-DDI ddi={} buf={} idle={} lanes={} reversed={} a4={} emp={} clk_override={} clk_sel=dpll{} clk_off={}]",
                d.ddi.letter(), if bf.enabled { "on" } else { "off" }, b(bf.idle), Lanes(bf.width_lanes), b(bf.reversed), b(bf.a_4_lanes),
                bf.trans_select, b(d.clock.select_override), d.clock.pll, b(d.clock.clk_off))),
            None => emit(log, format_args!("[I915-KBL-READOUT-DDI ddi={} buf=unreadable-not-powered clk_override={} clk_sel=dpll{}]",
                d.ddi.letter(), b(d.clock.select_override), d.clock.pll)),
        }
    }
    for r in state.dplls.iter() {
        match r {
            Ok(p) if p.enabled => emit(log, format_args!(
                "[I915-KBL-READOUT-DPLL id={} enabled=1 locked={} mode={} ssc={} rate_field={} port_khz={} ctrl1={:#04x}]",
                p.id, b(p.locked), if p.hdmi_mode { "hdmi" } else { "dp" }, b(p.ssc), p.link_rate_field, p.port_clock_khz, p.ctrl1)),
            Ok(_) => {}
            Err(e) => emit(log, format_args!("[I915-KBL-READOUT-DPLL error {}]", e)),
        }
    }
}

/// Enabled planes beyond a pipe's main line (the sprites), and every enabled or errored scaler.
pub fn extra_lines<L: Log>(log: &mut L, state: &ReadoutState) {
    for pr in state.pipes.iter() {
        for (i, pl) in pr.planes.iter().enumerate() {
            match pl {
                Some(Ok(p)) if p.enabled && i > 0 => emit(log, format_args!("[I915-KBL-READOUT-PLANE pipe={} id={} {}]", pr.pipe.letter(), i + 1, PlaneFields(Some(*p)))),
                Some(Err(e)) => emit(log, format_args!("[I915-KBL-READOUT-PLANE pipe={} id={} error {}]", pr.pipe.letter(), i + 1, e)),
                _ => {}
            }
        }
        for sc in pr.scalers.iter().flatten() {
            if sc.enabled {
                let bind = match sc.binding {
                    ScalerBinding::Pipe => BindName(None),
                    ScalerBinding::Plane(n) => BindName(Some(n + 1)),
                };
                emit(log, format_args!("[I915-KBL-READOUT-SCALER pipe={} id={} binding={} win={}x{}+{}+{}]", pr.pipe.letter(), sc.id, bind, sc.win_w, sc.win_h, sc.win_x, sc.win_y));
            }
        }
    }
}

struct BindName(Option<u8>);
impl core::fmt::Display for BindName {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self.0 {
            None => f.write_str("pipe"),
            Some(n) => write!(f, "plane{}", n),
        }
    }
}

/// Every line of the readout, in order.
pub fn all<L: Log>(log: &mut L, state: &ReadoutState) -> u32 {
    let mut active = 0;
    for p in Pipe::ALL {
        pipe_line(log, state, p);
        if let PipeState::Active(a) = state.pipes[p.index()].state {
            active += 1;
            timing_line(log, p, &a);
        }
    }
    power_line(log, state);
    cdclk_line(log, state);
    ddi_and_pll_lines(log, state);
    extra_lines(log, state);
    active
}
