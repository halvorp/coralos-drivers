// SPDX-License-Identifier: GPL-2.0-only
//! The registers and fields the WRITE planners need and the read-only readout (`regs`) never touched: plane keying / aux / watermark /
//! DDB, the cursor watermark block, pipe misc, line-time watermark, frame-start delay, DBUF power. Each line cites the Linux
//! FILE:LINE it came from (v7.0-rc6; skl_universal_plane_regs.h, intel_cursor_regs.h, skl_watermark_regs.h and intel_display_regs.h
//! are all under drivers/gpu/drm/i915/display/). Offsets are absolute BAR0 offsets (Kaby Lake's display MMIO base is 0, see `regs`).
//! Copyright (c) Intel Corporation.
use crate::regs::{pipe_reg, plane_reg};
use crate::types::{Pipe, Transcoder};

// ---- PLANE_CTL fields not decoded by the readout (skl_universal_plane_regs.h) ----
pub const PLANE_CTL_PIPE_GAMMA_ENABLE: u32 = 1 << 30; // skl_universal_plane_regs.h:41
pub const PLANE_CTL_YUV_RANGE_CORRECTION_DISABLE: u32 = 1 << 28; // skl_universal_plane_regs.h:42
pub const PLANE_CTL_PIPE_CSC_ENABLE: u32 = 1 << 23; // skl_universal_plane_regs.h:67
pub const PLANE_CTL_KEY_ENABLE_MASK: u32 = 3 << 21; // skl_universal_plane_regs.h:68
pub const PLANE_CTL_KEY_ENABLE_SOURCE: u32 = 1 << 21; // skl_universal_plane_regs.h:69
pub const PLANE_CTL_KEY_ENABLE_DESTINATION: u32 = 2 << 21; // skl_universal_plane_regs.h:70
pub const PLANE_CTL_YUV_TO_RGB_CSC_FORMAT_BT709: u32 = 1 << 18; // skl_universal_plane_regs.h:73
pub const PLANE_CTL_PLANE_GAMMA_DISABLE: u32 = 1 << 13; // skl_universal_plane_regs.h:82
pub const PLANE_CTL_ALPHA_DISABLE: u32 = 0 << 4; // skl_universal_plane_regs.h:93
pub const PLANE_CTL_ALPHA_SW_PREMULTIPLY: u32 = 2 << 4; // skl_universal_plane_regs.h:94
pub const PLANE_CTL_ALPHA_HW_PREMULTIPLY: u32 = 3 << 4; // skl_universal_plane_regs.h:95
pub const PLANE_SURF_DECRYPT: u32 = 1 << 2; // skl_universal_plane_regs.h:161
pub const PLANE_KEYMSK_ALPHA_ENABLE: u32 = 1 << 31; // skl_universal_plane_regs.h:151
pub const PLANE_KEYMAX_ALPHA_SHIFT: u32 = 24; // skl_universal_plane_regs.h:171
pub const PLANE_AUX_STRIDE_MASK: u32 = 0xfff; // skl_universal_plane_regs.h:210

// ---- plane register bases (plane 1, pipe A; `plane_reg` adds the pipe / plane steps) ----
pub const PLANE_KEYVAL_1_A: u32 = 0x70194; // skl_universal_plane_regs.h:136
pub const PLANE_KEYMSK_1_A: u32 = 0x70198; // skl_universal_plane_regs.h:144
pub const PLANE_KEYMAX_1_A: u32 = 0x701a0; // skl_universal_plane_regs.h:164
pub const PLANE_SURFLIVE_1_A: u32 = 0x701ac; // skl_universal_plane_regs.h:186
pub const PLANE_AUX_DIST_1_A: u32 = 0x701c0; // skl_universal_plane_regs.h:202
pub const PLANE_AUX_OFFSET_1_A: u32 = 0x701c4; // skl_universal_plane_regs.h:213
pub const PLANE_WM_1_A_0: u32 = 0x70240; // skl_universal_plane_regs.h:431
pub const PLANE_WM_TRANS_1_A: u32 = 0x70268; // skl_universal_plane_regs.h:460
pub const PLANE_NV12_BUF_CFG_1_A: u32 = 0x70278; // skl_universal_plane_regs.h:477
pub const PLANE_BUF_CFG_1_A: u32 = 0x7027c; // skl_universal_plane_regs.h:485

// ---- PLANE_WM / PLANE_BUF_CFG fields ----
pub const PLANE_WM_EN: u32 = 1 << 31; // skl_universal_plane_regs.h:438
pub const PLANE_WM_IGNORE_LINES: u32 = 1 << 30; // skl_universal_plane_regs.h:439
pub const PLANE_WM_LINES_SHIFT: u32 = 14; // skl_universal_plane_regs.h:441
pub const PLANE_WM_LINES_MASK: u32 = 0x1fff << 14; // skl_universal_plane_regs.h:441
pub const PLANE_WM_BLOCKS_MASK: u32 = 0x1fff; // skl_universal_plane_regs.h:442
pub const PLANE_BUF_END_SHIFT: u32 = 16; // skl_universal_plane_regs.h:494
pub const PLANE_BUF_END_MASK: u32 = 0x1fff << 16; // skl_universal_plane_regs.h:494
pub const PLANE_BUF_START_MASK: u32 = 0x1fff; // skl_universal_plane_regs.h:496

/// `PLANE_WM(pipe, plane, level)` = `_MMIO_SKL_PLANE_DW(.., level, ..)`: the level is a dword index (skl_universal_plane_regs.h:435).
pub const fn plane_wm(pipe: Pipe, plane_id: u32, level: u32) -> u32 {
    plane_reg(PLANE_WM_1_A_0, pipe, plane_id) + level * 4
}

// ---- cursor (intel_cursor_regs.h) ----
pub const CURACNTR: u32 = 0x70080; // intel_cursor_regs.h:11
pub const CURABASE: u32 = 0x70084; // intel_cursor_regs.h:43
pub const CURAPOS: u32 = 0x70088; // intel_cursor_regs.h:46
pub const CUR_WM_A_0: u32 = 0x70140; // intel_cursor_regs.h:78
pub const CUR_WM_TRANS_A: u32 = 0x70168; // intel_cursor_regs.h:94
pub const CUR_BUF_CFG_A: u32 = 0x7017c; // intel_cursor_regs.h:98
pub const CUR_WM_EN: u32 = 1 << 31; // intel_cursor_regs.h:81
pub const CUR_WM_IGNORE_LINES: u32 = 1 << 30; // intel_cursor_regs.h:82
pub const CUR_WM_LINES_SHIFT: u32 = 14; // intel_cursor_regs.h:83
pub const CUR_WM_LINES_MASK: u32 = 0x1fff << 14; // intel_cursor_regs.h:83
pub const CUR_WM_BLOCKS_MASK: u32 = 0xfff; // intel_cursor_regs.h:84
pub const CUR_BUF_END_SHIFT: u32 = 16; // intel_cursor_regs.h:102
pub const CUR_BUF_END_MASK: u32 = 0xfff << 16; // intel_cursor_regs.h:102
pub const CUR_BUF_START_MASK: u32 = 0xfff; // intel_cursor_regs.h:104

/// `CUR_WM(pipe, level)` — intel_cursor_regs.h:80.
pub const fn cur_wm(pipe: Pipe, level: u32) -> u32 {
    CUR_WM_A_0 + (pipe as u32) * (crate::regs::PIPE_STEP) + level * 4
}
/// `CUR_WM_TRANS(pipe)` — intel_cursor_regs.h:96.
pub const fn cur_wm_trans(pipe: Pipe) -> u32 {
    CUR_WM_TRANS_A + (pipe as u32) * crate::regs::PIPE_STEP
}
/// `CUR_BUF_CFG(pipe)` — intel_cursor_regs.h:100.
pub const fn cur_buf_cfg(pipe: Pipe) -> u32 {
    CUR_BUF_CFG_A + (pipe as u32) * crate::regs::PIPE_STEP
}
/// `CURBASE(pipe)` / `CURPOS(pipe)` / `CURCNTR(pipe)` — intel_cursor_regs.h:12,44,47 (`_MMIO_CURSOR2`: the pipe block offset).
pub const fn curbase(pipe: Pipe) -> u32 {
    CURABASE + (pipe as u32) * crate::regs::PIPE_STEP
}
pub const fn curpos(pipe: Pipe) -> u32 {
    CURAPOS + (pipe as u32) * crate::regs::PIPE_STEP
}
pub const fn curcntr(pipe: Pipe) -> u32 {
    CURACNTR + (pipe as u32) * crate::regs::PIPE_STEP
}

// ---- pipe / transcoder write-side registers (intel_display_regs.h) ----
pub const PIPE_MISC_A: u32 = 0x70030; // intel_display_regs.h:845
pub const PIPE_MISC_BPC_MASK: u32 = 7 << 5; // intel_display_regs.h:864
pub const PIPE_MISC_BPC_8: u32 = 0 << 5; // intel_display_regs.h:865
pub const PIPE_MISC_BPC_10: u32 = 1 << 5; // intel_display_regs.h:866
pub const PIPE_MISC_BPC_6: u32 = 2 << 5; // intel_display_regs.h:867
pub const WM_LINETIME_A: u32 = 0x45270; // intel_display_regs.h:2838
pub const HSW_LINETIME_MASK: u32 = 0x1ff; // intel_display_regs.h:2841
pub const HSW_IPS_LINETIME_SHIFT: u32 = 16; // intel_display_regs.h:2843
pub const CHICKEN_TRANS_A: u32 = 0x420c0; // intel_display_regs.h:1469
pub const CHICKEN_TRANS_EDP: u32 = 0x420cc; // intel_display_regs.h:1472
pub const HSW_FRAME_START_DELAY_SHIFT: u32 = 27; // intel_display_regs.h:1488
pub const HSW_FRAME_START_DELAY_MASK: u32 = 3 << 27; // intel_display_regs.h:1488
pub const TRANSCONF_STATE_ENABLE: u32 = 1 << 30; // intel_display_regs.h:734
pub const TRANSCONF_INTERLACE_PF_PD_ILK: u32 = 0 << 21; // intel_display_regs.h:759
pub const TRANSCONF_INTERLACE_IF_ID_ILK: u32 = 3 << 21; // intel_display_regs.h:761
pub const TU_SIZE_SHIFT: u32 = 25; // intel_display_regs.h:697
pub const DATA_LINK_N_MAX: u32 = 0x80_0000; // intel_display_regs.h:700

/// `CHICKEN_TRANS(trans)` for display ver < 14 — intel_display_regs.h:1474-1479 (`_PICK` over A/B/C/EDP).
pub const fn chicken_trans(t: Transcoder) -> u32 {
    match t {
        Transcoder::A => CHICKEN_TRANS_A,
        Transcoder::B => CHICKEN_TRANS_A + 4,
        Transcoder::C => CHICKEN_TRANS_A + 8,
        Transcoder::Edp => CHICKEN_TRANS_EDP,
    }
}
/// `PIPE_MISC(pipe)` — intel_display_regs.h:847.
pub const fn pipe_misc(pipe: Pipe) -> u32 {
    PIPE_MISC_A + (pipe as u32) * crate::regs::PIPE_STEP
}
/// `WM_LINETIME(pipe)` — intel_display_regs.h:2840 (pipe B is 4 above pipe A).
pub const fn wm_linetime(pipe: Pipe) -> u32 {
    WM_LINETIME_A + (pipe as u32) * 4
}
/// `PIPE_DATA_M1/N1`, `PIPE_LINK_M1/N1` for a transcoder — intel_display_regs.h:1025,1029,1041,1045 (`_MMIO_TRANS2`).
pub const fn pipe_data_m1(t: Transcoder) -> u32 {
    crate::regs::trans_reg(t, crate::regs::PIPEA_DATA_M1)
}
pub const fn pipe_data_n1(t: Transcoder) -> u32 {
    crate::regs::trans_reg(t, crate::regs::PIPEA_DATA_N1)
}
pub const fn pipe_link_m1(t: Transcoder) -> u32 {
    crate::regs::trans_reg(t, crate::regs::PIPEA_LINK_M1)
}
pub const fn pipe_link_n1(t: Transcoder) -> u32 {
    crate::regs::trans_reg(t, crate::regs::PIPEA_LINK_N1)
}
/// `TRANSCONF(trans)` — the pipe-register block of the transcoder (`regs::pipe_reg`).
pub const fn transconf(t: Transcoder) -> u32 {
    pipe_reg(t, crate::regs::TRANSACONF)
}
/// `PIPESRC(trans)` (`_MMIO_TRANS2`) — intel_display_regs.h:396.
pub const fn pipesrc(t: Transcoder) -> u32 {
    crate::regs::trans_reg(t, crate::regs::PIPEASRC)
}

// ---- DBUF slice power (skl_watermark_regs.h; 0-based slice numbering, DBUF_S1 of display ver 9 = slice 0) ----
pub const DBUF_CTL_S0: u32 = 0x45008; // skl_watermark_regs.h:56
pub const DBUF_POWER_REQUEST: u32 = 1 << 31; // skl_watermark_regs.h:65
pub const DBUF_POWER_STATE: u32 = 1 << 30; // skl_watermark_regs.h:66

/// The PCU mailbox command that reports the memory latency levels (i915_reg.h:1101), and its four byte lanes (:1102-1105).
pub const GEN9_PCODE_READ_MEM_LATENCY: u32 = 0x6; // i915_reg.h:1101
pub const GEN9_MEM_LATENCY_LEVEL_0_4_MASK: u32 = 0xff; // i915_reg.h:1105
pub const GEN9_MEM_LATENCY_LEVEL_1_5_MASK: u32 = 0xff << 8; // i915_reg.h:1104
pub const GEN9_MEM_LATENCY_LEVEL_2_6_MASK: u32 = 0xff << 16; // i915_reg.h:1103
pub const GEN9_MEM_LATENCY_LEVEL_3_7_MASK: u32 = 0xff << 24; // i915_reg.h:1102
