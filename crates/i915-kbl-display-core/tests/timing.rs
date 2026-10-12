// SPDX-License-Identifier: GPL-2.0-only
//! Timing, PIPESRC and DP M/N decode against words built by Linux's ENCODE rule: every field is written minus one
//! (`intel_set_transcoder_timings`, intel_display.c:2704-2731), so the decode adds one. The literals below are those
//! encodings worked out by hand for two CTA-861 modes.
use i915_kbl_display_core::timing::*;

// CTA-861 VIC 16, 1920x1080@60: clock 148500, h 1920/2008/2052/2200, v 1080/1084/1089/1125.
const HD_HTOTAL: u32 = 0x0897_077f; // (2200-1) << 16 | (1920-1)
const HD_HBLANK: u32 = 0x0897_077f; // end 2199, start 1919 (blank = active..total)
const HD_HSYNC: u32 = 0x0803_07d7; // end 2051, start 2007
const HD_VTOTAL: u32 = 0x0464_0437; // (1125-1) << 16 | (1080-1)
const HD_VBLANK: u32 = 0x0464_0437;
const HD_VSYNC: u32 = 0x0440_043b; // end 1088, start 1083
const HD_SRC: u32 = 0x077f_0437; // PIPESRC: (1920-1) << 16 | (1080-1)

// CTA-861 VIC 97, 3840x2160@60: clock 594000, h 3840/4016/4104/4400, v 2160/2168/2178/2250.
const K4_HTOTAL: u32 = 0x112f_0eff;
const K4_HBLANK: u32 = 0x112f_0eff;
const K4_HSYNC: u32 = 0x1007_0faf;
const K4_VTOTAL: u32 = 0x08c9_086f;
const K4_VBLANK: u32 = 0x08c9_086f;
const K4_VSYNC: u32 = 0x0881_0877;
const K4_SRC: u32 = 0x0eff_086f;

fn raw(h: u32, hb: u32, hs: u32, v: u32, vb: u32, vs: u32) -> RawTimings {
    RawTimings { htotal: h, hblank: hb, hsync: hs, vtotal: v, vblank: vb, vsync: vs }
}

#[test]
fn decode_1080p60() {
    let t = decode_timings(&raw(HD_HTOTAL, HD_HBLANK, HD_HSYNC, HD_VTOTAL, HD_VBLANK, HD_VSYNC), false);
    assert_eq!(
        t,
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
    );
    assert_eq!(decode_pipe_src(HD_SRC), (1920, 1080));
    assert_eq!(vrefresh_hz(148_500, &t), 60);
    assert_eq!(vrefresh_millihz(148_500, &t), 60_000);
    // the NTSC-family clock, 148352 kHz, is 59.94 Hz: the millihertz unit is what tells them apart
    assert_eq!(vrefresh_hz(148_352, &t), 60);
    assert_eq!(vrefresh_millihz(148_352, &t), 59_940);
}

#[test]
fn decode_2160p60() {
    let t = decode_timings(&raw(K4_HTOTAL, K4_HBLANK, K4_HSYNC, K4_VTOTAL, K4_VBLANK, K4_VSYNC), false);
    assert_eq!(t.hdisplay, 3840);
    assert_eq!(t.htotal, 4400);
    assert_eq!((t.hblank_start, t.hblank_end), (3840, 4400));
    assert_eq!((t.hsync_start, t.hsync_end), (4016, 4104));
    assert_eq!(t.vdisplay, 2160);
    assert_eq!(t.vtotal, 2250);
    assert_eq!((t.vblank_start, t.vblank_end), (2160, 2250));
    assert_eq!((t.vsync_start, t.vsync_end), (2168, 2178));
    assert_eq!(decode_pipe_src(K4_SRC), (3840, 2160));
    assert_eq!(vrefresh_hz(594_000, &t), 60);
}

#[test]
fn every_timing_field_lands_in_its_own_slot() {
    // Distinct values in every half of every register: a swapped or mis-shifted field cannot hide.
    let t = decode_timings(
        &raw(0x0002_0001, 0x0004_0003, 0x0006_0005, 0x0008_0007, 0x000a_0009, 0x000c_000b),
        false,
    );
    assert_eq!((t.htotal, t.hdisplay), (3, 2));
    assert_eq!((t.hblank_end, t.hblank_start), (5, 4));
    assert_eq!((t.hsync_end, t.hsync_start), (7, 6));
    assert_eq!((t.vtotal, t.vdisplay), (9, 8));
    assert_eq!((t.vblank_end, t.vblank_start), (11, 10));
    assert_eq!((t.vsync_end, t.vsync_start), (13, 12));
    assert_eq!(decode_pipe_src(0x0010_0020), (0x11, 0x21));
}

#[test]
fn interlaced_adds_a_line_to_vtotal_and_vblank_end() {
    // CTA-861 VIC 5, 1920x1080i: clock 74250, frame lines 1125. The writer stores (vtotal - 1) - 1 and
    // (vblank_end - 1) - 1 for an interlaced mode (intel_display.c:2773-2774, :2797-2798), so the registers hold
    // 1123 = 0x463; the reader adds the extra line back (intel_display.c:2890-2893).
    let r = raw(HD_HTOTAL, HD_HBLANK, HD_HSYNC, 0x0463_0437, 0x0463_0437, 0x0440_043b);
    let p = decode_timings(&r, false);
    let i = decode_timings(&r, true);
    assert!(i.interlaced && !p.interlaced);
    assert_eq!(p.vtotal, 1124);
    assert_eq!(i.vtotal, 1125);
    assert_eq!(p.vblank_end, 1124);
    assert_eq!(i.vblank_end, 1125);
    assert_eq!(i.vblank_start, p.vblank_start);
    assert_eq!(i.vsync_end, p.vsync_end);
    // interlaced refresh counts fields: drm_mode_vrefresh doubles the numerator (drm_modes.c:1294-1295)
    assert_eq!(vrefresh_hz(74_250, &i), 60);
    assert_eq!(vrefresh_millihz(74_250, &i), 60_000);
    assert_eq!(vrefresh_hz(74_250, &p), 30); // the same clock progressive would be 30 Hz: the doubling is what matters
}

#[test]
fn interlace_bits_in_transconf() {
    // TRANSCONF_INTERLACE_MASK_HSW = bits 22:21 (intel_display_regs.h:758): every non-zero value is interlaced
    assert!(!transconf_interlaced(0x8000_0000));
    assert!(transconf_interlaced(0x8020_0000)); // 1: PF-ID
    assert!(transconf_interlaced(0x8040_0000)); // 2
    assert!(transconf_interlaced(0x8060_0000)); // 3: IF-ID
    assert!(!transconf_interlaced(0x8010_0000)); // bit 20 is TRANSCONF_REFRESH_RATE_ALT_ILK, not interlace
    assert!(!transconf_interlaced(0x8080_0000)); // bit 23 is outside the HSW field
}

#[test]
fn m_n_decode_masks_and_tu() {
    // DATA_M carries TU_SIZE in 30:25 as (tu - 1); every M/N is 24 bits (intel_display_regs.h:697-699)
    let mn = decode_m_n(0x7e69_9999, 0x0080_0000, 0x0004_6666, 0x0008_0000);
    assert_eq!(mn.tu, 64);
    assert_eq!(mn.data_m, 0x69_9999);
    assert_eq!(mn.data_n, 0x80_0000);
    assert_eq!(mn.link_m, 0x4_6666);
    assert_eq!(mn.link_n, 0x8_0000);
    // junk above bit 23 in the N/M words is masked off, TU only comes from DATA_M
    let mn = decode_m_n(0xffff_ffff, 0xff12_3456, 0xff00_0001, 0xff00_0002);
    assert_eq!((mn.data_m, mn.data_n, mn.link_m, mn.link_n), (0xff_ffff, 0x12_3456, 1, 2));
    assert_eq!(mn.tu, 0x3f + 1);
    assert_eq!(decode_m_n(0, 0, 0, 0).tu, 1);
}

#[test]
fn dotclock_from_link_m_n() {
    // Linux's M/N for the two modes (intel_link_compute_m_n: link_n = 0x80000, link_m = pixel * 0x80000 / symbol clock,
    // intel_display.c:2540-2546): 148500 on a 270000 link, 594000 on 540000.
    let hd = decode_m_n(0, 0, 288_358, 0x8_0000);
    assert_eq!(dotclock_from_m_n(270_000, &hd), 148_500);
    let k4 = decode_m_n(0, 0, 576_716, 0x8_0000);
    assert_eq!(dotclock_from_m_n(540_000, &k4), 594_000);
    // round UP (DIV_ROUND_UP_ULL, intel_display.c:4082): m = 1, n = 3 on a 100000 link is 33333.33 -> 33334
    assert_eq!(dotclock_from_m_n(100_000, &decode_m_n(0, 0, 1, 3)), 33_334);
    // and exact quotients are not bumped
    assert_eq!(dotclock_from_m_n(100_000, &decode_m_n(0, 0, 1, 4)), 25_000);
    // link_n == 0: Linux returns 0 (intel_display.c:4079)
    assert_eq!(dotclock_from_m_n(540_000, &decode_m_n(0, 0, 5, 0)), 0);
    // the largest M and port clock must not overflow 32 bits mid-way
    assert_eq!(dotclock_from_m_n(540_000, &decode_m_n(0, 0, 0xff_ffff, 0xff_ffff)), 540_000);
}

#[test]
fn vrefresh_guards_and_rounding() {
    let t = ModeTimings::default();
    assert_eq!(vrefresh_hz(148_500, &t), 0); // htotal == 0
    assert_eq!(vrefresh_millihz(148_500, &t), 0);
    let mut t = ModeTimings { htotal: 2200, vtotal: 1125, ..Default::default() };
    assert_eq!(vrefresh_hz(148_500, &t), 60);
    // DIV_ROUND_CLOSEST: 59.94 -> 60, 59.4 -> 59
    t.htotal = 1000;
    t.vtotal = 1000;
    assert_eq!(vrefresh_hz(59_940, &t), 60);
    assert_eq!(vrefresh_hz(59_400, &t), 59);
    assert_eq!(vrefresh_hz(59_500, &t), 60); // exactly .5 rounds up
}

#[test]
fn field_get_is_a_shifted_mask() {
    assert_eq!(field_get(0x0ff0, 0xabcd), 0xbc);
    assert_eq!(field_get(0xffff_0000, 0x1234_5678), 0x1234);
    assert_eq!(field_get(0x1, 3), 1);
}

#[test]
fn vrefresh_rounding_boundaries_and_guards() {
    // 1_000_000 pixels per frame: kHz clock / 1000 = Hz, so 59_450 kHz is 59.45 Hz -> 59 and 59_500 -> 60 (round half up)
    let t = ModeTimings { htotal: 1000, vtotal: 1000, ..Default::default() };
    assert_eq!(vrefresh_hz(59_449, &t), 59);
    assert_eq!(vrefresh_hz(59_450, &t), 59);
    assert_eq!(vrefresh_hz(59_499, &t), 59);
    assert_eq!(vrefresh_hz(59_500, &t), 60);
    // millihertz rounds the same way at its own scale: 1 kHz over 14 pixels/frame is 71428.571... mHz -> 71429
    let t = ModeTimings { htotal: 14, vtotal: 1, ..Default::default() };
    assert_eq!(vrefresh_millihz(1, &t), 71_429);
    // smallest totals are valid modes, not guards: htotal = vtotal = 1 gives the clock itself
    let t = ModeTimings { htotal: 1, vtotal: 1, ..Default::default() };
    assert_eq!(vrefresh_hz(7, &t), 7_000);
    assert_eq!(vrefresh_millihz(7, &t), 7_000_000);
    // either total being 0 is a mode with no totals
    let t = ModeTimings { htotal: 0, vtotal: 5, ..Default::default() };
    assert_eq!((vrefresh_hz(1000, &t), vrefresh_millihz(1000, &t)), (0, 0));
    let t = ModeTimings { htotal: 5, vtotal: 0, ..Default::default() };
    assert_eq!((vrefresh_hz(1000, &t), vrefresh_millihz(1000, &t)), (0, 0));
    // interlace doubles the numerator at both scales
    let t = ModeTimings { htotal: 1000, vtotal: 1000, interlaced: true, ..Default::default() };
    assert_eq!(vrefresh_hz(30_000, &t), 60);
    assert_eq!(vrefresh_millihz(30_000, &t), 60_000);
}
