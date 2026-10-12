// SPDX-License-Identifier: GPL-2.0-only
//! End-to-end: whole register snapshots built by hand from Linux's encode rules, fed through `read_display_state`,
//! the decoded state asserted field by field. The register file is STRICT (see common/mod.rs): reading an address that
//! has no value panics, so a wrong offset anywhere fails the test instead of reading a convenient zero.
mod common;

use common::Regs;
use i915_kbl_display_core::cdclk::CdclkAnomaly;
use i915_kbl_display_core::error::ReadoutError;
use i915_kbl_display_core::plane::{PlaneFormat, Tiling};
use i915_kbl_display_core::power::{DcState, Well};
use i915_kbl_display_core::readout::*;
use i915_kbl_display_core::transcoder::DdiMode;
use i915_kbl_display_core::types::*;
use i915_kbl_display_core::KBL_DISPLAY;

fn run(r: &Regs) -> ReadoutState {
    let mut rd = |a: u32| r.read(a);
    read_display_state(&mut rd, &KBL_DISPLAY)
}

/// Timing registers of one transcoder at `base` (0x60000 A, 0x61000 B, 0x62000 C, 0x6f000 EDP).
fn timings(r: &mut Regs, base: u32, t: [u32; 6], src: u32) {
    for (i, v) in t.iter().enumerate() {
        r.set(base + 4 * i as u32, *v);
    }
    r.set(base + 0x1c, src); // PIPESRC
}

/// DP link M/N registers of one transcoder.
fn link_mn(r: &mut Regs, base: u32, data_m: u32, data_n: u32, link_m: u32, link_n: u32) {
    r.set(base + 0x30, data_m).set(base + 0x34, data_n).set(base + 0x40, link_m).set(base + 0x44, link_n);
}

// 1080p60 timing registers (hand-encoded from intel_display.c:2704-2731, see tests/timing.rs)
const HD: [u32; 6] = [0x0897_077f, 0x0897_077f, 0x0803_07d7, 0x0464_0437, 0x0464_0437, 0x0440_043b];
const HD_SRC: u32 = 0x077f_0437;
// 2160p60
const K4: [u32; 6] = [0x112f_0eff, 0x112f_0eff, 0x1007_0faf, 0x08c9_086f, 0x08c9_086f, 0x0881_0877];
const K4_SRC: u32 = 0x0eff_086f;

/// Vector A: the Pixelbook Go's eDP panel as a 1080p60 GOP framebuffer leaves it. Pipe A -> TRANSCODER_EDP (input A_ON)
/// -> DDI A, DP-SST, 2 lanes, 24 bpp, +hsync +vsync, DPLL0 at HBR (2.7 Gbit/s), CDCLK 450 MHz on the 8.1 GHz VCO,
/// primary plane XRGB8888 X-tiled at GGTT 0x100000, stride 15 tiles.
fn vector_a() -> Regs {
    let mut r = Regs::firmware_idle();
    // DPLL0 + CDCLK
    r.set(0x46010, 0xc000_0000).set(0x6c058, 0x0000_0003).set(0x6c05c, 0x0000_0001).set(0x6c060, 0x0000_0001);
    r.set(0x46000, 0x0000_0382); // CDCLK_FREQ_SEL 450_432, decimal 898
    // eDP transcoder
    r.set(0x6f400, 0x8203_0002); // TRANS_DDI_FUNC_CTL(EDP)
    r.set(0x7f008, 0x8000_0000); // TRANSCONF(EDP)
    r.set(0x64000, 0x8000_0002); // DDI_BUF_CTL(A): enabled, 2 lanes
    timings(&mut r, 0x6f000, HD, HD_SRC);
    // PIPESRC is a PIPE register at the pipe's transcoder-A-class offset, not the eDP transcoder's
    r.remove(0x6f01c).set(0x6001c, HD_SRC);
    link_mn(&mut r, 0x6f000, 0x7e69_9999, 0x0080_0000, 288_358, 0x0008_0000);
    // pipe A primary plane
    r.set(0x70180, 0x8400_0400).set(0x70188, 15).set(0x70190, 0x0437_077f).set(0x7019c, 0x0010_0005);
    r
}

#[test]
fn vector_a_edp_1080p60_on_dpll0() {
    let r = vector_a();
    let s = run(&r);
    let a = match s.pipes[0].state {
        PipeState::Active(a) => a,
        other => panic!("pipe A not active: {other:?}"),
    };
    assert_eq!(a.transcoder, Transcoder::Edp);
    assert_eq!(a.func.mode, DdiMode::DpSst);
    assert_eq!(a.func.ddi, Some(Ddi::A));
    assert_eq!(a.func.lanes, Some(2));
    assert_eq!(a.func.bpp, 24);
    assert!(a.func.hsync_positive && a.func.vsync_positive);
    assert!(!a.edp_force_thru);
    assert_eq!((a.src_w, a.src_h), (1920, 1080));
    assert_eq!((a.timings.hdisplay, a.timings.htotal, a.timings.vdisplay, a.timings.vtotal), (1920, 2200, 1080, 1125));
    assert_eq!((a.timings.hsync_start, a.timings.hsync_end), (2008, 2052));
    assert_eq!((a.timings.vsync_start, a.timings.vsync_end), (1084, 1089));
    assert!(!a.timings.interlaced);
    assert_eq!(a.pixel_multiplier, 1);
    assert_eq!(a.pll, Some(0));
    assert_eq!(a.pll_error, None);
    assert_eq!(a.port_clock_khz, Some(270_000)); // HBR
    assert_eq!(a.dotclock_khz, Some(148_500));
    assert_eq!(a.refresh_hz(), Some(60));
    assert_eq!(a.refresh_millihz(), Some(60_000));
    let mn = a.link_mn.unwrap();
    assert_eq!((mn.link_m, mn.link_n, mn.tu), (288_358, 524_288, 64));
    assert_eq!((mn.data_m, mn.data_n), (0x69_9999, 0x80_0000));
    assert!(a.ddi_buf.unwrap().enabled);
    assert!(!a.lanes_disagree_with_buf);
    assert!(a.transconf.enabled);

    // plane 1 of pipe A
    let p = s.pipes[0].planes[0].unwrap().unwrap();
    assert!(p.enabled);
    assert_eq!(p.format, Some(PlaneFormat::Xrgb8888));
    assert_eq!(p.tiling, Some(Tiling::X));
    assert_eq!((p.width, p.height), (1920, 1080));
    assert_eq!(p.stride_bytes, Some(7680));
    assert_eq!(p.surf, 0x0010_0000);
    assert!(!p.alpha && !p.rgb_order);
    // planes 2 and 3 off; the scalers off
    assert!(!s.pipes[0].planes[1].unwrap().unwrap().enabled);
    assert!(!s.pipes[0].planes[2].unwrap().unwrap().enabled);
    assert!(s.pipes[0].scalers[0].is_some() && s.pipes[0].scalers[1].is_some());
    assert!(!s.pipes[0].scalers[0].unwrap().enabled);

    // CDCLK
    assert_eq!(s.cdclk.vco_khz, 8_100_000);
    assert_eq!(s.cdclk.cdclk_khz, 450_000);
    assert_eq!(s.cdclk.decimal, 898);
    assert_eq!(s.cdclk.anomaly, CdclkAnomaly::None);

    // DPLL0
    let d0 = s.dplls[0].unwrap();
    assert!(d0.enabled && d0.locked && d0.override_set);
    assert_eq!(d0.port_clock_khz, 270_000);
    for id in 1..4 {
        assert!(!s.dplls[id].unwrap().enabled, "dpll {id}");
    }

    // the other pipes and ports are idle
    assert_eq!(s.pipes[1].state, PipeState::Disabled);
    assert_eq!(s.pipes[2].state, PipeState::Disabled);
    assert!(s.ddis[0].buf.unwrap().enabled);
    for d in 1..5 {
        assert!(!s.ddis[d].buf.unwrap().enabled, "ddi {d}");
    }
    // DDI A routes its clock from DPLL0
    assert!(s.ddis[0].clock.select_override && s.ddis[0].clock.pll == 0 && !s.ddis[0].clock.clk_off);

    // power: PW1 and MISC_IO only through the BIOS register, PW2 and DDI_A_E through the driver register
    assert!(s.power.well(Well::Pw1).enabled && s.power.well(Well::Pw2).enabled && s.power.well(Well::MiscIo).enabled);
    assert!(s.power.well(Well::DdiAE).enabled);
    assert!(!s.power.well(Well::DdiB).enabled && !s.power.well(Well::DdiC).enabled && !s.power.well(Well::DdiD).enabled);
    assert!(s.power.well(Well::Pw1).bios_req && !s.power.well(Well::Pw1).drv_req);
    assert!(s.power.fuse_download_done());
    assert_eq!(s.power.dc_state.state, DcState::Off);

    // the eDP transcoder's own PIPESRC copy was never read (it does not exist), and nothing unpowered was touched
    assert!(r.was_read(0x6001c) && !r.was_read(0x6f01c));
}

/// Vector B: a 4K eDP panel at 3840x2160@60 over 4 lanes of HBR2 on DPLL0, panel fitter in the path (A_ONOFF), DDI A
/// with all four lanes (DDI_A_4_LANES). CDCLK 540 MHz.
fn vector_b() -> Regs {
    let mut r = Regs::firmware_idle();
    r.set(0x46010, 0xc000_0000).set(0x6c058, 0x0000_0001).set(0x6c05c, 0x0000_0001).set(0x6c060, 1);
    r.set(0x46000, 0x0400_0436); // FREQ_SEL 540, decimal 1078
    r.set(0x6f400, 0x8203_4006); // EDP, DP-SST, 8bpc, +/+, input A_ONOFF, 4 lanes
    r.set(0x7f008, 0x8000_0000);
    r.set(0x64000, 0x8000_0016); // enabled, DDI_A_4_LANES, width 4
    timings(&mut r, 0x6f000, K4, K4_SRC);
    r.remove(0x6f01c).set(0x6001c, K4_SRC);
    link_mn(&mut r, 0x6f000, 0x7e69_9999, 0x0080_0000, 576_716, 0x0008_0000);
    r.set(0x70180, 0x8400_0400).set(0x70188, 30).set(0x70190, 0x086f_0eff).set(0x7019c, 0x0020_0000);
    // the panel fitter (scaler 1 of pipe A) bound to the pipe, window 3840x2160
    r.set(0x68180, 0x8000_0000).set(0x68170, 0).set(0x68174, 0x0f00_0870);
    r
}

#[test]
fn vector_b_4k_edp_hbr2_four_lanes() {
    let s = run(&vector_b());
    let a = match s.pipes[0].state {
        PipeState::Active(a) => a,
        other => panic!("{other:?}"),
    };
    assert_eq!(a.transcoder, Transcoder::Edp);
    assert!(a.edp_force_thru);
    assert_eq!(a.func.lanes, Some(4));
    assert!(a.ddi_buf.unwrap().a_4_lanes);
    assert!(!a.lanes_disagree_with_buf);
    assert_eq!((a.src_w, a.src_h), (3840, 2160));
    assert_eq!((a.timings.hdisplay, a.timings.htotal, a.timings.vdisplay, a.timings.vtotal), (3840, 4400, 2160, 2250));
    assert_eq!(a.port_clock_khz, Some(540_000)); // HBR2
    assert_eq!(a.dotclock_khz, Some(594_000));
    assert_eq!(a.refresh_hz(), Some(60));
    assert_eq!(s.cdclk.cdclk_khz, 540_000);
    assert_eq!(s.cdclk.vco_khz, 8_100_000);
    let p = s.pipes[0].planes[0].unwrap().unwrap();
    assert_eq!((p.width, p.height, p.stride_bytes), (3840, 2160, Some(15360)));
    assert_eq!(p.surf, 0x0020_0000);
    // the panel fitter is scaler 0 of pipe A (PS_CTRL_1A at 0x68180), bound to the pipe
    let s0 = s.pipes[0].scalers[0].unwrap();
    assert!(s0.enabled && s0.is_pipe_fitter());
    assert_eq!((s0.win_w, s0.win_h), (0x0f00, 0x0870));
    assert!(!s.pipes[0].scalers[1].unwrap().enabled);
}

#[test]
fn vector_c_everything_disabled() {
    let r = Regs::firmware_idle();
    let s = run(&r);
    for p in &s.pipes {
        assert_eq!(p.state, PipeState::Disabled, "pipe {:?}", p.pipe);
        for pl in p.planes.iter() {
            assert!(!pl.unwrap().unwrap().enabled);
        }
    }
    assert_eq!(s.cdclk.cdclk_khz, 24_000); // DPLL0 off: bypass clock
    assert_eq!(s.cdclk.vco_khz, 0);
    for d in s.dplls.iter() {
        assert!(!d.unwrap().enabled);
    }
    for d in s.ddis.iter() {
        assert!(!d.buf.unwrap().enabled);
    }
    // a pipe with a transcoder function enabled but TRANSCONF off is "func on, pipe off", not active
    let mut r = Regs::firmware_idle();
    r.set(0x6f400, 0x8203_0002).set(0x7f008, 0);
    assert_eq!(run(&r).pipes[0].state, PipeState::FuncOnPipeOff(Transcoder::Edp));
}

/// Vector D: pipe B -> TRANSCODER_B -> DDI B (an external DisplayPort sink behind the Type-C PS8751), DP-SST 4 lanes,
/// HBR from DPLL1 (LCPLL2), 1080p60.
fn vector_d() -> Regs {
    let mut r = Regs::firmware_idle();
    // DPLL0 stays up for CDCLK: CTRL1 slice 0x01 = override + rate field 0 (2700, VCO 8.1 GHz). DPLL1 (LCPLL2) is at HBR:
    // its slice is bits 6..=11, override (0x40) + rate field 1 (1350) << 7 (0x80) = 0xc0.
    r.set(0x46010, 0xc000_0000).set(0x6c058, 0x0000_00c1).set(0x6c060, 0x0000_0101);
    r.set(0x46014, 0x8000_0000); // LCPLL2_CTL enabled
    r.set(0x6c05c, 0x0000_0018); // DDI B: override (bit 3) + select DPLL1 (1 << 4)
    r.set(0x46000, 0);
    r.set(0x61400, 0x9203_0006); // TRANS_DDI_FUNC_CTL(B): enabled, port B, DP-SST, 8bpc, +/+, 4 lanes
    r.set(0x71008, 0x8000_0000);
    r.set(0x64100, 0x8000_0006); // DDI_BUF_CTL(B): enabled, 4 lanes
    r.set(0x6102c, 0); // TRANS_MULT(B)
    timings(&mut r, 0x61000, HD, HD_SRC);
    link_mn(&mut r, 0x61000, 0x7e34_cccc, 0x0080_0000, 288_358, 0x0008_0000);
    r.set(0x71180, 0x8400_0000).set(0x71188, 120).set(0x71190, 0x0437_077f).set(0x7119c, 0x0030_0000);
    r
}

#[test]
fn vector_d_pipe_b_external_dp_on_ddi_b_dpll1() {
    let s = run(&vector_d());
    assert_eq!(s.pipes[0].state, PipeState::Disabled);
    let a = match s.pipes[1].state {
        PipeState::Active(a) => a,
        other => panic!("{other:?}"),
    };
    assert_eq!(a.transcoder, Transcoder::B);
    assert_eq!(a.func.ddi, Some(Ddi::B));
    assert_eq!(a.func.lanes, Some(4));
    assert_eq!(a.pll, Some(1));
    assert_eq!(a.port_clock_khz, Some(270_000));
    assert_eq!(a.dotclock_khz, Some(148_500));
    assert_eq!(a.refresh_hz(), Some(60));
    assert_eq!(a.pixel_multiplier, 1);
    assert!(a.ddi_buf.unwrap().enabled);
    assert!(!a.lanes_disagree_with_buf);
    let d1 = s.dplls[1].unwrap();
    assert!(d1.enabled && d1.locked && d1.override_set);
    assert_eq!(d1.link_rate_field, 1);
    // the plane of pipe B: linear XRGB8888, 120 * 64 = 7680 bytes
    let p = s.pipes[1].planes[0].unwrap().unwrap();
    assert_eq!((p.format, p.tiling, p.stride_bytes), (Some(PlaneFormat::Xrgb8888), Some(Tiling::Linear), Some(7680)));
    assert_eq!(p.surf, 0x0030_0000);
    // CDCLK: DPLL0 CTRL1 slice is 0x01 (override, rate 2700): VCO 8.1 GHz, select 0 -> 450 MHz
    assert_eq!(s.cdclk.cdclk_khz, 450_000);
    // DDI B buffer is on, A is not
    assert!(s.ddis[1].buf.unwrap().enabled && !s.ddis[0].buf.unwrap().enabled);
}

/// Vector E: pipe C -> TRANSCODER_C -> DDI C, HDMI 1080p60 from the WRPLL DPLL2.
fn vector_e() -> Regs {
    let mut r = Regs::firmware_idle();
    r.set(0x46040, 0x8000_0000); // WRPLL_CTL1 = DPLL2 enabled
    r.set(0x6c058, 0x0002_1000); // DPLL2 slice (bits 12..17): override + HDMI mode
    r.set(0x6c060, 0x0001_0000); // DPLL2 locked
    r.set(0x6c048, 0x8040_0173).set(0x6c04c, 0x0000_02c5); // DPLL2 CFGCR1/2
    r.set(0x6c05c, 0x0000_0140); // DDI C: override (bit 6) + select DPLL2 (2 << 7)
    r.set(0x62400, 0xa003_0000); // TRANS_DDI_FUNC_CTL(C): enabled, port C, HDMI, 8bpc, +/+
    r.set(0x72008, 0x8000_0000);
    r.set(0x64200, 0x8000_0000);
    r.set(0x6202c, 0); // TRANS_MULT(C)
    timings(&mut r, 0x62000, HD, HD_SRC);
    r.set(0x72180, 0x8400_0000).set(0x72188, 120).set(0x72190, 0x0437_077f);
    r
}

#[test]
fn vector_e_pipe_c_hdmi_wrpll() {
    let s = run(&vector_e());
    let a = match s.pipes[2].state {
        PipeState::Active(a) => a,
        other => panic!("{other:?}"),
    };
    assert_eq!(a.transcoder, Transcoder::C);
    assert_eq!(a.func.mode, DdiMode::Hdmi);
    assert_eq!(a.func.ddi, Some(Ddi::C));
    assert_eq!(a.func.lanes, None);
    assert_eq!(a.link_mn, None);
    assert_eq!(a.pll, Some(2));
    assert_eq!(a.port_clock_khz, Some(148_500));
    assert_eq!(a.dotclock_khz, Some(148_500));
    assert_eq!(a.refresh_hz(), Some(60));
    assert!(!a.dotclock_deep_colour_unknown);
    assert!(!a.lanes_disagree_with_buf, "no lane count to disagree about for HDMI");
    let d2 = s.dplls[2].unwrap();
    assert!(d2.hdmi_mode && d2.locked && d2.enabled);
    // pipe C has ONE scaler, A and B have two
    assert!(s.pipes[2].scalers[0].is_some() && s.pipes[2].scalers[1].is_none());
    assert!(s.pipes[0].scalers[1].is_some() && s.pipes[1].scalers[1].is_some());
}

#[test]
fn deep_colour_hdmi_is_flagged() {
    let mut r = vector_e();
    r.set(0x62400, 0xa013_0000); // BPC_10
    let a = match run(&r).pipes[2].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!(a.func.bpp, 30);
    assert!(a.dotclock_deep_colour_unknown);
}

#[test]
fn pixel_multiplier_divides_the_dotclock() {
    let mut r = vector_e();
    r.set(0x6202c, 1); // TRANS_MULT(C) = 1 -> multiplier 2
    let a = match run(&r).pipes[2].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!(a.pixel_multiplier, 2);
    assert_eq!(a.dotclock_khz, Some(74_250));
}

#[test]
fn pll_problems_are_named_not_guessed() {
    // the DDI's clock select points at a PLL that is off
    let mut r = vector_a();
    r.set(0x46010, 0x4000_0000); // LCPLL1_CTL: locked but not enabled
    let a = match run(&r).pipes[0].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!(a.pll, Some(0));
    assert_eq!(a.port_clock_khz, None);
    assert_eq!(a.dotclock_khz, None);
    assert_eq!(a.pll_error, Some(ReadoutError::PllNotEnabled { ddi: 'A', pll: 0 }));
    // the select override clear: Linux sees no PLL at all
    let mut r = vector_a();
    r.set(0x6c05c, 0);
    let a = match run(&r).pipes[0].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!((a.pll, a.port_clock_khz, a.pll_error), (None, None, None));
    // a reserved link-rate field
    let mut r = vector_a();
    r.set(0x6c058, 0x0000_000d); // override + field 6
    let a = match run(&r).pipes[0].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!(a.pll_error, Some(ReadoutError::LinkRateReserved { pll: 0, field: 6 }));
}

#[test]
fn lane_disagreement_between_transcoder_and_buffer_is_flagged() {
    let mut r = vector_a();
    r.set(0x64000, 0x8000_0006); // buffer says 4 lanes, the transcoder says 2
    let a = match run(&r).pipes[0].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert!(a.lanes_disagree_with_buf);
}

#[test]
fn register_value_errors_surface_as_pipe_errors() {
    let mut r = vector_a();
    r.set(0x6f400, 0x8203_3002); // eDP input field 3
    assert_eq!(run(&r).pipes[0].state, PipeState::Error(ReadoutError::EdpInputReserved { field: 3 }));
    // reserved mode on the eDP transcoder
    let mut r = vector_a();
    r.set(0x6f400, 0x8503_0002);
    assert_eq!(run(&r).pipes[0].state, PipeState::Error(ReadoutError::DdiModeReserved { trans: "EDP", field: 5 }));
    // a pipe transcoder and the eDP transcoder both feeding pipe A
    let mut r = vector_a();
    r.set(0x60400, 0x9203_0002);
    r.set(0x70008, 0x8000_0000);
    assert_eq!(run(&r).pipes[0].state, PipeState::Error(ReadoutError::MultipleTranscoders { mask: 0x11 }));
    // a reserved plane tiling is that plane's error, not the pipe's
    let mut r = vector_a();
    r.set(0x70180, 0x8400_0800);
    let s = run(&r);
    assert!(matches!(s.pipes[0].state, PipeState::Active(_)));
    assert_eq!(s.pipes[0].planes[0], Some(Err(ReadoutError::TilingReserved { plane: 1, field: 2 })));
}

#[test]
fn interlaced_pipe() {
    let mut r = vector_a();
    r.set(0x7f008, 0x8020_0000); // TRANSCONF interlace field 1
    r.set(0x6f00c, 0x0463_0437).set(0x6f010, 0x0463_0437);
    let a = match run(&r).pipes[0].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert!(a.timings.interlaced && a.transconf.interlaced);
    assert_eq!(a.timings.vtotal, 1125);
}

// ---- power-domain gating: a register in an unpowered well must not be read ----

#[test]
fn pw2_off_pipes_b_and_c_and_transcoder_a_are_not_read() {
    let mut r = vector_a();
    r.set(0x45404, 0x0000_000c); // PW2 request + state cleared in the driver register
    r.forbid(0x61000, 0x64000); // pipe B / C transcoder blocks, TRANSCODER_B/C
    r.forbid(0x60000, 0x6001c); // transcoder A's timing registers (PIPESRC at 0x6001c is pipe A's, always powered)
    r.forbid(0x60020, 0x60404); // the rest of transcoder A's block, incl. TRANS_DDI_FUNC_CTL(A) at 0x60400
    r.forbid(0x71000, 0x73000); // pipe B / C blocks
    r.forbid(0x68800, 0x6a000); // scalers of B and C
    r.forbid(0x64100, 0x64500); // DDI B..E buffers
    let s = run(&r);
    // pipe A is still readable through the always-on eDP transcoder
    assert!(matches!(s.pipes[0].state, PipeState::Active(_)));
    assert_eq!(s.pipes[1].state, PipeState::NotPowered);
    assert_eq!(s.pipes[2].state, PipeState::NotPowered);
    assert!(s.pipes[1].planes.iter().all(|p| p.is_none()));
    assert!(s.pipes[1].scalers.iter().all(|p| p.is_none()));
    for d in 1..5 {
        assert!(s.ddis[d].buf.is_none(), "ddi {d}");
    }
    assert!(s.ddis[0].buf.is_some());
}

#[test]
fn every_register_the_readout_touches_is_in_the_snapshot_universe() {
    // vector_a supplies exactly what a powered KBL readout reads; the strict fake would already have panicked on a
    // stray address. Pin the count so an added or dropped read is a conscious change.
    let r = vector_a();
    let _ = run(&r);
    let reads = r.reads.borrow();
    let mut uniq: Vec<u32> = reads.clone();
    uniq.sort();
    uniq.dedup();
    // power 4 + cdclk 5 + dpll 5 + cfgcr 6 + ddi 5 + (pipe: func ctl 2 + planes 18 + scalers 3*2 per pipe A/B, 3 for C)
    // + pipe A transcoder: transconf 1 + timings 6 + pipesrc 1 + mn 4 ; plus idle pipes' func ctl pairs
    assert!(uniq.len() > 60, "{}", uniq.len());
    assert!(uniq.iter().all(|a| a % 4 == 0), "every register offset is dword aligned");
    assert!(uniq.iter().all(|a| *a < 0x80000), "all within the display register window");
}

#[test]
fn a_pixel_multiplier_register_that_wraps_to_zero_is_treated_as_one() {
    // TRANS_MULT = 0xffffffff -> + 1 wraps to 0; `if (pixel_multiplier) dotclock /= pixel_multiplier` skips the divide
    // (intel_display.c:4103-4104), so the dotclock is the undivided one.
    let mut r = vector_e();
    r.set(0x6202c, 0xffff_ffff);
    let a = match run(&r).pipes[2].state {
        PipeState::Active(a) => a,
        o => panic!("{o:?}"),
    };
    assert_eq!(a.pixel_multiplier, 0);
    assert_eq!(a.dotclock_khz, Some(148_500));
}

/// DPLL1 (LCPLL2) in HDMI mode, 148.5 MHz: its CFGCR registers live at 0x6c040 / 0x6c044.
#[test]
fn dpll1_hdmi_reads_its_own_cfgcr() {
    let mut r = Regs::firmware_idle();
    r.set(0x46014, 0x8000_0000); // LCPLL2_CTL enabled
    r.set(0x6c058, 0x0000_0840); // DPLL1 slice: HDMI mode (bit 11) | override (bit 6)
    r.set(0x6c040, 0x8040_0173).set(0x6c044, 0x0000_02c5);
    let s = run(&r);
    let d1 = s.dplls[1].unwrap();
    assert!(d1.enabled && d1.hdmi_mode);
    assert_eq!((d1.cfgcr1, d1.cfgcr2), (0x8040_0173, 0x0000_02c5));
    assert_eq!(d1.port_clock_khz, 148_500);
}

/// DPLL0 cannot do HDMI ("DPLL 0 doesn't support HDMI mode", intel_dpll_mgr.c:1360): the readout must not read CFGCR
/// registers for it at all (there are none at id - 1 = -1) and must not decode junk as a pixel clock.
#[test]
fn dpll0_never_reads_cfgcr() {
    let mut r = Regs::firmware_idle();
    r.set(0x46010, 0xc000_0000).set(0x6c058, 0x0000_0021); // DPLL0: override + HDMI mode
    let s = run(&r);
    let d0 = s.dplls[0].unwrap();
    assert!(d0.hdmi_mode);
    assert_eq!((d0.cfgcr1, d0.cfgcr2, d0.port_clock_khz), (0x0, 0x0, 0));
    for a in [0x6c038u32, 0x6c03c] {
        assert!(!r.was_read(a), "cfgcr of a nonexistent DPLL index read at {a:#x}");
    }
}

#[test]
fn a_part_without_the_edp_transcoder_never_reads_it() {
    use i915_kbl_display_core::DisplayCaps;
    let caps = DisplayCaps { transcoder_mask: 0b0_0111, ..KBL_DISPLAY };
    let mut r = vector_d();
    for a in [0x6f400u32] {
        r.remove(a); // a strict register file: touching the eDP transcoder panics
    }
    let mut rd = |a: u32| r.read(a);
    let s = read_display_state(&mut rd, &caps);
    assert!(matches!(s.pipes[1].state, PipeState::Active(_)));
    assert!(!r.was_read(0x6f400));
}
