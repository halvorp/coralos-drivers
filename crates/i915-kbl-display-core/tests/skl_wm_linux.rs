// SPDX-License-Identifier: GPL-2.0-only
//! Watermark / DDB numbers for a one-pipe Kaby Lake configuration, EVALUATED BY GCC FROM LINUX'S OWN CODE: the functions
//! `skl_compute_wm_params`, `skl_compute_plane_wm`, `skl_compute_wm_levels`, `skl_wm_latency`, `adjust_wm_latency`,
//! `skl_cursor_allocation`, `skl_allocate_plane_ddb` and `skl_crtc_allocate_plane_ddb` were cut verbatim out of
//! drivers/gpu/drm/i915/display/skl_watermark.c (v7.0-rc6) and compiled with stub types for the atomic state, and these tables are
//! what that program printed. The expected side is Linux's number, written out literally; the actual side is the Rust port.
use i915_kbl_display_core::skl_wm::*;

struct Case {
    name: &'static str,
    clock_khz: u32,
    htotal: u32,
    width: u32,
    height: u32,
    nv12: bool,
    tiling: u8, // 0 linear, 1 X, 2 Y
    ipc: bool,
    dimm16: bool,
    lat_words: [u32; 2],
    lat_adjusted: [u16; 8],
    /// (linetime_us, plane_blocks_per_line as 16.16, y_tile_minimum as 16.16)
    wp: (u32, u32, u32),
    /// per level: (enable, blocks, lines, min_ddb_alloc)
    wm: [(bool, u16, u16, u16); 8],
    uv: [(bool, u16, u16, u16); 8],
    /// (plane ddb start, end), (plane Y ddb start, end), (cursor ddb start, end)
    ddb: ((u16, u16), (u16, u16), (u16, u16)),
}

const CASES: &[Case] = &[
    Case {
        name: "s1", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [2, 4, 10, 18, 30, 40, 50, 60],
        wp: (15, 1048576, 8388608),
        wm: [(true, 4, 0, 5), (true, 17, 1, 18), (true, 17, 1, 18), (true, 23, 2, 24), (true, 37, 3, 38), (true, 49, 3, 50), (true, 61, 4, 62), (true, 72, 5, 73)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s2", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 0, ipc: true, dimm16: true,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [3, 5, 11, 19, 31, 41, 51, 61],
        wp: (15, 1048576, 8388608),
        wm: [(true, 10, 0, 11), (true, 17, 1, 18), (true, 20, 2, 21), (true, 29, 2, 30), (true, 43, 3, 44), (true, 55, 4, 56), (true, 66, 4, 67), (true, 78, 5, 79)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s3", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 1, ipc: false, dimm16: true,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [3, 5, 11, 19, 31, 41, 51, 61],
        wp: (15, 983040, 7864320),
        wm: [(true, 22, 0, 23), (true, 26, 2, 27), (true, 32, 2, 33), (true, 42, 3, 43), (true, 56, 4, 57), (true, 62, 4, 63), (true, 77, 5, 78), (true, 91, 6, 92)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s4", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 2, ipc: false, dimm16: false,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [2, 4, 10, 18, 30, 40, 50, 60],
        wp: (15, 983040, 7864320),
        wm: [(true, 121, 0, 122), (true, 241, 16, 242), (true, 241, 16, 242), (true, 241, 16, 242), (true, 241, 16, 242), (true, 241, 16, 242), (true, 241, 16, 242), (true, 241, 16, 242)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s5", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x120a0400, 0x3c32281e], lat_adjusted: [2, 6, 12, 20, 32, 42, 52, 62],
        wp: (15, 1048576, 8388608),
        wm: [(true, 4, 0, 5), (true, 17, 1, 18), (true, 17, 1, 18), (true, 26, 2, 27), (true, 40, 3, 41), (true, 50, 3, 51), (true, 63, 4, 64), (true, 74, 5, 75)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s6", clock_khz: 594000, htotal: 4400, width: 3840, height: 2160, nv12: false, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [2, 4, 10, 18, 30, 40, 50, 60],
        wp: (8, 2031616, 16252928),
        wm: [(true, 11, 0, 12), (true, 32, 1, 33), (true, 49, 2, 50), (true, 86, 3, 87), (true, 142, 5, 143), (true, 188, 6, 189), (true, 219, 7, 220), (true, 281, 9, 282)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s7", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: true, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [2, 4, 10, 18, 30, 40, 50, 60],
        wp: (15, 327680, 2621440),
        wm: [(true, 2, 0, 3), (true, 6, 1, 7), (true, 6, 1, 7), (true, 8, 2, 9), (true, 11, 2, 12), (true, 14, 3, 15), (true, 17, 3, 18), (true, 20, 4, 21)],
        uv: [(true, 3, 0, 4), (true, 6, 1, 7), (true, 8, 2, 9), (true, 12, 2, 13), (true, 17, 3, 18), (true, 17, 3, 18), (true, 22, 4, 23), (true, 27, 5, 28)],
        ddb: ((562, 860), (0, 562), (860, 892)),
    },
    Case {
        name: "s8", clock_khz: 148500, htotal: 2200, width: 1920, height: 1080, nv12: false, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x000a0402, 0x3c32281e], lat_adjusted: [2, 4, 10, 0, 0, 0, 0, 0],
        wp: (15, 1048576, 8388608),
        wm: [(true, 4, 0, 5), (true, 17, 1, 18), (true, 17, 1, 18), (false, 17, 1, 0), (false, 17, 1, 0), (false, 17, 1, 0), (false, 17, 1, 0), (false, 17, 1, 0)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
    Case {
        name: "s9", clock_khz: 594000, htotal: 4400, width: 3840, height: 2160, nv12: false, tiling: 2, ipc: true, dimm16: true,
        lat_words: [0x120a0402, 0x3c32281e], lat_adjusted: [3, 5, 11, 19, 31, 41, 51, 61],
        wp: (8, 1966080, 15728640),
        wm: [(true, 241, 0, 242), (true, 481, 16, 482), (true, 481, 16, 482), (true, 481, 16, 482), (true, 481, 16, 482), (true, 481, 16, 482), (true, 481, 16, 482), (true, 511, 17, 512)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 857), (0, 0), (857, 892)),
    },
    // s10: latency == linetime_us (20 us == 20 us) at level 2 on a wide-blanking mode, where min(method1, method2) = method2 < method1:
    // pins the `latency >= linetime_us` boundary of skl_compute_plane_wm.
    Case {
        name: "s10", clock_khz: 100000, htotal: 2000, width: 1024, height: 768, nv12: false, tiling: 0, ipc: false, dimm16: false,
        lat_words: [0x00140402, 0x0], lat_adjusted: [2, 4, 20, 0, 0, 0, 0, 0],
        wp: (20, 589824, 4718592),
        wm: [(true, 3, 0, 4), (true, 10, 1, 11), (true, 11, 1, 12), (false, 11, 1, 0), (false, 11, 1, 0), (false, 11, 1, 0), (false, 11, 1, 0), (false, 11, 1, 0)],
        uv: [(false, 0, 0, 0); 8],
        ddb: ((0, 860), (0, 0), (860, 892)),
    },
];

fn level_tuple(l: &WmLevel) -> (bool, u16, u16, u16) {
    (l.enable, l.blocks, l.lines, l.min_ddb_alloc)
}

fn geometry(c: &Case, color_plane: u8) -> PlaneWmGeometry {
    PlaneWmGeometry {
        width: c.width,
        cpp: match (c.nv12, color_plane) {
            (false, _) => 4,
            (true, 0) => 1,
            (true, _) => 2,
        },
        x_tiled: c.tiling == 1,
        y_tiled: c.tiling == 2,
        is_planar: c.nv12,
        color_plane,
    }
}

fn timing(c: &Case) -> WmTiming {
    WmTiming { htotal: c.htotal, clock_khz: c.clock_khz, pixel_rate_khz: c.clock_khz, vblank_lines: 45 }
}

#[test]
fn latency_adjustment_matches_linux() {
    for c in CASES {
        let mut lat = latencies_from_pcode(c.lat_words[0], c.lat_words[1]);
        adjust_latencies(&mut lat, &DramFacts { symmetric_memory: c.ipc, has_16gb_dimms: c.dimm16 });
        assert_eq!(lat, c.lat_adjusted, "{}", c.name);
    }
}

#[test]
fn wm_params_match_linux() {
    for c in CASES {
        let wp = compute_wm_params(&timing(c), &geometry(c, 0)).unwrap();
        assert_eq!((wp.linetime_us, wp.plane_blocks_per_line.0, wp.y_tile_minimum.0), c.wp, "{}", c.name);
    }
}

#[test]
fn watermark_levels_and_ddb_match_linux() {
    for c in CASES {
        let mut lat = latencies_from_pcode(c.lat_words[0], c.lat_words[1]);
        adjust_latencies(&mut lat, &DramFacts { symmetric_memory: c.ipc, has_16gb_dimms: c.dimm16 });
        let vp = VisiblePlane {
            plane_id: 0,
            y: geometry(c, 0),
            uv: if c.nv12 { Some(geometry(c, 1)) } else { None },
            src_h: c.height,
        };
        let inp = PipeWmInput {
            lat: &lat,
            ipc_enabled: c.ipc,
            timing: timing(c),
            planes: [Some(vp), None, None],
            alloc: single_pipe_alloc(),
            num_active_pipes: 1,
        };
        let r = compute_pipe_wm(&inp).unwrap();
        for l in 0..8 {
            assert_eq!(level_tuple(&r.planes[0].wm[l]), c.wm[l], "{} wm level {}", c.name, l);
            if c.nv12 {
                assert_eq!(level_tuple(&r.planes[0].uv_wm[l]), c.uv[l], "{} uv level {}", c.name, l);
            }
        }
        assert_eq!(
            (
                (r.plane_ddb[0].start, r.plane_ddb[0].end),
                (r.plane_ddb_y[0].start, r.plane_ddb_y[0].end),
                (r.cursor_ddb.start, r.cursor_ddb.end)
            ),
            c.ddb,
            "{} ddb",
            c.name
        );
        // planes 2 and 3 are not visible: all zero.
        assert_eq!(r.plane_ddb[1], Ddb::default());
        assert_eq!(r.plane_ddb[2], Ddb::default());
    }
}
