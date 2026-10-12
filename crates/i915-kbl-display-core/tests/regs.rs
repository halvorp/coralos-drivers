// SPDX-License-Identifier: GPL-2.0-only
//! Pin every register constant to the LITERAL Linux value (written out from the header, not from `regs.rs`), and the
//! address arithmetic to literal addresses. Wrong-value coverage: a mutated constant fails here by value; the
//! decode tests in the other files fail it by behaviour as well.
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::types::*;

macro_rules! pin {
    ($($c:ident == $v:expr),* $(,)?) => { $( assert_eq!($c, $v, stringify!($c)); )* };
}

#[test]
fn base_offsets_are_linux_literals() {
    // intel_display_device.c:79-101
    pin!(PIPE_A_OFFSET == 0x70000, PIPE_B_OFFSET == 0x71000, PIPE_C_OFFSET == 0x72000, PIPE_EDP_OFFSET == 0x7f000);
    pin!(TRANSCODER_A_OFFSET == 0x60000, TRANSCODER_B_OFFSET == 0x61000, TRANSCODER_C_OFFSET == 0x62000);
    pin!(TRANSCODER_EDP_OFFSET == 0x6f000);
}

#[test]
fn transcoder_timing_registers() {
    // intel_display_regs.h:346-410
    pin!(TRANS_HTOTAL_A == 0x60000, TRANS_HBLANK_A == 0x60004, TRANS_HSYNC_A == 0x60008);
    pin!(TRANS_VTOTAL_A == 0x6000c, TRANS_VBLANK_A == 0x60010, TRANS_VSYNC_A == 0x60014);
    pin!(PIPEASRC == 0x6001c, TRANS_MULT_A == 0x6002c);
    pin!(HTOTAL_MASK == 0xffff0000, HACTIVE_MASK == 0xffff, HBLANK_END_MASK == 0xffff0000, HBLANK_START_MASK == 0xffff);
    pin!(HSYNC_END_MASK == 0xffff0000, HSYNC_START_MASK == 0xffff, VTOTAL_MASK == 0xffff0000, VACTIVE_MASK == 0xffff);
    pin!(VBLANK_END_MASK == 0xffff0000, VBLANK_START_MASK == 0xffff, VSYNC_END_MASK == 0xffff0000, VSYNC_START_MASK == 0xffff);
    pin!(PIPESRC_WIDTH_MASK == 0xffff0000, PIPESRC_HEIGHT_MASK == 0xffff);
    // COMPOSITE: the two halves of each timing register tile the word exactly
    assert_eq!(HTOTAL_MASK | HACTIVE_MASK, u32::MAX);
    assert_eq!(HTOTAL_MASK & HACTIVE_MASK, 0);
    assert_eq!(PIPESRC_WIDTH_MASK | PIPESRC_HEIGHT_MASK, u32::MAX);
}

#[test]
fn transconf_and_mn_registers() {
    pin!(TRANSACONF == 0x70008, TRANSCONF_ENABLE == 0x8000_0000, TRANSCONF_INTERLACE_MASK_HSW == 0x0060_0000);
    pin!(PIPEA_DATA_M1 == 0x60030, PIPEA_DATA_N1 == 0x60034, PIPEA_LINK_M1 == 0x60040, PIPEA_LINK_N1 == 0x60044);
    pin!(TU_SIZE_MASK == 0x7e00_0000, DATA_LINK_M_N_MASK == 0x00ff_ffff);
    assert_eq!(TU_SIZE_MASK & DATA_LINK_M_N_MASK, 0);
}

#[test]
fn trans_ddi_func_ctl_fields() {
    // intel_display_regs.h:2228-2273
    pin!(TRANS_DDI_FUNC_CTL_A == 0x60400, TRANS_DDI_FUNC_ENABLE == 0x8000_0000, TRANS_DDI_PORT_SHIFT == 28);
    pin!(TRANS_DDI_PORT_MASK == 0x7000_0000, TRANS_DDI_MODE_SELECT_MASK == 0x0700_0000);
    // FAMILY: mode select values 0..4 (5..7 undefined)
    pin!(TRANS_DDI_MODE_SELECT_HDMI == 0x0000_0000, TRANS_DDI_MODE_SELECT_DVI == 0x0100_0000);
    pin!(TRANS_DDI_MODE_SELECT_DP_SST == 0x0200_0000, TRANS_DDI_MODE_SELECT_DP_MST == 0x0300_0000);
    pin!(TRANS_DDI_MODE_SELECT_FDI_OR_128B132B == 0x0400_0000);
    // FAMILY: bits per colour
    pin!(TRANS_DDI_BPC_MASK == 0x0070_0000, TRANS_DDI_BPC_8 == 0, TRANS_DDI_BPC_10 == 0x0010_0000);
    pin!(TRANS_DDI_BPC_6 == 0x0020_0000, TRANS_DDI_BPC_12 == 0x0030_0000);
    pin!(TRANS_DDI_PVSYNC == 0x0002_0000, TRANS_DDI_PHSYNC == 0x0001_0000);
    // FAMILY: eDP input select
    pin!(TRANS_DDI_EDP_INPUT_MASK == 0x7000, TRANS_DDI_EDP_INPUT_A_ON == 0, TRANS_DDI_EDP_INPUT_A_ONOFF == 0x4000);
    pin!(TRANS_DDI_EDP_INPUT_B_ONOFF == 0x5000, TRANS_DDI_EDP_INPUT_C_ONOFF == 0x6000, TRANS_DDI_EDP_INPUT_D_ONOFF == 0x7000);
    pin!(TRANS_DDI_DP_VC_PAYLOAD_ALLOC == 0x100);
    // the four single-purpose fields do not overlap
    let all = [TRANS_DDI_FUNC_ENABLE, TRANS_DDI_PORT_MASK, TRANS_DDI_MODE_SELECT_MASK, TRANS_DDI_BPC_MASK, TRANS_DDI_PVSYNC,
        TRANS_DDI_PHSYNC, TRANS_DDI_EDP_INPUT_MASK, DDI_PORT_WIDTH_MASK];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_eq!(a & b, 0, "overlap {a:#x} {b:#x}");
        }
    }
}

#[test]
fn ddi_buf_ctl_fields() {
    pin!(DDI_BUF_CTL_A == 0x64000, DDI_BUF_CTL_B == 0x64100, DDI_BUF_CTL_ENABLE == 0x8000_0000);
    pin!(DDI_BUF_EMP_MASK == 0x0f00_0000, DDI_BUF_PORT_REVERSAL == 0x1_0000, DDI_BUF_IS_IDLE == 0x80);
    pin!(DDI_A_4_LANES == 0x10, DDI_PORT_WIDTH_MASK == 0xe, DDI_PORT_WIDTH_SHIFT == 1, DDI_INIT_DISPLAY_DETECTED == 1);
}

#[test]
fn dpll_registers_and_values() {
    pin!(DPLL_CTRL1 == 0x6c058, DPLL_CTRL2 == 0x6c05c, DPLL_STATUS == 0x6c060);
    // FAMILY: link-rate encodings, all six
    pin!(DPLL_CTRL1_LINK_RATE_2700 == 0, DPLL_CTRL1_LINK_RATE_1350 == 1, DPLL_CTRL1_LINK_RATE_810 == 2);
    pin!(DPLL_CTRL1_LINK_RATE_1620 == 3, DPLL_CTRL1_LINK_RATE_1080 == 4, DPLL_CTRL1_LINK_RATE_2160 == 5);
    pin!(DPLL1_CFGCR1 == 0x6c040, DPLL2_CFGCR1 == 0x6c048, DPLL1_CFGCR2 == 0x6c044, DPLL2_CFGCR2 == 0x6c04c);
    pin!(DPLL_CFGCR1_DCO_FRACTION_MASK == 0x00ff_fe00, DPLL_CFGCR1_DCO_INTEGER_MASK == 0x1ff);
    pin!(DPLL_CFGCR2_QDIV_RATIO_MASK == 0xff00, DPLL_CFGCR2_QDIV_MODE_BIT == 0x80, DPLL_CFGCR2_KDIV_MASK == 0x60);
    pin!(DPLL_CFGCR2_KDIV_5 == 0, DPLL_CFGCR2_KDIV_2 == 0x20, DPLL_CFGCR2_KDIV_3 == 0x40, DPLL_CFGCR2_KDIV_1 == 0x60);
    pin!(DPLL_CFGCR2_PDIV_MASK == 0x1c, DPLL_CFGCR2_PDIV_1 == 0, DPLL_CFGCR2_PDIV_2 == 4, DPLL_CFGCR2_PDIV_3 == 8);
    pin!(DPLL_CFGCR2_PDIV_7 == 0x10, DPLL_CFGCR2_PDIV_7_INVALID == 0x14);
    pin!(LCPLL1_CTL == 0x46010, LCPLL2_CTL == 0x46014, LCPLL_PLL_ENABLE == 0x8000_0000, LCPLL_PLL_LOCK == 0x4000_0000);
    pin!(WRPLL_CTL1 == 0x46040, WRPLL_CTL2 == 0x46060);
}

#[test]
fn cdclk_registers() {
    pin!(CDCLK_CTL == 0x46000, CDCLK_FREQ_SEL_MASK == 0x0c00_0000, CDCLK_FREQ_DECIMAL_MASK == 0x7ff);
    pin!(CDCLK_FREQ_450_432 == 0, CDCLK_FREQ_540 == 0x0400_0000, CDCLK_FREQ_337_308 == 0x0800_0000);
    pin!(CDCLK_FREQ_675_617 == 0x0c00_0000);
}

#[test]
fn power_registers() {
    pin!(HSW_PWR_WELL_CTL1 == 0x45400, HSW_PWR_WELL_CTL2 == 0x45404, SKL_FUSE_STATUS == 0x42000);
    pin!(SKL_FUSE_DOWNLOAD_STATUS == 0x8000_0000, DC_STATE_EN == 0x45504, DC_STATE_EN_UPTO_DC5 == 1);
    pin!(DC_STATE_EN_DC9 == 8, DC_STATE_EN_UPTO_DC6 == 2, DC_STATE_EN_UPTO_DC5_DC6_MASK == 3);
    // FAMILY: the seven well indices
    pin!(SKL_PW_CTL_IDX_PW_2 == 15, SKL_PW_CTL_IDX_PW_1 == 14, SKL_PW_CTL_IDX_DDI_D == 4, SKL_PW_CTL_IDX_DDI_C == 3);
    pin!(SKL_PW_CTL_IDX_DDI_B == 2, SKL_PW_CTL_IDX_DDI_A_E == 1, SKL_PW_CTL_IDX_MISC_IO == 0);
}

#[test]
fn plane_registers() {
    pin!(PLANE_CTL_1_A == 0x70180, PLANE_STRIDE_1_A == 0x70188, PLANE_POS_1_A == 0x7018c, PLANE_SIZE_1_A == 0x70190);
    pin!(PLANE_SURF_1_A == 0x7019c, PLANE_OFFSET_1_A == 0x701a4, PLANE_STEP == 0x100, PIPE_STEP == 0x1000);
    pin!(PLANE_CTL_ENABLE == 0x8000_0000, PLANE_CTL_FORMAT_MASK_SKL == 0x0f00_0000);
    // FAMILY: pixel formats (skl_universal_plane_regs.h:50-60)
    pin!(PLANE_CTL_FORMAT_YUV422 == 0, PLANE_CTL_FORMAT_NV12 == 0x0100_0000, PLANE_CTL_FORMAT_XRGB_2101010 == 0x0200_0000);
    pin!(PLANE_CTL_FORMAT_P010 == 0x0300_0000, PLANE_CTL_FORMAT_XRGB_8888 == 0x0400_0000, PLANE_CTL_FORMAT_P012 == 0x0500_0000);
    pin!(PLANE_CTL_FORMAT_XRGB_16161616F == 0x0600_0000, PLANE_CTL_FORMAT_P016 == 0x0700_0000, PLANE_CTL_FORMAT_XYUV == 0x0800_0000);
    pin!(PLANE_CTL_FORMAT_INDEXED == 0x0c00_0000, PLANE_CTL_FORMAT_RGB_565 == 0x0e00_0000);
    pin!(PLANE_CTL_ORDER_RGBX == 0x10_0000, PLANE_CTL_RENDER_DECOMPRESSION_ENABLE == 0x8000, PLANE_CTL_ALPHA_MASK == 0x30);
    // FAMILY: tiling and rotation
    pin!(PLANE_CTL_TILED_MASK == 0x1c00, PLANE_CTL_TILED_LINEAR == 0, PLANE_CTL_TILED_X == 0x400);
    pin!(PLANE_CTL_TILED_Y == 0x1000, PLANE_CTL_TILED_YF == 0x1400);
    pin!(PLANE_CTL_ROTATE_MASK == 3, PLANE_CTL_ROTATE_0 == 0, PLANE_CTL_ROTATE_90 == 1, PLANE_CTL_ROTATE_180 == 2);
    pin!(PLANE_CTL_ROTATE_270 == 3);
    pin!(PLANE_STRIDE_MASK == 0xfff, PLANE_POS_Y_MASK == 0xffff0000, PLANE_POS_X_MASK == 0xffff);
    pin!(PLANE_HEIGHT_MASK == 0xffff0000, PLANE_WIDTH_MASK == 0xffff, PLANE_SURF_ADDR_MASK == 0xffff_f000);
    pin!(PLANE_OFFSET_Y_MASK == 0xffff0000, PLANE_OFFSET_X_MASK == 0xffff);
}

#[test]
fn scaler_registers() {
    pin!(PS_1A_CTRL == 0x68180, PS_2A_CTRL == 0x68280, PS_SCALER_EN == 0x8000_0000, PS_BINDING_MASK == 0x0e00_0000);
    pin!(PS_BINDING_PIPE == 0, PS_WIN_POS_1A == 0x68170, PS_WIN_SZ_1A == 0x68174, PS_ID_STEP == 0x100, PS_PIPE_STEP == 0x800);
    pin!(PS_WIN_XPOS_MASK == 0xffff0000, PS_WIN_YPOS_MASK == 0xffff, PS_WIN_XSIZE_MASK == 0xffff0000, PS_WIN_YSIZE_MASK == 0xffff);
}

#[test]
fn transcoder_register_addresses() {
    // _MMIO_TRANS2 with hsw trans_offsets: A 0x60000, B 0x61000, C 0x62000, EDP 0x6f000 (intel_display_device.c:149-154)
    let cases: [(Transcoder, u32); 4] = [
        (Transcoder::A, 0x60000),
        (Transcoder::B, 0x61000),
        (Transcoder::C, 0x62000),
        (Transcoder::Edp, 0x6f000),
    ];
    for (t, base) in cases {
        assert_eq!(trans_reg(t, 0x60000), base, "HTOTAL {t:?}");
        assert_eq!(trans_reg(t, 0x6001c), base + 0x1c, "PIPESRC {t:?}");
        assert_eq!(trans_ddi_func_ctl(t), base + 0x400, "TRANS_DDI_FUNC_CTL {t:?}");
    }
    // TRANS_DDI_FUNC_CTL addresses in Linux's own list: _A 0x60400 _B 0x61400 _C 0x62400 _EDP 0x6F400 (intel_display_regs.h:2228-2232)
    assert_eq!(trans_ddi_func_ctl(Transcoder::A), 0x60400);
    assert_eq!(trans_ddi_func_ctl(Transcoder::B), 0x61400);
    assert_eq!(trans_ddi_func_ctl(Transcoder::C), 0x62400);
    assert_eq!(trans_ddi_func_ctl(Transcoder::Edp), 0x6f400);
    // TRANSCONF lives in the pipe block: A 0x70008, B 0x71008, C 0x72008, EDP 0x7f008
    assert_eq!(pipe_reg(Transcoder::A, 0x70008), 0x70008);
    assert_eq!(pipe_reg(Transcoder::B, 0x70008), 0x71008);
    assert_eq!(pipe_reg(Transcoder::C, 0x70008), 0x72008);
    assert_eq!(pipe_reg(Transcoder::Edp, 0x70008), 0x7f008);
}

#[test]
fn port_plane_scaler_addresses() {
    // DDI_BUF_CTL(port) = 0x64000 + port * 0x100
    for (d, a) in [(Ddi::A, 0x64000), (Ddi::B, 0x64100), (Ddi::C, 0x64200), (Ddi::D, 0x64300), (Ddi::E, 0x64400)] {
        assert_eq!(ddi_buf_ctl(d), a, "{d:?}");
    }
    // PLANE_CTL(pipe, plane): _PLANE_CTL_1_A 0x70180, _2_A 0x70280, _1_B 0x71180; pipe C extrapolates to 0x72180
    assert_eq!(plane_reg(0x70180, Pipe::A, 0), 0x70180);
    assert_eq!(plane_reg(0x70180, Pipe::A, 1), 0x70280);
    assert_eq!(plane_reg(0x70180, Pipe::B, 0), 0x71180);
    assert_eq!(plane_reg(0x70180, Pipe::C, 0), 0x72180);
    assert_eq!(plane_reg(0x70180, Pipe::C, 2), 0x72380);
    assert_eq!(plane_reg(0x7019c, Pipe::B, 1), 0x7129c); // _PLANE_SURF_2_B
    // SKL_PS_CTRL(pipe, id): _PS_1A 0x68180, _2A 0x68280, _1B 0x68980, _1C 0x69180
    assert_eq!(scaler_reg(0x68180, Pipe::A, 0), 0x68180);
    assert_eq!(scaler_reg(0x68180, Pipe::A, 1), 0x68280);
    assert_eq!(scaler_reg(0x68180, Pipe::B, 0), 0x68980);
    assert_eq!(scaler_reg(0x68180, Pipe::C, 0), 0x69180);
    assert_eq!(scaler_reg(0x68170, Pipe::B, 1), 0x68a70); // _PS_WIN_POS_2B
}

#[test]
fn enum_values_are_linux_enum_values() {
    // enum pipe / transcoder / port (intel_display_limits.h:17-19, 34-43, 95-99)
    assert_eq!((Pipe::A as u8, Pipe::B as u8, Pipe::C as u8), (0, 1, 2));
    assert_eq!((Transcoder::A as u8, Transcoder::B as u8, Transcoder::C as u8, Transcoder::Edp as u8), (0, 1, 2, 4));
    assert_eq!(Transcoder::Edp.mask_bit(), 0x10);
    assert_eq!((Ddi::A as u8, Ddi::B as u8, Ddi::C as u8, Ddi::D as u8, Ddi::E as u8), (0, 1, 2, 3, 4));
}

/// Registers moved in from i915-display-clock-core (sentinel audit 2026-10-12), pinned to the Linux literals.
#[test]
fn clock_side_registers_have_linux_values() {
    use i915_kbl_display_core::regs::*;
    assert_eq!(DPLL_CFGCR1_FREQ_ENABLE, 0x8000_0000); // intel_display_regs.h:2587
    assert_eq!(DPLL_CFGCR2_CENTRAL_FREQ_MASK, 3); // :2612
    assert_eq!(CDCLK_DIVMUX_CD_OVERRIDE, 0x8_0000); // :2533
    assert_eq!(SKL_DFSM, 0x51000); // :1539
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_MASK, 0x0180_0000); // :1542
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_675, 0); // :1543
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_540, 0x0080_0000); // :1544
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_450, 0x0100_0000); // :1545
    assert_eq!(SKL_DFSM_CDCLK_LIMIT_337_5, 0x0180_0000); // :1546
    assert_eq!((HSW_PWR_WELL_CTL3, HSW_PWR_WELL_CTL4), (0x45408, 0x4540c)); // :2127-2128
    assert_eq!((SKL_PG0, SKL_PG1, SKL_PG2), (0, 1, 2)); // :2216-2218
    assert_eq!(SWF_ILK_18, 0x4f060); // :1004 (SWF_ILK(0x18) = 0x4f000 + 0x18 * 4)
}
