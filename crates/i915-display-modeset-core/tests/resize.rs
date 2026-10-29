// SPDX-License-Identifier: GPL-2.0-only
//! The adopted pipe's logical scanout size: the register plan Linux's fastset would write for 2560x1440 and 2880x1620 on the Pixelbook
//! Go's 3840x2160 eDP panel (values worked by hand from `intel_set_pipe_src_size`, `skl_pfit_enable`, `skl_scaler_calc_phase`,
//! `drm_calc_scale`, `skl_plane_update_noarm`), the restore plan, every refusal by name, the watermark bound against the Rust port of
//! `skl_compute_wm_levels` (itself checked against GCC-compiled Linux in i915-kbl-display-core), and the vblank-evasion judge.
use i915_display_modeset_core::adopt::ADOPT_WRITE_ALLOWLIST;
use i915_display_modeset_core::initial::{adopt, AdoptedDisplay};
use i915_display_modeset_core::resize::*;
use i915_kbl_display_core::readout::{read_display_state, ReadoutState};
use i915_kbl_display_core::skl_pfit::{calc_phase, calc_scale, pfit_enable_writes, scaler_limits, PfitError, Window};
use i915_kbl_display_core::skl_wm::{compute_wm_levels, compute_wm_params, plane_wm_reg_val, Latencies, PlaneWmGeometry, WmTiming};
use i915_kbl_display_core::types::Pipe;
use i915_kbl_display_core::KBL_DISPLAY;
use std::collections::BTreeMap;

/// The Pixelbook Go as its firmware left it (tests/initial.rs `atlas()`): eDP 3840x2160 (4000x2222 total) on transcoder EDP, pipe source
/// 1920x1080, PLANE_1 linear XRGB8888 stride 7680 at GGTT 0, pipe A scaler 1 (id 0) bound to the pipe over the whole panel, HQ mode;
/// PLANE_1's DDB 0..860 and the watermark levels Linux computes for a 1920-wide linear plane at this raster ([`linux_levels`]).
fn atlas() -> BTreeMap<u32, u32> {
    let mut r: BTreeMap<u32, u32> = [
        (0x45400u32, 0x3000_0003u32),
        (0x45404, 0xc000_000c),
        (0x42000, 0x8e00_0000),
        (0x46010, 0xc000_0000),
        (0x6c058, 3),
        (0x6c05c, 1),
        (0x6c060, 1),
        (0x46000, 0x382),
        (0x6f400, 0x8203_0002 | 4 << 12), // TRANS_DDI_FUNC_CTL: eDP input A_ONOFF (the fitter is in the path)
        (0x7f008, 0x8000_0000),
        (0x64000, 0x8000_0002),
        (0x6f000, 0x0f9f_0eff),
        (0x6f004, 0x0f9f_0eff),
        (0x6f008, 0x0f4f_0f2f),
        (0x6f00c, 0x08ad_086f),
        (0x6f010, 0x08ad_086f),
        (0x6f014, 0x0877_0872),
        (0x6001c, 0x077f_0437),
        (0x6f030, 0x7e69_9999),
        (0x6f034, 0x0080_0000),
        (0x6f040, 288_358),
        (0x6f044, 0x0008_0000),
        (0x70180, 0x8400_2000),
        (0x70188, 120),
        (0x70190, 0x0437_077f),
        (0x7019c, 0),
        (0x68180, 0x9000_0000), // PS_1A_CTRL: enabled, bound to the pipe, HQ
        (0x68188, 0x3000),
        (0x68194, 0x3000),
        (0x68170, 0),
        (0x68174, 0x0f00_0870),
        (0x7027c, 859 << 16), // PLANE_BUF_CFG: 0..860
    ]
    .into_iter()
    .collect();
    for (level, l) in linux_levels(1920, false).iter().enumerate() {
        r.insert(0x70240 + 4 * level as u32, plane_wm_reg_val(l));
    }
    r
}

fn state(regs: &BTreeMap<u32, u32>) -> ReadoutState {
    let mut rd = |a: u32| regs.get(&a).copied().unwrap_or(0);
    read_display_state(&mut rd, &KBL_DISPLAY)
}

fn setup(regs: &BTreeMap<u32, u32>) -> (AdoptedDisplay, FirmwareGeometry) {
    let st = state(regs);
    let a = adopt(&st, 4096).expect("the Atlas firmware scanout is adopted");
    let fw = read_firmware_geometry(|r| regs.get(&r).copied().unwrap_or(0), &KBL_DISPLAY, &st, &a).expect("active pipe");
    (a, fw)
}

/// The 4K60 eDP raster's watermark timing (the dotclock the M/N above gives is not needed: the bound is checked at this one).
fn timing_4k() -> WmTiming {
    WmTiming { htotal: 4000, clock_khz: 533_250, pixel_rate_khz: 533_250, vblank_lines: 62 }
}

/// The adjusted latencies of the Linux-evaluated case "s1" (skl_wm_linux.rs): [2, 4, 10, 18, 30, 40, 50, 60] us.
const LAT: Latencies = [2, 4, 10, 18, 30, 40, 50, 60];

fn linux_levels(width: u32, x_tiled: bool) -> [i915_kbl_display_core::skl_wm::WmLevel; 8] {
    linux_levels_at(&timing_4k(), false, width, x_tiled)
}

fn linux_levels_at(t: &WmTiming, ipc: bool, width: u32, x_tiled: bool) -> [i915_kbl_display_core::skl_wm::WmLevel; 8] {
    let g = PlaneWmGeometry { width, cpp: 4, x_tiled, y_tiled: false, is_planar: false, color_plane: 0 };
    let wp = compute_wm_params(t, &g).unwrap();
    compute_wm_levels(&LAT, ipc, &wp, t).unwrap()
}

fn write_of(p: &ScanoutResize, reg: u32) -> u32 {
    p.forward.iter().find(|(r, _)| *r == reg).map(|(_, v)| *v).unwrap_or_else(|| panic!("no write to {reg:#x}"))
}

#[test]
fn the_policy_picks_panel_over_one_and_a_half_by_default_and_three_quarters_on_request() {
    assert_eq!(LogicalSize::default().resolve((3840, 2160), (1920, 1080)), (2560, 1440));
    assert_eq!(LogicalSize::Fraction { num: 3, den: 4 }.resolve((3840, 2160), (1920, 1080)), (2880, 1620));
    assert_eq!(LogicalSize::Adopted.resolve((3840, 2160), (1920, 1080)), (1920, 1080));
    assert_eq!(LogicalSize::Exact { width: 2560, height: 1600 }.resolve((3840, 2160), (1920, 1080)), (2560, 1600));
    assert_eq!(LogicalSize::default().resolve((2736, 1824), (1368, 912)), (1824, 1216), "rounded down to even");
    assert_eq!(LogicalSize::default().resolve((1920, 1080), (1920, 1080)), (1920, 1080), "a 1080p panel is not shrunk to 720p");
}

#[test]
fn the_policy_parses_from_config_text() {
    assert_eq!(LogicalSize::parse("adopted"), Some(LogicalSize::Adopted));
    assert_eq!(LogicalSize::parse("2/3"), Some(LogicalSize::Fraction { num: 2, den: 3 }));
    assert_eq!(LogicalSize::parse(" 3/4 "), Some(LogicalSize::Fraction { num: 3, den: 4 }));
    assert_eq!(LogicalSize::parse("2880x1620"), Some(LogicalSize::Exact { width: 2880, height: 1620 }));
    for bad in ["", "4/3", "0/3", "2/0", "x1440", "2560x", "big"] {
        assert_eq!(LogicalSize::parse(bad), None, "{bad:?}");
    }
}

#[test]
fn linux_scale_and_phase_formulas() {
    // drm_calc_scale: (src << 16) / dst rounded down when upscaling
    assert_eq!(calc_scale(1920, 3840), 0x8000);
    assert_eq!(calc_scale(2560, 3840), 43690);
    assert_eq!(calc_scale(2880, 3840), 0xc000);
    assert_eq!(calc_scale(4096, 1000), 268_436, "downscale rounds up");
    // skl_scaler_calc_phase(1, scale, false): -0.5 + scale / 2, wrapped, >> 2, masked to u2.13
    assert_eq!(calc_phase(1, 0x8000, false), 0x3000, "the firmware's 2x");
    assert_eq!(calc_phase(1, 43690, false), 0x3554);
    assert_eq!(calc_phase(1, 0xc000, false), 0x3800);
    assert_eq!(calc_phase(1, 0x10000, false), 0x0001, "1:1 is phase 0 with the trip bit");
}

#[test]
fn plan_2560x1440_on_the_4k_panel() {
    let (a, fw) = setup(&atlas());
    assert!(fw.fitter_on);
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!((p.logical_w, p.logical_h, p.panel_w, p.panel_h, p.scaler_id), (2560, 1440, 3840, 2160, 0));
    assert_eq!((p.stride, p.fb_size), (10240, 10240 * 1440));
    assert_eq!((p.hscale, p.vscale, p.ratio_milli()), (43690, 43690, 1500));
    assert_eq!(p.window, Window { x: 0, y: 0, w: 3840, h: 2160 }, "16:9 onto 16:9: the whole panel");
    let order: Vec<u32> = p.forward.iter().map(|(r, _)| *r).collect();
    assert_eq!(
        &order[..11],
        &[0x6001c, 0x68180, 0x68188, 0x68194, 0x68170, 0x68174, 0x45270, 0x70188, 0x7018c, 0x70190, 0x70240],
        "intel_pipe_fastset (PIPESRC, skl_pfit_enable, hsw_set_linetime_wm) then skl_plane_update_noarm (STRIDE, POS, SIZE, WM, DDB)"
    );
    assert_eq!(write_of(&p, 0x6001c), (2559 << 16) | 1439, "PIPESRC_WIDTH(w - 1) | PIPESRC_HEIGHT(h - 1)");
    assert_eq!(write_of(&p, 0x68180), 0x9000_0000, "PS_SCALER_EN | PS_BINDING_PIPE | HQ (kept) | PS_FILTER_MEDIUM");
    assert_eq!(write_of(&p, 0x68188), 0x3554, "PS_Y_PHASE(0) | PS_UV_RGB_PHASE(vphase)");
    assert_eq!(write_of(&p, 0x68194), 0x3554);
    assert_eq!(write_of(&p, 0x68170), 0);
    assert_eq!(write_of(&p, 0x68174), (3840 << 16) | 2160);
    assert_eq!(write_of(&p, 0x70188), 10240 / 64, "linear stride in 64-byte units");
    assert_eq!(write_of(&p, 0x7018c), 0);
    assert_eq!(write_of(&p, 0x70190), (1439 << 16) | 2559);
    assert_eq!(write_of(&p, 0x70268), 0, "display ver 9: no transition watermark (WaDisableTWM)");
    let plan = p.forward_plan(0x80_0000);
    assert_eq!(plan.len(), p.forward.len() + 2, "PLANE_SURF and its posting read close the burst");
    let allowed: Vec<u32> = RESIZE_WRITE_ALLOWLIST.iter().chain(ADOPT_WRITE_ALLOWLIST.iter()).map(|r| r.offset()).collect();
    for r in p.written() {
        assert!(allowed.contains(&r.offset()), "{:#x} is not on the resize allow-list", r.offset());
    }
}

#[test]
fn plan_2880x1620_on_the_4k_panel() {
    let (a, fw) = setup(&atlas());
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2880, 1620).unwrap().unwrap();
    assert_eq!((p.stride, p.hscale, p.vscale, p.ratio_milli()), (11520, 0xc000, 0xc000, 1333));
    assert_eq!(write_of(&p, 0x6001c), (2879 << 16) | 1619);
    assert_eq!(write_of(&p, 0x68188), 0x3800);
    assert_eq!(write_of(&p, 0x68194), 0x3800);
    assert_eq!(write_of(&p, 0x68174), (3840 << 16) | 2160);
    assert_eq!(write_of(&p, 0x70188), 11520 / 64);
    assert_eq!(write_of(&p, 0x70190), (1619 << 16) | 2879);
}

#[test]
fn the_restore_puts_the_firmware_geometry_back_register_for_register() {
    let regs = atlas();
    let (a, fw) = setup(&regs);
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!(p.back.len(), p.forward.len());
    for ((rf, _), (rb, vb)) in p.forward.iter().zip(&p.back) {
        assert_eq!(rf, rb, "same registers, same order");
        assert_eq!(*vb, regs.get(rb).copied().unwrap_or(0), "{rb:#x} goes back to what the firmware left");
    }
    let rp = p.restore_plan(0);
    assert_eq!(rp.len(), p.back.len() + 2);
}

#[test]
fn the_adopted_size_needs_no_plan() {
    let (a, fw) = setup(&atlas());
    assert_eq!(plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 1920, 1080), Ok(None));
}

#[test]
fn every_refusal_is_named() {
    let (a, fw) = setup(&atlas());
    let refuse = |a: &AdoptedDisplay, fw: &FirmwareGeometry, w, h| plan_scanout_resize(&KBL_DISPLAY, a, fw, w, h).unwrap_err().name();
    assert_eq!(refuse(&a, &fw, 0, 1440), "RESIZE-SIZE-INVALID");
    assert_eq!(refuse(&a, &fw, 4096, 2304), "RESIZE-DOWNSCALE-NOT-FASTSET");
    // Linux's scaler minimum (skl_scaler_min_src_size: 8x8)
    assert_eq!(refuse(&a, &fw, 4, 4), "PFIT-SCALER-OUT-OF-RANGE");
    // no fitter on an eDP transcoder: the A_ON -> A_ONOFF mux change is a full modeset
    let mut no_fit = a;
    no_fit.fitter = None;
    let fw_off = FirmwareGeometry { fitter_on: false, ..fw };
    assert_eq!(refuse(&no_fit, &fw_off, 2560, 1440), "RESIZE-EDP-MUX-NEEDS-MODESET");
    // no scaler on the pipe at all
    assert_eq!(refuse(&no_fit, &FirmwareGeometry { scalers_on_pipe: 0, ..fw_off }, 2560, 1440), "RESIZE-NO-SCALER-FREE");
    // only pipe A / plane 1 is on the allow-list
    let mut on_b = a;
    on_b.pipe = Pipe::B;
    assert_eq!(refuse(&on_b, &fw, 2560, 1440), "RESIZE-PLANE-NOT-IN-ALLOW-LIST");
    // no DDB, and a level 0 that no longer fits it
    assert_eq!(refuse(&a, &FirmwareGeometry { buf_cfg: 0, other_pipes_active: true, ..fw }, 2560, 1440), "RESIZE-PLANE-NO-DDB");
    // a 4-block DDB the plan may not regrow (another pipe holds DDB: the weighted split is not ported) and a level 0 that no longer fits it
    let tiny = FirmwareGeometry { buf_cfg: 3 << 16, other_pipes_active: true, ..fw };
    assert_eq!(refuse(&a, &tiny, 2560, 1440), "ddb-too-small");
    // need = the 2560-wide level 0's min_ddb_alloc, have = the firmware's PLANE_BUF_CFG size (skl_crtc_allocate_plane_ddb's -EINVAL)
    let e = plan_scanout_resize(&KBL_DISPLAY, &a, &tiny, 2560, 1440).unwrap_err();
    let ResizeRefusal::DdbTooSmall { need, have } = e else { panic!("{e:?}") };
    assert_eq!(have, 4);
    let l0 = rescale_wm(&fw.wm, 1920, 2560)[0];
    assert_eq!(need, l0.min_ddb_alloc());
    assert!(need > have);
}

#[test]
fn the_planned_resize_carries_the_ddb_it_was_checked_against_and_its_health_delay() {
    let (a, fw) = setup(&atlas());
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!(p.ddb_blocks, 860, "892 blocks less the 32-block cursor reserve: what the firmware left too");
    assert_eq!(p.ddb_how, "linux-single-plane");
    assert_eq!(write_of(&p, 0x7027c), 859 << 16, "PLANE_BUF_CFG 0..860 (end - 1 in the high half)");
    assert_eq!(write_of(&p, 0x70278), 0, "PLANE_NV12_BUF_CFG: no Y plane (skl_write_plane_wm, display ver < 11)");
    assert_eq!(p.wm0_need, rescale_wm(&fw.wm, 1920, 2560)[0].min_ddb_alloc());
    assert!(p.wm0_need <= p.ddb_blocks);
    // 4000 x 2222 at the readout's dotclock: three frames, rounded up, + 1 ms
    let khz = fw.dotclock_khz.expect("the Atlas raster has a dotclock") as u64;
    assert_eq!(p.health_after_ms(), (3 * 4000 * 2222u64).div_ceil(khz) + 1);
}

#[test]
fn a_non_edp_pipe_without_a_fitter_gets_scaler_zero_in_hq_mode() {
    let (mut a, fw) = setup(&atlas());
    a.fitter = None;
    a.transcoder = i915_kbl_display_core::types::Transcoder::A;
    let fw = FirmwareGeometry { fitter_on: false, transcoder: a.transcoder, ..fw };
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!(p.scaler_id, 0);
    assert_eq!(write_of(&p, 0x68180), 0x9000_0000, "intel_atomic_setup_scaler: one user on a two-scaler pipe -> scaler 0, HQ");
    assert_eq!(p.back[1], (0x68180, 0), "skl_detach_scaler on the way back");
}

#[test]
fn scaler_limits_follow_the_display_version() {
    let l9 = scaler_limits(9).unwrap();
    assert_eq!((l9.max_src, l9.max_dst, l9.max_hscale), ((4096, 4096), (4096, 4096), 0x2ffff));
    assert_eq!(scaler_limits(11).unwrap().max_src, (5120, 4096));
    assert_eq!(scaler_limits(12).unwrap().max_dst, (8192, 8192));
    assert_eq!(scaler_limits(8).unwrap_err().name(), "PFIT-UNKNOWN-DISPLAY-VERSION");
    let full = Window { x: 0, y: 0, w: 1000, h: 1000 };
    // more than 3x down is out of calculate_max_scale's range
    assert!(matches!(pfit_enable_writes(9, Pipe::A, 0, 0, 4096, 4096, full), Err(PfitError::OutOfRange { .. }) | Err(PfitError::ScaleOutOfRange { .. })));
    assert!(matches!(pfit_enable_writes(9, Pipe::A, 0, 0, 3500, 3500, full), Err(PfitError::ScaleOutOfRange { .. })));
    // the pipe source may not exceed the destination limit
    assert!(matches!(pfit_enable_writes(9, Pipe::A, 0, 0, 4100, 2000, Window { x: 0, y: 0, w: 4096, h: 2160 }), Err(PfitError::OutOfRange { .. })));
}

#[test]
fn a_different_aspect_is_pillarboxed_and_centred_as_linux_does() {
    let (a, fw) = setup(&atlas());
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1600).unwrap().unwrap();
    // 16:10 onto 16:9: width = 2560 * 2160 / 1600 = 3456, x = (3840 - 3456 + 1) / 2 = 192
    assert_eq!(p.window, Window { x: 192, y: 0, w: 3456, h: 2160 });
    assert_eq!(write_of(&p, 0x68170), 192 << 16);
}

#[test]
fn the_watermark_bound_is_never_below_linux_for_the_wider_plane() {
    let cea_1080p = WmTiming { htotal: 2200, clock_khz: 148_500, pixel_rate_khz: 148_500, vblank_lines: 45 };
    for t in [timing_4k(), cea_1080p] {
        for ipc in [false, true] {
            for x_tiled in [false, true] {
                let old = linux_levels_at(&t, ipc, 1920, x_tiled);
                let fw: [u32; 8] = core::array::from_fn(|l| plane_wm_reg_val(&old[l]));
                for new_w in [2048u32, 2560, 2880, 3840] {
                    let bound = rescale_wm(&fw, 1920, new_w);
                    let linux = linux_levels_at(&t, ipc, new_w, x_tiled);
                    for level in 0..8 {
                        let (b, l) = (bound[level], linux[level]);
                        let what = format!("htotal={} ipc={ipc} x={x_tiled} w={new_w} level {level}", t.htotal);
                        if !l.enable {
                            continue;
                        }
                        assert!(b.enable, "{what}: Linux enables it");
                        assert!(b.blocks >= l.blocks as u32, "{what}: bound {} blocks < Linux {}", b.blocks, l.blocks);
                        assert!(b.lines >= l.lines as u32, "{what}: bound {} lines < Linux {}", b.lines, l.lines);
                    }
                }
            }
        }
    }
}

#[test]
fn levels_that_no_longer_fit_the_ddb_are_disabled_from_the_first_misfit_up() {
    let (a, fw) = setup(&atlas());
    let levels = rescale_wm(&fw.wm, 1920, 2560);
    // a DDB that holds level 2 but not level 3, kept (another pipe is active: the plan may not regrow it)
    let ddb = levels[2].min_ddb_alloc();
    assert!(levels[3].min_ddb_alloc() > ddb);
    let small = FirmwareGeometry { buf_cfg: (ddb - 1) << 16, other_pipes_active: true, ..fw };
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &small, 2560, 1440).unwrap().unwrap();
    assert_eq!(p.wm_levels, 3);
    for level in 3..8u32 {
        assert_eq!(write_of(&p, 0x70240 + 4 * level), 0, "level {level}: skl_check_wm_level's memset");
    }
    assert_ne!(write_of(&p, 0x70240 + 8), 0);
}

#[test]
fn vblank_evasion_window_follows_intel_vblank_evade() {
    // 3840x2160 eDP, vblank_start 2160, vtotal 2222, htotal 4000, 533.25 MHz, 1000 us: ceil(1000 * 533250 / 4_000_000) = 134 lines
    let e = VblankEvade::new(2160, 2222, 4000, Some(533_250), scanline_offset(9, false), EVADE_US).unwrap();
    assert_eq!((e.min, e.max), (2026, 2159));
    assert!(e.clear(2024), "scanline 2025: before the window");
    assert!(!e.clear(2025), "scanline 2026: inside");
    assert!(!e.clear(2158), "scanline 2159: the last line before vblank");
    assert!(e.clear(2159), "scanline 2160: in vblank - the writes latch at the NEXT vblank start, together");
    assert!(e.clear(2221), "wraps to line 0");
    assert_eq!(scanline_offset(9, true), 2, "HDMI on HSW+ counts one more");
    assert_eq!(VblankEvade::new(2160, 2222, 4000, None, 1, EVADE_US), None, "no dotclock: nothing to evade by");
    let (_, fw) = setup(&atlas());
    assert_eq!((fw.vblank_start, fw.vtotal, fw.htotal), (2160, 2222, 4000));
}

#[test]
fn the_plane_ddb_is_recomputed_as_linux_allocates_a_single_plane_on_a_single_pipe() {
    // the firmware gave the plane 0..500 and the cursor the top 32 blocks: Linux's allocation is the pipe's whole DDB (one active pipe,
    // intel_crtc_ddb_weight), the cursor's fixed reserve at the top (skl_cursor_allocation), every other block to the one plane with a data rate
    let mut regs = atlas();
    regs.insert(0x7027c, 499 << 16);
    regs.insert(0x7017c, (891 << 16) | 860);
    let (a, fw) = setup(&regs);
    assert_eq!((fw.buf_cfg, fw.cur_buf_cfg, fw.other_pipes_active), (499 << 16, (891 << 16) | 860, false));
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!((p.ddb_blocks, p.ddb_how), (860, "linux-single-plane"));
    assert_eq!(write_of(&p, 0x7027c), 859 << 16);
    let back = |reg: u32| p.back.iter().find(|(r, _)| *r == reg).map(|(_, v)| *v).unwrap();
    assert_eq!(back(0x7027c), 499 << 16, "the restore puts the firmware's DDB back");
    // PLANE_BUF_CFG after the watermark words (skl_write_plane_wm), before PLANE_SURF (forward_plan appends it last)
    let at = |reg: u32| p.forward.iter().position(|(r, _)| *r == reg).unwrap();
    assert!(at(0x70268) < at(0x7027c) && at(0x7027c) < at(0x70278));
    // a VISIBLE cursor whose reserve is not at the top: its size is skl_cursor_allocation's, from latencies this port does not read, so
    // neither entry moves - the firmware's plane DDB is kept, named
    regs.insert(0x7017c, (31 << 16) | 0);
    regs.insert(0x7027c, (891 << 16) | 32);
    regs.insert(0x70080, 0x27); // CURCNTR: MCURSOR_MODE_MASK set
    let (a, fw) = setup(&regs);
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!((p.ddb_blocks, p.ddb_how), (860, "kept-cursor-visible-not-at-top"));
}

/// The Pixelbook Go as silicon showed it (stick "amber mountain marsh", 2026-10-29): the firmware gave PLANE_1 152 blocks
/// (`REFUSED why=ddb-too-small need=228 have=152`), pipes B and C not powered, the cursor hidden. Linux re-places a hidden cursor's
/// reserve at the top of the pipe's DDB (skl_crtc_allocate_plane_ddb :1452-1458) and updates it with the commit
/// (skl_ddb_add_affected_planes :2426-2459 -> i9xx_cursor_update_arm -> skl_write_cursor_wm), wherever the firmware left it.
fn atlas_silicon_ddb(cursor_buf_cfg: u32) -> BTreeMap<u32, u32> {
    let mut regs = atlas();
    regs.insert(0x7027c, 151 << 16); // PLANE_BUF_CFG 0..152
    regs.insert(0x7017c, cursor_buf_cfg);
    regs
}

#[test]
fn the_silicon_152_block_plane_gets_linux_s_860_wherever_the_firmware_left_the_hidden_cursor() {
    for (cursor, what) in [
        ((895 << 16) | 864, "the top of a 896-block DBUF (the firmware did not keep the 4 bypass blocks)"),
        ((183 << 16) | 152, "right above the plane"),
        (0, "no reserve"),
    ] {
        let (a, fw) = setup(&atlas_silicon_ddb(cursor));
        assert!(!fw.other_pipes_active);
        let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap_or_else(|e| panic!("{what}: {e:?}")).unwrap();
        assert_eq!((p.ddb_blocks, p.ddb_how), (860, "linux-single-plane"), "{what}");
        assert_eq!((p.ddb.dbuf, p.ddb.plane, p.ddb.cursor, p.ddb.fw_plane, p.ddb.fw_cursor), (892, (0, 860), (860, 892), (0, 152), ddb_entry(cursor)), "{what}");
        assert_eq!(write_of(&p, 0x7027c), 859 << 16);
        // the hidden cursor's block, as i9xx_cursor_update_arm(NULL) writes it: zero watermarks, the new reserve, CURPOS, CURBASE (arms)
        assert_eq!(write_of(&p, 0x7017c), (891 << 16) | 860, "{what}");
        assert_eq!(write_of(&p, 0x70140), 0);
        assert_eq!(write_of(&p, 0x70168), 0);
        let at = |reg: u32| p.forward.iter().position(|(r, _)| *r == reg).unwrap();
        assert!(at(0x7027c) < at(0x7017c) && at(0x7017c) < at(0x70088) && at(0x70088) < at(0x70084), "CUR_BUF_CFG, CURPOS, CURBASE last");
        let back = |reg: u32| p.back.iter().find(|(r, _)| *r == reg).map(|(_, v)| *v).unwrap();
        assert_eq!((back(0x7027c), back(0x7017c)), (151 << 16, cursor), "the restore puts both firmware entries back");
        let line = format!("{}", p.ddb);
        assert!(line.starts_with("ddb_how=linux-single-plane dbuf=892 plane=0..860 cursor=860..892 fw_plane=0..152 fw_cursor="), "{line}");
        assert!(line.ends_with(" other_pipes=0 cursor_visible=0"), "{line}");
    }
    // a cursor reserve already at the top and at least Linux's 32-block floor is left where it is: no cursor write at all
    let (a, fw) = setup(&atlas_silicon_ddb((891 << 16) | 860));
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    assert_eq!(p.ddb_blocks, 860);
    assert!(p.forward.iter().all(|(r, _)| *r != 0x7017c && *r != 0x70084));
}

#[test]
fn every_kept_ddb_names_its_inputs() {
    let (_, fw) = setup(&atlas_silicon_ddb((183 << 16) | 152));
    let c = plane_ddb(&KBL_DISPLAY, &FirmwareGeometry { other_pipes_active: true, ..fw });
    assert_eq!((c.how, c.plane), ("kept-other-pipes-active", (0, 152)));
    assert_eq!(format!("{c}"), "ddb_how=kept-other-pipes-active dbuf=892 plane=0..152 cursor=152..184 fw_plane=0..152 fw_cursor=152..184 other_pipes=1 cursor_visible=0");
}

#[test]
fn the_fastset_rewrites_the_line_time_watermark_as_hsw_set_linetime_wm_does() {
    let (a, fw) = setup(&atlas());
    let p = plan_scanout_resize(&KBL_DISPLAY, &a, &fw, 2560, 1440).unwrap().unwrap();
    // skl_linetime_wm: DIV_ROUND_UP(htotal * 1000 * 8, pixel_rate); an upscale's pixel rate is the dotclock (ilk_pipe_pixel_rate)
    let khz = fw.dotclock_khz.unwrap();
    assert_eq!(write_of(&p, 0x45270), (4000 * 1000 * 8u32).div_ceil(khz).min(0x1ff));
    let at = |reg: u32| p.forward.iter().position(|(r, _)| *r == reg).unwrap();
    assert!(at(0x68174) < at(0x45270) && at(0x45270) < at(0x70188), "intel_pipe_fastset: skl_pfit_enable, then hsw_set_linetime_wm, then the plane");
}
