// SPDX-License-Identifier: GPL-2.0-only
//! Every constant and address function in `regs_write` against the value GCC computes from the Linux headers
//! (skl_universal_plane_regs.h, intel_cursor_regs.h, skl_watermark_regs.h, intel_display_regs.h, i915_reg.h at v7.0-rc6; the
//! `REG_BIT` / `REG_GENMASK` / `REG_FIELD_PREP` / `_PICK_EVEN` stubs are the kernel's own definitions). Expected side literal.
use i915_kbl_display_core::regs_write::*;
use i915_kbl_display_core::types::{Pipe, Transcoder};

#[test]
fn plane_ctl_fields() {
    assert_eq!(PLANE_CTL_PIPE_GAMMA_ENABLE, 0x4000_0000);
    assert_eq!(PLANE_CTL_YUV_RANGE_CORRECTION_DISABLE, 0x1000_0000);
    assert_eq!(PLANE_CTL_PIPE_CSC_ENABLE, 0x0080_0000);
    assert_eq!(PLANE_CTL_KEY_ENABLE_MASK, 0x0060_0000);
    assert_eq!(PLANE_CTL_KEY_ENABLE_SOURCE, 0x0020_0000);
    assert_eq!(PLANE_CTL_KEY_ENABLE_DESTINATION, 0x0040_0000);
    assert_eq!(PLANE_CTL_YUV_TO_RGB_CSC_FORMAT_BT709, 0x0004_0000);
    assert_eq!(PLANE_CTL_PLANE_GAMMA_DISABLE, 0x2000);
    assert_eq!(PLANE_CTL_ALPHA_SW_PREMULTIPLY, 0x20);
    assert_eq!(PLANE_CTL_ALPHA_HW_PREMULTIPLY, 0x30);
    assert_eq!(PLANE_SURF_DECRYPT, 0x4);
    assert_eq!(PLANE_KEYMSK_ALPHA_ENABLE, 0x8000_0000);
    assert_eq!(0xffu32 << PLANE_KEYMAX_ALPHA_SHIFT, 0xff00_0000);
    assert_eq!(PLANE_AUX_STRIDE_MASK, 0xfff);
    // zero-valued member pinned by behaviour (PORT_CONVENTIONS 3a.2): ALPHA_DISABLE selects neither premultiply mode.
    assert_ne!(PLANE_CTL_ALPHA_DISABLE, PLANE_CTL_ALPHA_SW_PREMULTIPLY);
    assert_ne!(PLANE_CTL_ALPHA_DISABLE, PLANE_CTL_ALPHA_HW_PREMULTIPLY);
}

#[test]
fn plane_register_bases_and_fields() {
    assert_eq!(PLANE_KEYVAL_1_A, 0x70194);
    assert_eq!(PLANE_KEYMSK_1_A, 0x70198);
    assert_eq!(PLANE_KEYMAX_1_A, 0x701a0);
    assert_eq!(PLANE_SURFLIVE_1_A, 0x701ac);
    assert_eq!(PLANE_AUX_DIST_1_A, 0x701c0);
    assert_eq!(PLANE_AUX_OFFSET_1_A, 0x701c4);
    assert_eq!(PLANE_WM_1_A_0, 0x70240);
    assert_eq!(PLANE_WM_TRANS_1_A, 0x70268);
    assert_eq!(PLANE_NV12_BUF_CFG_1_A, 0x70278);
    assert_eq!(PLANE_BUF_CFG_1_A, 0x7027c);
    assert_eq!(PLANE_WM_EN, 0x8000_0000);
    assert_eq!(PLANE_WM_IGNORE_LINES, 0x4000_0000);
    assert_eq!(PLANE_WM_LINES_MASK, 0x07ff_c000);
    assert_eq!(PLANE_WM_BLOCKS_MASK, 0x1fff);
    assert_eq!(PLANE_BUF_END_MASK, 0x1fff_0000);
    assert_eq!(PLANE_BUF_START_MASK, 0x1fff);
    // PLANE_WM(pipe, plane, level): level is a dword index.
    assert_eq!(plane_wm(Pipe::A, 0, 0), 0x70240);
    assert_eq!(plane_wm(Pipe::B, 0, 0), 0x71240);
    assert_eq!(plane_wm(Pipe::A, 1, 2), 0x70348);
    assert_eq!(plane_wm(Pipe::C, 2, 7), 0x7245c);
}

#[test]
fn cursor_block() {
    assert_eq!((CURACNTR, CURABASE, CURAPOS), (0x70080, 0x70084, 0x70088));
    assert_eq!((cur_wm(Pipe::A, 0), cur_wm(Pipe::A, 7), cur_wm(Pipe::B, 3)), (0x70140, 0x7015c, 0x7114c));
    assert_eq!((cur_wm_trans(Pipe::A), cur_wm_trans(Pipe::B)), (0x70168, 0x71168));
    assert_eq!((cur_buf_cfg(Pipe::A), cur_buf_cfg(Pipe::B), cur_buf_cfg(Pipe::C)), (0x7017c, 0x7117c, 0x7217c));
    assert_eq!((curcntr(Pipe::B), curbase(Pipe::B), curpos(Pipe::B)), (0x71080, 0x71084, 0x71088));
    assert_eq!(CUR_WM_EN, 0x8000_0000);
    assert_eq!(CUR_WM_IGNORE_LINES, 0x4000_0000);
    assert_eq!(CUR_WM_LINES_MASK, 0x07ff_c000);
    assert_eq!(CUR_WM_BLOCKS_MASK, 0xfff);
    assert_eq!(CUR_BUF_END_MASK, 0x0fff_0000);
    assert_eq!(CUR_BUF_START_MASK, 0xfff);
}

#[test]
fn pipe_transcoder_and_dbuf() {
    assert_eq!(PIPE_MISC_A, 0x70030);
    assert_eq!(pipe_misc(Pipe::B), 0x71030);
    assert_eq!(PIPE_MISC_BPC_MASK, 0xe0);
    assert_eq!((PIPE_MISC_BPC_8, PIPE_MISC_BPC_10, PIPE_MISC_BPC_6), (0, 0x20, 0x40));
    assert_ne!(PIPE_MISC_BPC_8, PIPE_MISC_BPC_10);
    assert_eq!((wm_linetime(Pipe::A), wm_linetime(Pipe::B)), (0x45270, 0x45274));
    assert_eq!(HSW_LINETIME_MASK, 0x1ff);
    assert_eq!(0x1ffu32 << HSW_IPS_LINETIME_SHIFT, 0x01ff_0000);
    assert_eq!(
        (chicken_trans(Transcoder::A), chicken_trans(Transcoder::B), chicken_trans(Transcoder::C), chicken_trans(Transcoder::Edp)),
        (0x420c0, 0x420c4, 0x420c8, 0x420cc)
    );
    assert_eq!(HSW_FRAME_START_DELAY_MASK, 0x1800_0000);
    assert_eq!(3u32 << HSW_FRAME_START_DELAY_SHIFT, HSW_FRAME_START_DELAY_MASK);
    assert_eq!(TRANSCONF_STATE_ENABLE, 0x4000_0000);
    assert_eq!(TRANSCONF_INTERLACE_IF_ID_ILK, 0x0060_0000);
    assert_ne!(TRANSCONF_INTERLACE_PF_PD_ILK, TRANSCONF_INTERLACE_IF_ID_ILK);
    assert_eq!(TU_SIZE_SHIFT, 25);
    assert_eq!(DATA_LINK_N_MAX, 0x80_0000);
    assert_eq!(DBUF_CTL_S0, 0x45008);
    assert_eq!(DBUF_POWER_REQUEST, 0x8000_0000);
    assert_eq!(DBUF_POWER_STATE, 0x4000_0000);
}

#[test]
fn memory_latency_mailbox() {
    assert_eq!(GEN9_PCODE_READ_MEM_LATENCY, 0x6);
    assert_eq!(GEN9_MEM_LATENCY_LEVEL_0_4_MASK, 0xff);
    assert_eq!(GEN9_MEM_LATENCY_LEVEL_1_5_MASK, 0xff00);
    assert_eq!(GEN9_MEM_LATENCY_LEVEL_2_6_MASK, 0xff_0000);
    assert_eq!(GEN9_MEM_LATENCY_LEVEL_3_7_MASK, 0xff00_0000);
}
