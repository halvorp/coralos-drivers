// SPDX-License-Identifier: GPL-2.0-only
//! Properties of the watermark code that do not come from a gcc table: the register encoders (values from Linux's
//! `PLANE_WM_*` / `PLANE_BUF_*` macros evaluated by gcc), the fixed-point helpers, the vblank prefill limit, the named refusals,
//! and that every DDB allocation is inside the pipe's range with no overlap.
use i915_kbl_display_core::skl_wm::*;

#[test]
fn register_encoders_match_linux_macros() {
    // PLANE_WM_EN | PLANE_WM_BLOCKS(123) | PLANE_WM_LINES(17)  = 0x8004407b (gcc)
    let l = WmLevel { enable: true, ignore_lines: false, blocks: 123, lines: 17, min_ddb_alloc: 124 };
    assert_eq!(plane_wm_reg_val(&l), 0x8004_407b);
    assert_eq!(plane_wm_reg_val(&WmLevel::default()), 0);
    let ign = WmLevel { ignore_lines: true, ..l };
    assert_eq!(plane_wm_reg_val(&ign), 0xc004_407b);
    assert_eq!(plane_wm_reg_val(&WmLevel { blocks: 8191, ..WmLevel::default() }), 0x1fff);
    // PLANE_BUF_END(891) | PLANE_BUF_START(0) = 0x37b0000 ; PLANE_BUF_END(499) | PLANE_BUF_START(100) = 0x1f30064
    assert_eq!(plane_ddb_reg_val(&Ddb { start: 0, end: 892 }), 0x037b_0000);
    assert_eq!(plane_ddb_reg_val(&Ddb { start: 100, end: 500 }), 0x01f3_0064);
    assert_eq!(plane_ddb_reg_val(&Ddb::default()), 0, "an empty entry is register value 0");
    // cursor: CUR_WM_EN | blocks | lines<<14 ; CUR_BUF_END(end-1) << 16 | CUR_BUF_START
    assert_eq!(cursor_wm_reg_val(&l), 0x8004_407b);
    assert_eq!(cursor_ddb_reg_val(&Ddb { start: 860, end: 892 }), 0x037b_035c);
    assert_eq!(cursor_ddb_reg_val(&Ddb::default()), 0);
}

#[test]
fn fixed_point_helpers_follow_intel_fixed_h() {
    assert_eq!(Fx::from_u32(16).unwrap().0, 16 << 16);
    assert!(Fx::from_u32(0x1_0000).is_err());
    assert_eq!(Fx(0x1_0001).round_up(), 2);
    assert_eq!(Fx(0x1_0000).round_up(), 1);
    assert_eq!(Fx::div(7680, 512).unwrap().0, 15 << 16);
    assert_eq!(Fx::div(1, 3).unwrap().0, 21846, "ceil(65536 / 3)");
    assert_eq!(Fx::div(1, 0).unwrap_err().name(), "WM-ZERO-DIVISOR");
    assert_eq!(Fx::mul_u32(8, Fx(16 << 16)).unwrap().0, 128 << 16);
    assert!(Fx::mul_u32(u32::MAX, Fx(2)).is_err(), "a truncated fixed-point value is a named overflow");
    assert_eq!(Fx::mul_round_up(3, Fx(0x8000)).unwrap(), 2, "3 * 0.5 rounded up");
    assert_eq!(Fx(10 << 16).ratio_round_up(Fx(3 << 16)).unwrap(), 4);
}

#[test]
fn transition_watermark_is_zero_on_gen9_whatever_ipc_says() {
    assert_eq!(compute_transition_wm(), WmLevel::default());
}

fn sample_lat() -> Latencies {
    latencies_from_pcode(0x120a_0402, 0x3c32_281e)
}

#[test]
fn latency_decode_lanes() {
    assert_eq!(sample_lat(), [2, 4, 10, 18, 30, 40, 50, 60]);
}

#[test]
fn latency_zero_level_disables_the_rest_and_level0_zero_adds_two() {
    let mut l = latencies_from_pcode(0x0000_0400, 0);
    adjust_latencies(&mut l, &DramFacts { symmetric_memory: true, has_16gb_dimms: false });
    // wm[0] == 0 -> +2 on level 0 and on following non-zero levels until the first zero.
    assert_eq!(l, [2, 6, 0, 0, 0, 0, 0, 0]);
    let mut l = latencies_from_pcode(0x1204_0a02, 0);
    adjust_latencies(&mut l, &DramFacts { symmetric_memory: true, has_16gb_dimms: false });
    assert_eq!(l, [2, 10, 10, 18, 0, 0, 0, 0], "sanitize cuts at the first zero (level 4); make_wm_latency_monotonic raises level 2 from 4 to 10");
    assert_eq!(DramFacts::UNKNOWN.ipc_enabled(), false);
}

#[test]
fn x_tile_and_ipc_adders() {
    let lat = sample_lat();
    assert_eq!(wm_latency(&lat, 3, false, false), 18);
    assert_eq!(wm_latency(&lat, 3, true, false), 22, "WaIncreaseLatencyIPCEnabled");
    assert_eq!(wm_latency(&lat, 3, false, true), 33, "memory-bandwidth WA for X tiling");
    assert_eq!(wm_latency(&lat, 3, true, true), 37);
    let zero = [0u16; 8];
    assert_eq!(wm_latency(&zero, 0, true, true), 0, "a zero latency stays zero (level disabled)");
}

#[test]
fn rejected_levels_for_zero_latency_and_too_many_lines() {
    let t = WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines: 45 };
    let g = PlaneWmGeometry { width: 1920, cpp: 4, x_tiled: false, y_tiled: false, is_planar: false, color_plane: 0 };
    let wp = compute_wm_params(&t, &g).unwrap();
    let r = compute_plane_wm(3, 0, &wp, &t, &WmLevel::default()).unwrap();
    assert_eq!(r.min_ddb_alloc, u16::MAX);
    assert!(!r.enable);
    // A latency so long that lines > 31 is rejected, never truncated into the 5-bit field.
    let r = compute_plane_wm(7, 5000, &wp, &t, &WmLevel::default()).unwrap();
    assert_eq!(r.min_ddb_alloc, u16::MAX);
}

#[test]
fn ddb_is_inside_the_pipe_range_and_never_overlaps() {
    let lat = sample_lat();
    let t = WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines: 45 };
    let g = PlaneWmGeometry { width: 1920, cpp: 4, x_tiled: false, y_tiled: false, is_planar: false, color_plane: 0 };
    let v = |id: u8| VisiblePlane { plane_id: id, y: g, uv: None, src_h: 1080 };
    let r = compute_pipe_wm(&PipeWmInput {
        lat: &lat,
        ipc_enabled: false,
        timing: t,
        planes: [Some(v(0)), Some(v(1)), Some(v(2))],
        alloc: single_pipe_alloc(),
        num_active_pipes: 1,
    })
    .unwrap();
    let mut spans: Vec<(u16, u16)> = r.plane_ddb.iter().map(|d| (d.start, d.end)).collect();
    spans.push((r.cursor_ddb.start, r.cursor_ddb.end));
    spans.sort();
    for s in &spans {
        assert!(s.1 <= DBUF_SIZE_BLOCKS, "{s:?} beyond the DBUF");
    }
    for w in spans.windows(2) {
        assert!(w[0].1 <= w[1].0, "{w:?} overlap");
    }
    // The planes fill everything the cursor left, exactly (no block is stranded).
    let total: u32 = r.plane_ddb.iter().map(|d| d.size() as u32).sum::<u32>() + r.cursor_ddb.size() as u32;
    assert_eq!(total, DBUF_SIZE_BLOCKS as u32);
}

#[test]
fn ddb_exceeded_is_a_named_error() {
    let lat = sample_lat();
    let t = WmTiming { htotal: 8800, clock_khz: 2_376_000, pixel_rate_khz: 2_376_000, vblank_lines: 90 };
    let g = PlaneWmGeometry { width: 7680, cpp: 4, x_tiled: false, y_tiled: true, is_planar: false, color_plane: 0 };
    let v = |id: u8| VisiblePlane { plane_id: id, y: g, uv: None, src_h: 4320 };
    // Two Y-tiled 8K planes need 2 * (480 + 1) + 1 blocks at level 0 alone: more than the 860 the cursor reserve leaves.
    let e = compute_pipe_wm(&PipeWmInput {
        lat: &lat,
        ipc_enabled: false,
        timing: t,
        planes: [Some(v(0)), Some(v(1)), None],
        alloc: single_pipe_alloc(),
        num_active_pipes: 1,
    })
    .unwrap_err();
    assert_eq!(e.name(), "WM-DDB-EXCEEDED");
    // And an allocation too small for even the cursor reserve.
    let g1 = PlaneWmGeometry { y_tiled: false, ..g };
    let e = compute_pipe_wm(&PipeWmInput {
        lat: &lat,
        ipc_enabled: false,
        timing: t,
        planes: [Some(VisiblePlane { plane_id: 0, y: g1, uv: None, src_h: 4320 }), None, None],
        alloc: Ddb { start: 0, end: 16 },
        num_active_pipes: 1,
    })
    .unwrap_err();
    assert_eq!(e.name(), "WM-DDB-EXCEEDED");
}

#[test]
fn vblank_limit_picks_the_highest_level_the_blank_can_feed() {
    // Hand arithmetic from skl_prefill.c with the sample timing (148.5 MHz, htotal 2200), wm0 prefill 1 line, cdclk 337.5 MHz:
    //   fixed = 1 line + ceil(148500 * (20 << 16) / 2.2e6) = 65536 + 88474; cdclk part = ceil(65536 * 28836 / 65536) = 28836;
    //   full = 182846 (2.79 lines), plus ceil(148500 * (latency << 16) / 2.2e6) per level (latency 0 for level 0):
    //   level 4 (30 us) 315557 = 4.8 lines, level 5 (40 us) 359794 = 5.49, level 6 (50 us) 404030 = 6.17, level 7 (60 us) 448267 = 6.84.
    let lat = sample_lat();
    let t = |vblank_lines| WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines };
    assert_eq!(max_level_for_vblank(&lat, &t(2), 1, 337_500).unwrap_err().name(), "WM-VBLANK-TOO-SHORT");
    assert_eq!(max_level_for_vblank(&lat, &t(3), 1, 337_500).unwrap(), 0);
    assert_eq!(max_level_for_vblank(&lat, &t(5), 1, 337_500).unwrap(), 4);
    assert_eq!(max_level_for_vblank(&lat, &t(6), 1, 337_500).unwrap(), 5);
    assert_eq!(max_level_for_vblank(&lat, &t(7), 1, 337_500).unwrap(), 7);
    assert_eq!(max_level_for_vblank(&lat, &t(45), 1, 337_500).unwrap(), 7);
    // More wm0 lines to prefill push every level up: 5 lines of wm0 at 7 vblank lines no longer reaches level 7.
    assert!(max_level_for_vblank(&lat, &t(7), 5, 337_500).unwrap() < 7);
    // A slower cdclk raises the prefill (the adjustment factor tends to 1.0).
    assert!(max_level_for_vblank(&lat, &t(7), 1, 148_500).unwrap() < 7);
    assert_eq!(max_level_for_vblank(&lat, &t(7), 1, 0).unwrap_err().name(), "WM-ZERO-DIVISOR");
}

#[test]
fn cursor_reserve_is_32_for_one_pipe_8_otherwise_at_minimum() {
    let lat = sample_lat();
    let t = WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines: 45 };
    assert_eq!(cursor_allocation(&lat, false, &t, 1).unwrap(), 32);
    // Two active pipes: the floor is 8, but the computed minimum for a 256-wide ARGB cursor is higher here.
    assert!(cursor_allocation(&lat, false, &t, 2).unwrap() >= 8);
}

#[test]
fn limit_levels_only_clears_the_enable_bits() {
    let lat = sample_lat();
    let t = WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines: 45 };
    let g = PlaneWmGeometry { width: 1920, cpp: 4, x_tiled: false, y_tiled: false, is_planar: false, color_plane: 0 };
    let mut r = compute_pipe_wm(&PipeWmInput {
        lat: &lat,
        ipc_enabled: false,
        timing: t,
        planes: [Some(VisiblePlane { plane_id: 0, y: g, uv: None, src_h: 1080 }), None, None],
        alloc: single_pipe_alloc(),
        num_active_pipes: 1,
    })
    .unwrap();
    assert!(r.planes[0].wm[7].enable && r.planes[0].wm[2].enable);
    let (b7, l7) = (r.planes[0].wm[7].blocks, r.planes[0].wm[7].lines);
    r.limit_levels(2);
    assert!(r.planes[0].wm[2].enable);
    assert!(!r.planes[0].wm[3].enable && !r.planes[0].wm[7].enable);
    assert_eq!((r.planes[0].wm[7].blocks, r.planes[0].wm[7].lines), (b7, l7));
    assert_eq!(r.max_wm0_lines(), 0, "level 0 has no line count on display ver 9 (skl_wm_has_lines)");
}

#[test]
fn vblank_equality_is_enough() {
    // htotal 2000 at 100 MHz: 20 us of prefill is EXACTLY one line (100000 * (20 << 16) / (2000 * 1000) = 65536), so with the
    // framestart delay (1 line) the fixed prefill is exactly 2 lines. wm0 prefill 0, level 0 only (latency table [2, 0, ...]).
    let lat: Latencies = [2, 0, 0, 0, 0, 0, 0, 0];
    let t = |vblank_lines| WmTiming { htotal: 2000, clock_khz: 100_000, pixel_rate_khz: 100_000, vblank_lines };
    assert_eq!(max_level_for_vblank(&lat, &t(2), 0, 100_000).unwrap(), 0, "guardband == prefill is not 'too short' (skl_prefill_vblank_too_short uses <)");
    assert_eq!(max_level_for_vblank(&lat, &t(1), 0, 100_000).unwrap_err().name(), "WM-VBLANK-TOO-SHORT");
}

#[test]
fn accepted_levels_never_exceed_31_lines_and_31_is_reachable() {
    // skl_wm_max_lines() is 31 below display ver 13: sweep the latency and look at what is accepted.
    let t = WmTiming { htotal: 2200, clock_khz: 148500, pixel_rate_khz: 148500, vblank_lines: 45 };
    let g = PlaneWmGeometry { width: 1920, cpp: 4, x_tiled: false, y_tiled: false, is_planar: false, color_plane: 0 };
    let wp = compute_wm_params(&t, &g).unwrap();
    let mut best = 0u16;
    let mut first_reject = None;
    for latency in 1..1500u32 {
        let r = compute_plane_wm(3, latency, &wp, &t, &WmLevel::default()).unwrap();
        if r.min_ddb_alloc == u16::MAX {
            first_reject.get_or_insert(latency);
        } else {
            assert!(r.lines <= 31, "latency {latency}: {} lines accepted", r.lines);
            best = best.max(r.lines);
        }
    }
    assert_eq!(best, 31, "the largest accepted line count is the limit itself");
    assert!(first_reject.is_some());
}
