// SPDX-License-Identifier: GPL-2.0-only
//! Pipe / transcoder write values against GCC's evaluation of Linux's own macros (intel_display_regs.h, combined exactly as
//! `intel_set_transcoder_timings`, `intel_set_pipe_src_size`, `bdw_set_pipe_misc`, `intel_ddi_transcoder_func_reg_val_get`,
//! `hsw_set_frame_start_delay`, `hsw_set_linetime_wm` combine them), and the M/N pairs GCC computes by compiling Linux's
//! `intel_link_compute_m_n()` / `compute_m_n()` / `drm_dp_max_dprx_data_rate()` verbatim. Expected values are literal.
use i915_kbl_display_core::mode_write::*;
use i915_kbl_display_core::timing::ModeTimings;
use i915_kbl_display_core::types::{Ddi, Pipe, Transcoder};

fn cea_1080p60() -> ModeTimings {
    ModeTimings {
        hdisplay: 1920,
        htotal: 2200,
        hblank_start: 1920,
        hblank_end: 2200,
        hsync_start: 2008,
        hsync_end: 2052,
        vdisplay: 1080,
        vtotal: 1125,
        vblank_start: 1080,
        vblank_end: 1125,
        vsync_start: 1084,
        vsync_end: 1089,
        interlaced: false,
    }
}

#[test]
fn timings_in_linux_order_with_linux_values() {
    let w = timing_writes(Transcoder::Edp, &cea_1080p60()).unwrap();
    assert_eq!(
        w.as_slice(),
        &[
            (0x6f028, 0),            // TRANS_VSYNCSHIFT(EDP)
            (0x6f000, 0x0897_077f),  // HACTIVE(1919) | HTOTAL(2199)
            (0x6f004, 0x0897_077f),  // HBLANK_START(1919) | HBLANK_END(2199)
            (0x6f008, 0x0803_07d7),  // HSYNC_START(2007) | HSYNC_END(2051)
            (0x6f00c, 0x0464_0437),  // VACTIVE(1079) | VTOTAL(1124)
            (0x6f010, 0x0464_0437),  // VBLANK_START(1079) | VBLANK_END(1124)
            (0x6f014, 0x0440_043b),  // VSYNC_START(1083) | VSYNC_END(1088)
        ]
    );
    // transcoder A has the same offsets without the EDP block base
    let a = timing_writes(Transcoder::A, &cea_1080p60()).unwrap();
    assert_eq!(a.as_slice()[1], (0x60000, 0x0897_077f));
    let mut i = cea_1080p60();
    i.interlaced = true;
    assert_eq!(timing_writes(Transcoder::Edp, &i).unwrap_err().name(), "MODE-INTERLACED-NOT-PORTED");
    let mut big = cea_1080p60();
    big.htotal = 0x1_0002;
    assert_eq!(timing_writes(Transcoder::Edp, &big).unwrap_err().name(), "MODE-FIELD-OVERFLOW");
}

#[test]
fn pipe_registers() {
    assert_eq!(pipesrc_value(1920, 1080).unwrap(), 0x077f_0437);
    assert_eq!(pipe_misc_value(24).unwrap(), 0x00);
    assert_eq!(pipe_misc_value(30).unwrap(), 0x20);
    assert_eq!(pipe_misc_value(18).unwrap(), 0x40);
    assert_eq!(pipe_misc_value(36).unwrap_err().name(), "MODE-UNSUPPORTED-BPP");
    assert_eq!(transconf_modeset_value(), 0, "TRANSCONF_INTERLACE_PF_PD_ILK, not enabled during a modeset");
    assert_eq!(transconf_enable_value(0), 0x8000_0000);
    assert_eq!(frame_start_delay_rmw(1), (0x1800_0000, 0), "HSW_FRAME_START_DELAY(framestart_delay - 1)");
    assert_eq!(frame_start_delay_rmw(2), (0x1800_0000, 0x0800_0000));
}

#[test]
fn ddi_func_ctl() {
    let both = SyncFlags { phsync: true, pvsync: true };
    assert_eq!(ddi_func_ctl_value(Ddi::A, Transcoder::Edp, Pipe::A, 24, both, 4).unwrap(), 0x8203_0006);
    assert_eq!(ddi_func_ctl_value(Ddi::A, Transcoder::Edp, Pipe::A, 18, SyncFlags { phsync: false, pvsync: false }, 2).unwrap(), 0x8220_0002);
    assert_eq!(ddi_func_ctl_value(Ddi::A, Transcoder::Edp, Pipe::A, 30, SyncFlags { phsync: true, pvsync: false }, 1).unwrap(), 0x8211_0000);
    assert_eq!(ddi_func_ctl_value(Ddi::B, Transcoder::Edp, Pipe::A, 24, both, 4).unwrap_err().name(), "MODE-NOT-EDP-ON-DDI-A");
    assert_eq!(ddi_func_ctl_value(Ddi::A, Transcoder::A, Pipe::A, 24, both, 4).unwrap_err().name(), "MODE-NOT-EDP-ON-DDI-A");
    assert_eq!(ddi_func_ctl_value(Ddi::A, Transcoder::Edp, Pipe::A, 24, both, 3).unwrap_err().name(), "MODE-BAD-LANE-COUNT");
}

#[test]
fn link_m_n_matches_linux_compute() {
    // (bpp, lanes, pixel clock kHz, link clock in 10 kbit/s) -> (tu, data_m, data_n, link_m, link_n), from the gcc build of intel_link_compute_m_n().
    let cases: &[((u32, u32, u32, u32), (u32, u32, u32, u32, u32))] = &[
        ((24, 4, 148500, 270000), (64, 3460300, 8388608, 288358, 524288)),
        ((24, 2, 148500, 270000), (64, 6920601, 8388608, 288358, 524288)),
        ((24, 2, 148500, 162000), (64, 11534336, 8388608, 480597, 524288)),
        ((24, 4, 594000, 540000), (64, 6920601, 8388608, 576716, 524288)),
        ((18, 4, 141000, 270000), (64, 2464153, 8388608, 273794, 524288)),
        ((24, 4, 533250, 270000), (64, 12425625, 8388608, 1035468, 524288)),
        ((30, 4, 297000, 270000), (64, 8650752, 8388608, 576716, 524288)),
        ((24, 1, 25175, 162000), (64, 3910800, 8388608, 81475, 524288)),
    ];
    for ((bpp, lanes, pclk, lclk), (tu, dm, dn, lm, ln)) in cases {
        let mn = compute_link_mn(*pclk, *lclk, *lanes, *bpp).unwrap();
        assert_eq!((mn.tu, mn.data_m, mn.data_n, mn.link_m, mn.link_n), (*tu, *dm, *dn, *lm, *ln), "{bpp}bpp {lanes}L {pclk}kHz {lclk}");
    }
}

#[test]
fn mn_write_order_and_tu() {
    let mn = compute_link_mn(148500, 270000, 4, 24).unwrap();
    let w = mn_writes(Transcoder::Edp, &mn);
    let regs: Vec<u32> = w.as_slice().iter().map(|x| x.0).collect();
    // PIPE_DATA_M1, PIPE_DATA_N1, PIPE_LINK_M1, PIPE_LINK_N1 - LINK_N last ("arms the double buffered update").
    assert_eq!(regs, [0x6f030, 0x6f034, 0x6f040, 0x6f044]);
    assert_eq!(w.as_slice()[0].1, 0x7e34_cccc, "TU_SIZE(64) | data_m");
    assert_eq!(w.as_slice()[3].1, 524288);
}

#[test]
fn linetime() {
    assert_eq!(linetime_wm(2200, 148500).unwrap(), 119); // DIV_ROUND_UP(2200 * 1000 * 8, 148500)
    assert_eq!(linetime_wm(4400, 594000).unwrap(), 60);
    assert_eq!(linetime_wm(1344, 65000).unwrap(), 166);
    assert_eq!(linetime_wm(100_000, 100).unwrap(), 0x1ff, "clamped to 0x1ff");
    assert_eq!(linetime_reg_value(15), 0xf);
    assert_eq!(linetime_reg_value(0x1ff), 0x1ff);
    assert_eq!(linetime_wm(2200, 0).unwrap_err().name(), "MODE-ZERO-DIVISOR");
}
