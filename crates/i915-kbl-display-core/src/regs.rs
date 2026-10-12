// SPDX-License-Identifier: GPL-2.0-only
//! Every display register offset, field mask and field value the readout touches, each with the Linux FILE:LINE
//! it was taken from (intel_display_regs.h = drivers/gpu/drm/i915/display/intel_display_regs.h, likewise
//! skl_universal_plane_regs.h and intel_display_device.c, same directory).
//!
//! Kaby Lake's display MMIO base is 0 (`mmio_offset` is set only for VLV, intel_display_device.c:527, and
//! CHV, intel_display_device.c:648; `skl_display` at intel_display_device.c:661 sets none), so the offsets here are absolute
//! BAR0 offsets. A transcoder or pipe register's address is `TRANSCODER_X_OFFSET - TRANSCODER_A_OFFSET + <A reg>`
//! (INTEL_DISPLAY_DEVICE_TRANS_OFFSET, intel_display_device.h:270-273), implemented by [`trans_reg`] / [`pipe_reg`].
//! Copyright (c) Intel Corporation.

use crate::types::{Ddi, Pipe, Transcoder};

// ---- per-transcoder / per-pipe base offsets (HSW_PIPE_OFFSETS, intel_display_device.c:142-154) ----
pub const PIPE_A_OFFSET: u32 = 0x70000; // intel_display_device.c:79
pub const PIPE_B_OFFSET: u32 = 0x71000; // intel_display_device.c:80
pub const PIPE_C_OFFSET: u32 = 0x72000; // intel_display_device.c:81
pub const PIPE_EDP_OFFSET: u32 = 0x7f000; // intel_display_device.c:90
pub const TRANSCODER_A_OFFSET: u32 = 0x60000; // intel_display_device.c:96
pub const TRANSCODER_B_OFFSET: u32 = 0x61000; // intel_display_device.c:97
pub const TRANSCODER_C_OFFSET: u32 = 0x62000; // intel_display_device.c:98
pub const TRANSCODER_EDP_OFFSET: u32 = 0x6f000; // intel_display_device.c:101

const fn trans_offset(t: Transcoder) -> u32 {
    match t {
        Transcoder::A => TRANSCODER_A_OFFSET,
        Transcoder::B => TRANSCODER_B_OFFSET,
        Transcoder::C => TRANSCODER_C_OFFSET,
        Transcoder::Edp => TRANSCODER_EDP_OFFSET,
    }
}
const fn pipe_offset(t: Transcoder) -> u32 {
    match t {
        Transcoder::A => PIPE_A_OFFSET,
        Transcoder::B => PIPE_B_OFFSET,
        Transcoder::C => PIPE_C_OFFSET,
        Transcoder::Edp => PIPE_EDP_OFFSET,
    }
}

/// `_MMIO_TRANS2(display, trans, reg_a)`: the transcoder `t` copy of the transcoder-A register `reg_a`
/// (intel_display_reg_defs.h:39, offset arithmetic at intel_display_device.h:270-273).
pub const fn trans_reg(t: Transcoder, reg_a: u32) -> u32 {
    trans_offset(t) - TRANSCODER_A_OFFSET + reg_a
}
/// `_MMIO_PIPE2(display, pipe, reg_a)` (intel_display_reg_defs.h:38, intel_display_device.h:265-268). Indexed by
/// TRANSCODER because TRANSCONF(display, trans) passes the transcoder (the eDP transcoder keeps its pipe-register block).
pub const fn pipe_reg(t: Transcoder, reg_a: u32) -> u32 {
    pipe_offset(t) - PIPE_A_OFFSET + reg_a
}

// ---- transcoder timing registers (transcoder-A copies; intel_display_regs.h:346-412) ----
pub const TRANS_HTOTAL_A: u32 = 0x60000; // intel_display_regs.h:346
pub const HTOTAL_MASK: u32 = 0xffff_0000; // intel_display_regs.h:349 REG_GENMASK(31, 16)
pub const HACTIVE_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:351 REG_GENMASK(15, 0)
pub const TRANS_HBLANK_A: u32 = 0x60004; // intel_display_regs.h:354
pub const HBLANK_END_MASK: u32 = 0xffff_0000; // intel_display_regs.h:357
pub const HBLANK_START_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:359
pub const TRANS_HSYNC_A: u32 = 0x60008; // intel_display_regs.h:362
pub const HSYNC_END_MASK: u32 = 0xffff_0000; // intel_display_regs.h:365
pub const HSYNC_START_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:367
pub const TRANS_VTOTAL_A: u32 = 0x6000c; // intel_display_regs.h:370
pub const VTOTAL_MASK: u32 = 0xffff_0000; // intel_display_regs.h:373
pub const VACTIVE_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:375
pub const TRANS_VBLANK_A: u32 = 0x60010; // intel_display_regs.h:378
pub const VBLANK_END_MASK: u32 = 0xffff_0000; // intel_display_regs.h:381
pub const VBLANK_START_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:383
pub const TRANS_VSYNC_A: u32 = 0x60014; // intel_display_regs.h:386
pub const VSYNC_END_MASK: u32 = 0xffff_0000; // intel_display_regs.h:389
pub const VSYNC_START_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:391
pub const PIPEASRC: u32 = 0x6001c; // intel_display_regs.h:394
pub const PIPESRC_WIDTH_MASK: u32 = 0xffff_0000; // intel_display_regs.h:397
pub const PIPESRC_HEIGHT_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:399
pub const TRANS_MULT_A: u32 = 0x6002c; // intel_display_regs.h:410

// ---- TRANSCONF (pipe-register block; intel_display_regs.h:730-732, 758) ----
pub const TRANSACONF: u32 = 0x70008; // intel_display_regs.h:730
pub const TRANSCONF_ENABLE: u32 = 1 << 31; // intel_display_regs.h:732
pub const TRANSCONF_INTERLACE_MASK_HSW: u32 = 0x0060_0000; // intel_display_regs.h:758 REG_GENMASK(22, 21)

// ---- DP link M/N (transcoder block; intel_display_regs.h:1023-1043, 697-699) ----
pub const PIPEA_DATA_M1: u32 = 0x60030; // intel_display_regs.h:1023
pub const PIPEA_DATA_N1: u32 = 0x60034; // intel_display_regs.h:1027
pub const PIPEA_LINK_M1: u32 = 0x60040; // intel_display_regs.h:1039
pub const PIPEA_LINK_N1: u32 = 0x60044; // intel_display_regs.h:1043
pub const TU_SIZE_MASK: u32 = 0x7e00_0000; // intel_display_regs.h:697 REG_GENMASK(30, 25)
pub const DATA_LINK_M_N_MASK: u32 = 0x00ff_ffff; // intel_display_regs.h:699 REG_GENMASK(23, 0)

// ---- TRANS_DDI_FUNC_CTL (intel_display_regs.h:2228-2273) ----
pub const TRANS_DDI_FUNC_CTL_A: u32 = 0x60400; // intel_display_regs.h:2228
pub const TRANS_DDI_FUNC_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2237
pub const TRANS_DDI_PORT_SHIFT: u32 = 28; // intel_display_regs.h:2239
pub const TRANS_DDI_PORT_MASK: u32 = 7 << 28; // intel_display_regs.h:2241
pub const TRANS_DDI_MODE_SELECT_MASK: u32 = 7 << 24; // intel_display_regs.h:2245
pub const TRANS_DDI_MODE_SELECT_HDMI: u32 = 0 << 24; // intel_display_regs.h:2246
pub const TRANS_DDI_MODE_SELECT_DVI: u32 = 1 << 24; // intel_display_regs.h:2247
pub const TRANS_DDI_MODE_SELECT_DP_SST: u32 = 2 << 24; // intel_display_regs.h:2248
pub const TRANS_DDI_MODE_SELECT_DP_MST: u32 = 3 << 24; // intel_display_regs.h:2249
pub const TRANS_DDI_MODE_SELECT_FDI_OR_128B132B: u32 = 4 << 24; // intel_display_regs.h:2250
pub const TRANS_DDI_BPC_MASK: u32 = 7 << 20; // intel_display_regs.h:2251
pub const TRANS_DDI_BPC_8: u32 = 0 << 20; // intel_display_regs.h:2252
pub const TRANS_DDI_BPC_10: u32 = 1 << 20; // intel_display_regs.h:2253
pub const TRANS_DDI_BPC_6: u32 = 2 << 20; // intel_display_regs.h:2254
pub const TRANS_DDI_BPC_12: u32 = 3 << 20; // intel_display_regs.h:2255
pub const TRANS_DDI_PVSYNC: u32 = 1 << 17; // intel_display_regs.h:2258
pub const TRANS_DDI_PHSYNC: u32 = 1 << 16; // intel_display_regs.h:2259
pub const TRANS_DDI_EDP_INPUT_MASK: u32 = 7 << 12; // intel_display_regs.h:2262
pub const TRANS_DDI_EDP_INPUT_A_ON: u32 = 0 << 12; // intel_display_regs.h:2263
pub const TRANS_DDI_EDP_INPUT_A_ONOFF: u32 = 4 << 12; // intel_display_regs.h:2264
pub const TRANS_DDI_EDP_INPUT_B_ONOFF: u32 = 5 << 12; // intel_display_regs.h:2265
pub const TRANS_DDI_EDP_INPUT_C_ONOFF: u32 = 6 << 12; // intel_display_regs.h:2266
pub const TRANS_DDI_EDP_INPUT_D_ONOFF: u32 = 7 << 12; // intel_display_regs.h:2267
pub const TRANS_DDI_DP_VC_PAYLOAD_ALLOC: u32 = 1 << 8; // intel_display_regs.h:2273

// ---- DDI_BUF_CTL (intel_display_regs.h:2345-2376) ----
pub const DDI_BUF_CTL_A: u32 = 0x64000; // intel_display_regs.h:2345
pub const DDI_BUF_CTL_B: u32 = 0x64100; // intel_display_regs.h:2346
pub const DDI_BUF_CTL_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2349
pub const DDI_BUF_EMP_MASK: u32 = 0x0f00_0000; // intel_display_regs.h:2352 REG_GENMASK(27, 24)
pub const DDI_BUF_PORT_REVERSAL: u32 = 1 << 16; // intel_display_regs.h:2360
pub const DDI_BUF_IS_IDLE: u32 = 1 << 7; // intel_display_regs.h:2364
pub const DDI_A_4_LANES: u32 = 1 << 4; // intel_display_regs.h:2366
pub const DDI_PORT_WIDTH_MASK: u32 = 0x0000_000e; // intel_display_regs.h:2367 REG_GENMASK(3, 1)
pub const DDI_PORT_WIDTH_SHIFT: u32 = 1; // intel_display_regs.h:2375
pub const DDI_INIT_DISPLAY_DETECTED: u32 = 1 << 0; // intel_display_regs.h:2376

// ---- DPLL (SKL class; intel_display_regs.h:2556-2611, intel_dpll_mgr.c:1356-1379) ----
pub const DPLL_CTRL1: u32 = 0x6c058; // intel_display_regs.h:2556
pub const DPLL_CTRL1_LINK_RATE_2700: u32 = 0; // intel_display_regs.h:2563
pub const DPLL_CTRL1_LINK_RATE_1350: u32 = 1; // intel_display_regs.h:2564
pub const DPLL_CTRL1_LINK_RATE_810: u32 = 2; // intel_display_regs.h:2565
pub const DPLL_CTRL1_LINK_RATE_1620: u32 = 3; // intel_display_regs.h:2566
pub const DPLL_CTRL1_LINK_RATE_1080: u32 = 4; // intel_display_regs.h:2567
pub const DPLL_CTRL1_LINK_RATE_2160: u32 = 5; // intel_display_regs.h:2568
pub const DPLL_CTRL2: u32 = 0x6c05c; // intel_display_regs.h:2571
pub const DPLL_STATUS: u32 = 0x6c060; // intel_display_regs.h:2579
pub const DPLL1_CFGCR1: u32 = 0x6c040; // intel_display_regs.h:2583
pub const DPLL2_CFGCR1: u32 = 0x6c048; // intel_display_regs.h:2584
pub const DPLL1_CFGCR2: u32 = 0x6c044; // intel_display_regs.h:2592
pub const DPLL2_CFGCR2: u32 = 0x6c04c; // intel_display_regs.h:2593
pub const DPLL_CFGCR1_DCO_FRACTION_MASK: u32 = 0x7fff << 9; // intel_display_regs.h:2588
pub const DPLL_CFGCR1_DCO_INTEGER_MASK: u32 = 0x1ff; // intel_display_regs.h:2590
pub const DPLL_CFGCR2_QDIV_RATIO_MASK: u32 = 0xff << 8; // intel_display_regs.h:2596
pub const DPLL_CFGCR2_QDIV_MODE_BIT: u32 = 1 << 7; // intel_display_regs.h:2598 DPLL_CFGCR2_QDIV_MODE(1)
pub const DPLL_CFGCR2_KDIV_MASK: u32 = 3 << 5; // intel_display_regs.h:2599
pub const DPLL_CFGCR2_KDIV_5: u32 = 0 << 5; // intel_display_regs.h:2601
pub const DPLL_CFGCR2_KDIV_2: u32 = 1 << 5; // intel_display_regs.h:2602
pub const DPLL_CFGCR2_KDIV_3: u32 = 2 << 5; // intel_display_regs.h:2603
pub const DPLL_CFGCR2_KDIV_1: u32 = 3 << 5; // intel_display_regs.h:2604
pub const DPLL_CFGCR2_PDIV_MASK: u32 = 7 << 2; // intel_display_regs.h:2605
pub const DPLL_CFGCR2_PDIV_1: u32 = 0 << 2; // intel_display_regs.h:2607
pub const DPLL_CFGCR2_PDIV_2: u32 = 1 << 2; // intel_display_regs.h:2608
pub const DPLL_CFGCR2_PDIV_3: u32 = 2 << 2; // intel_display_regs.h:2609
pub const DPLL_CFGCR2_PDIV_7: u32 = 4 << 2; // intel_display_regs.h:2610
pub const DPLL_CFGCR2_PDIV_7_INVALID: u32 = 5 << 2; // intel_display_regs.h:2611
pub const LCPLL1_CTL: u32 = 0x46010; // intel_display_regs.h:2551
pub const LCPLL2_CTL: u32 = 0x46014; // intel_display_regs.h:2552
pub const LCPLL_PLL_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2553
pub const LCPLL_PLL_LOCK: u32 = 1 << 30; // intel_display_regs.h:2497
pub const WRPLL_CTL1: u32 = 0x46040; // intel_display_regs.h:2423
pub const WRPLL_CTL2: u32 = 0x46060; // intel_display_regs.h:2424

// ---- CDCLK (intel_display_regs.h:2518-2540) ----
pub const CDCLK_CTL: u32 = 0x46000; // intel_display_regs.h:2518
pub const CDCLK_FREQ_SEL_MASK: u32 = 3 << 26; // intel_display_regs.h:2519
pub const CDCLK_FREQ_450_432: u32 = 0 << 26; // intel_display_regs.h:2520
pub const CDCLK_FREQ_540: u32 = 1 << 26; // intel_display_regs.h:2521
pub const CDCLK_FREQ_337_308: u32 = 2 << 26; // intel_display_regs.h:2522
pub const CDCLK_FREQ_675_617: u32 = 3 << 26; // intel_display_regs.h:2523
pub const CDCLK_FREQ_DECIMAL_MASK: u32 = 0x7ff; // intel_display_regs.h:2540

// ---- power wells (intel_display_regs.h:2125-2146, 2223-2225, 2820-2829) ----
pub const HSW_PWR_WELL_CTL1: u32 = 0x45400; // intel_display_regs.h:2125
pub const HSW_PWR_WELL_CTL2: u32 = 0x45404; // intel_display_regs.h:2126
pub const SKL_PW_CTL_IDX_PW_2: u32 = 15; // intel_display_regs.h:2136
pub const SKL_PW_CTL_IDX_PW_1: u32 = 14; // intel_display_regs.h:2137
pub const SKL_PW_CTL_IDX_DDI_D: u32 = 4; // intel_display_regs.h:2141
pub const SKL_PW_CTL_IDX_DDI_C: u32 = 3; // intel_display_regs.h:2142
pub const SKL_PW_CTL_IDX_DDI_B: u32 = 2; // intel_display_regs.h:2143
pub const SKL_PW_CTL_IDX_DDI_A_E: u32 = 1; // intel_display_regs.h:2144
pub const SKL_PW_CTL_IDX_MISC_IO: u32 = 0; // intel_display_regs.h:2146
pub const SKL_FUSE_STATUS: u32 = 0x42000; // intel_display_regs.h:2223
pub const SKL_FUSE_DOWNLOAD_STATUS: u32 = 1 << 31; // intel_display_regs.h:2224
pub const DC_STATE_EN: u32 = 0x45504; // intel_display_regs.h:2820
pub const DC_STATE_EN_UPTO_DC5: u32 = 1 << 0; // intel_display_regs.h:2826
pub const DC_STATE_EN_DC9: u32 = 1 << 3; // intel_display_regs.h:2827
pub const DC_STATE_EN_UPTO_DC6: u32 = 2 << 0; // intel_display_regs.h:2828
pub const DC_STATE_EN_UPTO_DC5_DC6_MASK: u32 = 0x3; // intel_display_regs.h:2829

// ---- universal planes (skl_universal_plane_regs.h) ----
pub const PLANE_CTL_1_A: u32 = 0x70180; // skl_universal_plane_regs.h:31
pub const PLANE_STRIDE_1_A: u32 = 0x70188; // skl_universal_plane_regs.h:102
pub const PLANE_POS_1_A: u32 = 0x7018c; // skl_universal_plane_regs.h:112
pub const PLANE_SIZE_1_A: u32 = 0x70190; // skl_universal_plane_regs.h:124
pub const PLANE_SURF_1_A: u32 = 0x7019c; // skl_universal_plane_regs.h:153
pub const PLANE_OFFSET_1_A: u32 = 0x701a4; // skl_universal_plane_regs.h:174
pub const PLANE_CTL_ENABLE: u32 = 1 << 31; // skl_universal_plane_regs.h:38
pub const PLANE_CTL_FORMAT_MASK_SKL: u32 = 0x0f00_0000; // skl_universal_plane_regs.h:48 REG_GENMASK(27, 24)
pub const PLANE_CTL_FORMAT_YUV422: u32 = 0 << 24; // skl_universal_plane_regs.h:50
pub const PLANE_CTL_FORMAT_NV12: u32 = 1 << 24; // skl_universal_plane_regs.h:51
pub const PLANE_CTL_FORMAT_XRGB_2101010: u32 = 2 << 24; // skl_universal_plane_regs.h:52
pub const PLANE_CTL_FORMAT_P010: u32 = 3 << 24; // skl_universal_plane_regs.h:53
pub const PLANE_CTL_FORMAT_XRGB_8888: u32 = 4 << 24; // skl_universal_plane_regs.h:54
pub const PLANE_CTL_FORMAT_P012: u32 = 5 << 24; // skl_universal_plane_regs.h:55
pub const PLANE_CTL_FORMAT_XRGB_16161616F: u32 = 6 << 24; // skl_universal_plane_regs.h:56
pub const PLANE_CTL_FORMAT_P016: u32 = 7 << 24; // skl_universal_plane_regs.h:57
pub const PLANE_CTL_FORMAT_XYUV: u32 = 8 << 24; // skl_universal_plane_regs.h:58
pub const PLANE_CTL_FORMAT_INDEXED: u32 = 12 << 24; // skl_universal_plane_regs.h:59
pub const PLANE_CTL_FORMAT_RGB_565: u32 = 14 << 24; // skl_universal_plane_regs.h:60
pub const PLANE_CTL_ORDER_RGBX: u32 = 1 << 20; // skl_universal_plane_regs.h:71
pub const PLANE_CTL_RENDER_DECOMPRESSION_ENABLE: u32 = 1 << 15; // skl_universal_plane_regs.h:79
pub const PLANE_CTL_TILED_MASK: u32 = 7 << 10; // skl_universal_plane_regs.h:83
pub const PLANE_CTL_TILED_LINEAR: u32 = 0 << 10; // skl_universal_plane_regs.h:84
pub const PLANE_CTL_TILED_X: u32 = 1 << 10; // skl_universal_plane_regs.h:85
pub const PLANE_CTL_TILED_Y: u32 = 4 << 10; // skl_universal_plane_regs.h:86
pub const PLANE_CTL_TILED_YF: u32 = 5 << 10; // skl_universal_plane_regs.h:87
pub const PLANE_CTL_ALPHA_MASK: u32 = 3 << 4; // skl_universal_plane_regs.h:92 REG_GENMASK(5, 4)
pub const PLANE_CTL_ROTATE_MASK: u32 = 3; // skl_universal_plane_regs.h:96
pub const PLANE_CTL_ROTATE_0: u32 = 0; // skl_universal_plane_regs.h:97
pub const PLANE_CTL_ROTATE_90: u32 = 1; // skl_universal_plane_regs.h:98
pub const PLANE_CTL_ROTATE_180: u32 = 2; // skl_universal_plane_regs.h:99
pub const PLANE_CTL_ROTATE_270: u32 = 3; // skl_universal_plane_regs.h:100
pub const PLANE_STRIDE_MASK: u32 = 0xfff; // skl_universal_plane_regs.h:109 PLANE_STRIDE__MASK REG_GENMASK(11, 0)
pub const PLANE_POS_Y_MASK: u32 = 0xffff_0000; // skl_universal_plane_regs.h:119
pub const PLANE_POS_X_MASK: u32 = 0x0000_ffff; // skl_universal_plane_regs.h:121
pub const PLANE_HEIGHT_MASK: u32 = 0xffff_0000; // skl_universal_plane_regs.h:131
pub const PLANE_WIDTH_MASK: u32 = 0x0000_ffff; // skl_universal_plane_regs.h:133
pub const PLANE_SURF_ADDR_MASK: u32 = 0xffff_f000; // skl_universal_plane_regs.h:160 REG_GENMASK(31, 12)
pub const PLANE_OFFSET_Y_MASK: u32 = 0xffff_0000; // skl_universal_plane_regs.h:181
pub const PLANE_OFFSET_X_MASK: u32 = 0x0000_ffff; // skl_universal_plane_regs.h:183
/// `_PLANE(plane, a, b)` = `_PICK_EVEN`: plane N of a pipe sits N * 0x100 above plane 1 (intel_display_reg_defs.h:17,
/// _PICK_EVEN at i915_reg_defs.h:127; PLANE_CTL_2_A - PLANE_CTL_1_A = 0x70280 - 0x70180, skl_universal_plane_regs.h:31-32).
pub const PLANE_STEP: u32 = 0x100;
/// `_PIPE(pipe, a, b)` = `_PICK_EVEN`: pipe B is 0x1000 above pipe A (intel_display_reg_defs.h:16;
/// PLANE_CTL_1_B - PLANE_CTL_1_A = 0x71180 - 0x70180, skl_universal_plane_regs.h:31,33).
pub const PIPE_STEP: u32 = 0x1000;

// ---- SKL pipe scalers (panel fitter; intel_display_regs.h:1059-1157) ----
pub const PS_1A_CTRL: u32 = 0x68180; // intel_display_regs.h:1059
pub const PS_2A_CTRL: u32 = 0x68280; // intel_display_regs.h:1060
pub const PS_SCALER_EN: u32 = 1 << 31; // intel_display_regs.h:1067
pub const PS_BINDING_MASK: u32 = 7 << 25; // intel_display_regs.h:1079
pub const PS_BINDING_PIPE: u32 = 0 << 25; // intel_display_regs.h:1080
pub const PS_WIN_POS_1A: u32 = 0x68170; // intel_display_regs.h:1134
pub const PS_WIN_XPOS_MASK: u32 = 0xffff_0000; // intel_display_regs.h:1142
pub const PS_WIN_YPOS_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:1144
pub const PS_WIN_SZ_1A: u32 = 0x68174; // intel_display_regs.h:1147
pub const PS_WIN_XSIZE_MASK: u32 = 0xffff_0000; // intel_display_regs.h:1155
pub const PS_WIN_YSIZE_MASK: u32 = 0x0000_ffff; // intel_display_regs.h:1157
/// Scaler 2 of a pipe is 0x100 above scaler 1 (`_ID(id, _PS_1A_CTRL, _PS_2A_CTRL)`, intel_display_regs.h:1058-1060);
/// pipe B's scaler block is 0x800 above pipe A's (`_PS_1B_CTRL - _PS_1A_CTRL = 0x68980 - 0x68180`, :1059-1061).
pub const PS_ID_STEP: u32 = 0x100;
pub const PS_PIPE_STEP: u32 = 0x800;

/// DDI_BUF_CTL(port) = `_PICK_EVEN(port, _DDI_BUF_CTL_A, _DDI_BUF_CTL_B)` (intel_display_regs.h:2348).
pub const fn ddi_buf_ctl(port: Ddi) -> u32 {
    DDI_BUF_CTL_A + (port as u32) * (DDI_BUF_CTL_B - DDI_BUF_CTL_A)
}
/// TRANS_DDI_FUNC_CTL(trans) (intel_display_regs.h:2235).
pub const fn trans_ddi_func_ctl(t: Transcoder) -> u32 {
    trans_reg(t, TRANS_DDI_FUNC_CTL_A)
}
/// PLANE_<reg>(pipe, plane_id) for the registers above: `_PIPE` then `_PLANE` (skl_universal_plane_regs.h:11-12).
pub const fn plane_reg(plane_1_a: u32, pipe: Pipe, plane_id: u32) -> u32 {
    plane_1_a + (pipe as u32) * PIPE_STEP + plane_id * PLANE_STEP
}
/// SKL_PS_*(pipe, id) (intel_display_regs.h:1062-1064, :1137-1139).
pub const fn scaler_reg(ps_1a: u32, pipe: Pipe, id: u32) -> u32 {
    ps_1a + (pipe as u32) * PS_PIPE_STEP + id * PS_ID_STEP
}

// ---- registers the clock/power WRITE side needs (moved from i915-display-clock-core, sentinel audit 2026-10-12) -------------
// Ported from Linux drivers/gpu/drm/i915/display/intel_display_regs.h at v7.0-rc6.

/// `DPLL_CFGCR1_FREQ_ENABLE` (intel_display_regs.h:2587).
pub const DPLL_CFGCR1_FREQ_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2587
/// `DPLL_CFGCR2_CENTRAL_FREQ_MASK` (intel_display_regs.h:2612).
pub const DPLL_CFGCR2_CENTRAL_FREQ_MASK: u32 = 3; // intel_display_regs.h:2612
/// `CDCLK_DIVMUX_CD_OVERRIDE` (intel_display_regs.h:2533).
pub const CDCLK_DIVMUX_CD_OVERRIDE: u32 = 1 << 19; // intel_display_regs.h:2533

/// `SKL_DFSM` (intel_display_regs.h:1539): the display fuse word whose bits 24:23 cap CDCLK.
pub const SKL_DFSM: u32 = 0x51000; // intel_display_regs.h:1539
pub const SKL_DFSM_CDCLK_LIMIT_MASK: u32 = 3 << 23; // intel_display_regs.h:1542
pub const SKL_DFSM_CDCLK_LIMIT_675: u32 = 0 << 23; // intel_display_regs.h:1543
pub const SKL_DFSM_CDCLK_LIMIT_540: u32 = 1 << 23; // intel_display_regs.h:1544
pub const SKL_DFSM_CDCLK_LIMIT_450: u32 = 2 << 23; // intel_display_regs.h:1545
pub const SKL_DFSM_CDCLK_LIMIT_337_5: u32 = 3 << 23; // intel_display_regs.h:1546

/// `HSW_PWR_WELL_CTL3` / `CTL4`: the KVMR and DEBUG requester registers (intel_display_regs.h:2127-2128). Never written here.
pub const HSW_PWR_WELL_CTL3: u32 = 0x45408; // intel_display_regs.h:2127
pub const HSW_PWR_WELL_CTL4: u32 = 0x4540c; // intel_display_regs.h:2128

/// `enum skl_power_gate` (intel_display_regs.h:2215-2221): the fuse-distribution gates SKL_FUSE_STATUS reports.
pub const SKL_PG0: u32 = 0; // intel_display_regs.h:2216
pub const SKL_PG1: u32 = 1; // intel_display_regs.h:2217
pub const SKL_PG2: u32 = 2; // intel_display_regs.h:2218

/// `SWF_ILK(0x18)`: the pre-OS "display initialised" scratch word `skl_sanitize_cdclk` reads (intel_display_regs.h:1004,
/// intel_cdclk.c:1239).
pub const SWF_ILK_18: u32 = 0x4f000 + 0x18 * 4; // intel_display_regs.h:1004

// ---- DDI DP transport, buffer translation, balance leg, PCH raw clock (re-homed from i915-display-link-core, 2026-10-12) ----
// Ported from Linux drivers/gpu/drm/i915/display/intel_display_regs.h at v7.0-rc6.
pub const DP_TP_CTL_A: u32 = 0x64040; // intel_display_regs.h:2302
pub const DP_TP_CTL_B: u32 = 0x64140; // intel_display_regs.h:2303
pub const DP_TP_STATUS_A: u32 = 0x64044; // intel_display_regs.h:2329
pub const DP_TP_STATUS_B: u32 = 0x64144; // intel_display_regs.h:2330
pub const DP_TP_CTL_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2307
pub const DP_TP_CTL_MODE_MASK: u32 = 1 << 27; // intel_display_regs.h:2309
pub const DP_TP_CTL_MODE_SST: u32 = 0; // intel_display_regs.h:2310 REG_FIELD_PREP(MASK, 0)
pub const DP_TP_CTL_MODE_MST: u32 = 1 << 27; // intel_display_regs.h:2311 REG_FIELD_PREP(MASK, 1)
pub const DP_TP_CTL_ENHANCED_FRAME_ENABLE: u32 = 1 << 18; // intel_display_regs.h:2317
pub const DP_TP_CTL_LINK_TRAIN_MASK: u32 = 7 << 8; // intel_display_regs.h:2319 REG_GENMASK(10, 8)
pub const DP_TP_CTL_LINK_TRAIN_PAT1: u32 = 0 << 8; // intel_display_regs.h:2320
pub const DP_TP_CTL_LINK_TRAIN_PAT2: u32 = 1 << 8; // intel_display_regs.h:2321
pub const DP_TP_CTL_LINK_TRAIN_PAT3: u32 = 4 << 8; // intel_display_regs.h:2322
pub const DP_TP_CTL_LINK_TRAIN_PAT4: u32 = 5 << 8; // intel_display_regs.h:2323
pub const DP_TP_CTL_LINK_TRAIN_IDLE: u32 = 2 << 8; // intel_display_regs.h:2324
pub const DP_TP_CTL_LINK_TRAIN_NORMAL: u32 = 3 << 8; // intel_display_regs.h:2325
pub const DP_TP_STATUS_IDLE_DONE: u32 = 1 << 25; // intel_display_regs.h:2335
/// `DP_TP_CTL(port)` = `_MMIO_PORT(port, _DP_TP_CTL_A, _DP_TP_CTL_B)` — intel_display_regs.h:2305.
pub const fn dp_tp_ctl(port: Ddi) -> u32 {
    DP_TP_CTL_A + (port as u32) * (DP_TP_CTL_B - DP_TP_CTL_A)
}
/// `DP_TP_STATUS(port)` — intel_display_regs.h:2332.
pub const fn dp_tp_status(port: Ddi) -> u32 {
    DP_TP_STATUS_A + (port as u32) * (DP_TP_STATUS_B - DP_TP_STATUS_A)
}
pub const DDI_BUF_TRANS_A: u32 = 0x64E00; // intel_display_regs.h:2379
pub const DDI_BUF_TRANS_B: u32 = 0x64E60; // intel_display_regs.h:2380
/// `DDI_BUF_BALANCE_LEG_ENABLE` (bit 31 of trans1) — intel_display_regs.h:2382.
pub const DDI_BUF_BALANCE_LEG_ENABLE: u32 = 1 << 31; // intel_display_regs.h:2382
/// `DDI_BUF_TRANS_LO(port, i)` — intel_display_regs.h:2381.
pub const fn ddi_buf_trans_lo(port: Ddi, i: u32) -> u32 {
    DDI_BUF_TRANS_A + (port as u32) * (DDI_BUF_TRANS_B - DDI_BUF_TRANS_A) + i * 8
}
/// `DDI_BUF_TRANS_HI(port, i)` — intel_display_regs.h:2383.
pub const fn ddi_buf_trans_hi(port: Ddi, i: u32) -> u32 {
    ddi_buf_trans_lo(port, i) + 4
}
/// `DISPIO_CR_TX_BMU_CR0` — intel_display_regs.h:45.
pub const DISPIO_CR_TX_BMU_CR0: u32 = 0x6C00C; // intel_display_regs.h:45
/// `BALANCE_LEG_SHIFT(port)` — intel_display_regs.h:47.
pub const fn balance_leg_shift(port: Ddi) -> u32 {
    8 + 3 * (port as u32)
}
/// `BALANCE_LEG_MASK(port)` — intel_display_regs.h:48.
pub const fn balance_leg_mask(port: Ddi) -> u32 {
    7 << (8 + 3 * (port as u32))
}
/// `BALANCE_LEG_DISABLE(port)` — intel_display_regs.h:51.
pub const fn balance_leg_disable(port: Ddi) -> u32 {
    1 << (23 + (port as u32))
}
/// `DDI_BUF_TRANS_SELECT(n)` = `REG_FIELD_PREP(DDI_BUF_EMP_MASK, n)` — intel_display_regs.h:2353.
pub const fn ddi_buf_trans_select(n: u32) -> u32 {
    (n << DDI_BUF_EMP_MASK.trailing_zeros()) & DDI_BUF_EMP_MASK
}
/// `DDI_PORT_WIDTH(width)` = `REG_FIELD_PREP(DDI_PORT_WIDTH_MASK, DDI_PORT_WIDTH_ENCODE(width))`; the encode maps 3 lanes to
/// 4 and otherwise `width - 1` — intel_display_regs.h:2368-2371.
pub const fn ddi_port_width(width: u32) -> u32 {
    let enc = if width == 3 { 4 } else { width - 1 };
    (enc << DDI_PORT_WIDTH_MASK.trailing_zeros()) & DDI_PORT_WIDTH_MASK
}
/// `PCH_RAWCLK_FREQ` — intel_display_regs.h:1807.
pub const PCH_RAWCLK_FREQ: u32 = 0xc6204; // intel_display_regs.h:1807
/// `RAWCLK_FREQ_MASK` — intel_display_regs.h:1812.
pub const RAWCLK_FREQ_MASK: u32 = 0x3ff; // intel_display_regs.h:1812
