// SPDX-License-Identifier: GPL-2.0-only
//! i915-gpu-commands-core: the command-stream opcodes, ONE definition each (the "one home per header" rule: a later crate
//! that emits a command imports it from here and, if it is missing, ADDS it here).
//!
//! Ported from Linux drivers/gpu/drm/i915/gt/intel_gpu_commands.h at v7.0-rc6. Every constant carries `file:line`. This is the
//! Gen4-Gen9 subset the Kaby Lake ring uses: the MI_ instruction set (`:37-194`), the 3D `GFX_OP_*` headers (`:199-223`), the
//! Gen6+ blitter headers (`:239-282`) and `PIPE_CONTROL` (`:287-319`). The Gen11+ / Gen12 bit groups and the CCS / XY_CTRL_SURF
//! copy commands (`:225-237`, `:320-475`) are NOT ported: no Kaby Lake path emits them.
//! Copyright (c) 2003-2018 Intel Corporation and the i915 authors.

#![no_std]
#![forbid(unsafe_code)]

pub mod parser_cmds;
pub use parser_cmds::*;

/// `INSTR_CLIENT_SHIFT` (intel_gpu_commands.h:21).
pub const INSTR_CLIENT_SHIFT: u32 = 29; // intel_gpu_commands.h:21
/// `INSTR_MI_CLIENT` (intel_gpu_commands.h:22): zero-valued, pinned by being the only client whose header has no bits set.
pub const INSTR_MI_CLIENT: u32 = 0x0; // intel_gpu_commands.h:22
pub const INSTR_BC_CLIENT: u32 = 0x2; // intel_gpu_commands.h:23
pub const INSTR_RC_CLIENT: u32 = 0x3; // intel_gpu_commands.h:25

/// `__INSTR(client)` (intel_gpu_commands.h:32).
pub const fn instr(client: u32) -> u32 {
    client << INSTR_CLIENT_SHIFT
}

/// `MI_INSTR(opcode, flags)` (intel_gpu_commands.h:37-38).
pub const fn mi_instr(opcode: u32, flags: u32) -> u32 {
    instr(INSTR_MI_CLIENT) | (opcode << 23) | flags
}

/// `MI_GLOBAL_GTT`: bit 22 of many MI headers selects GGTT over PPGTT (intel_gpu_commands.h:40).
pub const MI_GLOBAL_GTT: u32 = 1 << 22; // intel_gpu_commands.h:40

pub const MI_NOOP: u32 = mi_instr(0, 0); // intel_gpu_commands.h:42
pub const MI_SET_PREDICATE: u32 = mi_instr(0x01, 0); // intel_gpu_commands.h:43
pub const MI_SET_PREDICATE_DISABLE: u32 = 0; // intel_gpu_commands.h:44
pub const MI_USER_INTERRUPT: u32 = mi_instr(0x02, 0); // intel_gpu_commands.h:45
pub const MI_WAIT_FOR_EVENT: u32 = mi_instr(0x03, 0); // intel_gpu_commands.h:46
pub const MI_WAIT_FOR_OVERLAY_FLIP: u32 = 1 << 16; // intel_gpu_commands.h:47
pub const MI_WAIT_FOR_PLANE_B_FLIP: u32 = 1 << 6; // intel_gpu_commands.h:48
pub const MI_WAIT_FOR_PLANE_A_FLIP: u32 = 1 << 2; // intel_gpu_commands.h:49
pub const MI_WAIT_FOR_PLANE_A_SCANLINES: u32 = 1 << 1; // intel_gpu_commands.h:50
pub const MI_FLUSH: u32 = mi_instr(0x04, 0); // intel_gpu_commands.h:51
pub const MI_READ_FLUSH: u32 = 1 << 0; // intel_gpu_commands.h:52
pub const MI_EXE_FLUSH: u32 = 1 << 1; // intel_gpu_commands.h:53
pub const MI_NO_WRITE_FLUSH: u32 = 1 << 2; // intel_gpu_commands.h:54
pub const MI_SCENE_COUNT: u32 = 1 << 3; // intel_gpu_commands.h:55
pub const MI_END_SCENE: u32 = 1 << 4; // intel_gpu_commands.h:56
pub const MI_INVALIDATE_ISP: u32 = 1 << 5; // intel_gpu_commands.h:57
pub const MI_REPORT_HEAD: u32 = mi_instr(0x07, 0); // intel_gpu_commands.h:58
pub const MI_ARB_ON_OFF: u32 = mi_instr(0x08, 0); // intel_gpu_commands.h:59
pub const MI_ARB_ENABLE: u32 = 1 << 0; // intel_gpu_commands.h:60
/// Zero-valued (intel_gpu_commands.h:61): pinned by behaviour - `MI_ARB_ON_OFF | MI_ARB_DISABLE` differs from `| MI_ARB_ENABLE`.
pub const MI_ARB_DISABLE: u32 = 0 << 0; // intel_gpu_commands.h:61
pub const MI_BATCH_BUFFER_END: u32 = mi_instr(0x0a, 0); // intel_gpu_commands.h:62
pub const MI_SUSPEND_FLUSH: u32 = mi_instr(0x0b, 0); // intel_gpu_commands.h:63
pub const MI_SUSPEND_FLUSH_EN: u32 = 1 << 0; // intel_gpu_commands.h:64
pub const MI_SET_APPID: u32 = mi_instr(0x0e, 0); // intel_gpu_commands.h:65
pub const MI_OVERLAY_FLIP: u32 = mi_instr(0x11, 0); // intel_gpu_commands.h:67
pub const MI_OVERLAY_CONTINUE: u32 = 0x0 << 21; // intel_gpu_commands.h:68
pub const MI_OVERLAY_ON: u32 = 0x1 << 21; // intel_gpu_commands.h:69
pub const MI_OVERLAY_OFF: u32 = 0x2 << 21; // intel_gpu_commands.h:70
pub const MI_LOAD_SCAN_LINES_INCL: u32 = mi_instr(0x12, 0); // intel_gpu_commands.h:71
pub const MI_DISPLAY_FLIP: u32 = mi_instr(0x14, 2); // intel_gpu_commands.h:72
pub const MI_DISPLAY_FLIP_I915: u32 = mi_instr(0x14, 1); // intel_gpu_commands.h:73
/// `MI_DISPLAY_FLIP_PLANE(n)` (intel_gpu_commands.h:74).
pub const fn mi_display_flip_plane(n: u32) -> u32 {
    n << 20
}
pub const MI_DISPLAY_FLIP_IVB_PLANE_A: u32 = 0 << 19; // intel_gpu_commands.h:76
pub const MI_DISPLAY_FLIP_IVB_PLANE_B: u32 = 1 << 19; // intel_gpu_commands.h:77
pub const MI_DISPLAY_FLIP_IVB_SPRITE_A: u32 = 2 << 19; // intel_gpu_commands.h:78
pub const MI_DISPLAY_FLIP_IVB_SPRITE_B: u32 = 3 << 19; // intel_gpu_commands.h:79
pub const MI_DISPLAY_FLIP_IVB_PLANE_C: u32 = 4 << 19; // intel_gpu_commands.h:80
pub const MI_DISPLAY_FLIP_IVB_SPRITE_C: u32 = 5 << 19; // intel_gpu_commands.h:81
pub const MI_DISPLAY_FLIP_SKL_PLANE_1_A: u32 = 0 << 8; // intel_gpu_commands.h:83
pub const MI_DISPLAY_FLIP_SKL_PLANE_1_B: u32 = 1 << 8; // intel_gpu_commands.h:84
pub const MI_DISPLAY_FLIP_SKL_PLANE_1_C: u32 = 2 << 8; // intel_gpu_commands.h:85
pub const MI_DISPLAY_FLIP_SKL_PLANE_2_A: u32 = 4 << 8; // intel_gpu_commands.h:86
pub const MI_DISPLAY_FLIP_SKL_PLANE_2_B: u32 = 5 << 8; // intel_gpu_commands.h:87
pub const MI_DISPLAY_FLIP_SKL_PLANE_2_C: u32 = 6 << 8; // intel_gpu_commands.h:88
pub const MI_DISPLAY_FLIP_SKL_PLANE_3_A: u32 = 7 << 8; // intel_gpu_commands.h:89
pub const MI_DISPLAY_FLIP_SKL_PLANE_3_B: u32 = 8 << 8; // intel_gpu_commands.h:90
pub const MI_DISPLAY_FLIP_SKL_PLANE_3_C: u32 = 9 << 8; // intel_gpu_commands.h:91

pub const MI_SEMAPHORE_MBOX: u32 = mi_instr(0x16, 1); // intel_gpu_commands.h:92
pub const MI_SEMAPHORE_GLOBAL_GTT: u32 = 1 << 22; // intel_gpu_commands.h:93
pub const MI_SEMAPHORE_UPDATE: u32 = 1 << 21; // intel_gpu_commands.h:94
pub const MI_SEMAPHORE_COMPARE: u32 = 1 << 20; // intel_gpu_commands.h:95
pub const MI_SEMAPHORE_REGISTER: u32 = 1 << 18; // intel_gpu_commands.h:96
pub const MI_SEMAPHORE_SYNC_MASK: u32 = 3 << 16; // intel_gpu_commands.h:110

pub const MI_SET_CONTEXT: u32 = mi_instr(0x18, 0); // intel_gpu_commands.h:111
pub const MI_MM_SPACE_GTT: u32 = 1 << 8; // intel_gpu_commands.h:112
pub const MI_MM_SPACE_PHYSICAL: u32 = 0 << 8; // intel_gpu_commands.h:113
pub const MI_SAVE_EXT_STATE_EN: u32 = 1 << 3; // intel_gpu_commands.h:114
pub const MI_RESTORE_EXT_STATE_EN: u32 = 1 << 2; // intel_gpu_commands.h:115
pub const MI_FORCE_RESTORE: u32 = 1 << 1; // intel_gpu_commands.h:116
pub const MI_RESTORE_INHIBIT: u32 = 1 << 0; // intel_gpu_commands.h:117
pub const HSW_MI_RS_SAVE_STATE_EN: u32 = 1 << 3; // intel_gpu_commands.h:118
pub const HSW_MI_RS_RESTORE_STATE_EN: u32 = 1 << 2; // intel_gpu_commands.h:119
pub const MI_SEMAPHORE_SIGNAL: u32 = mi_instr(0x1b, 0); // intel_gpu_commands.h:120
/// `MI_SEMAPHORE_TARGET(engine)` (intel_gpu_commands.h:121).
pub const fn mi_semaphore_target(engine: u32) -> u32 {
    engine << 15
}
pub const MI_SEMAPHORE_WAIT: u32 = mi_instr(0x1c, 2); // intel_gpu_commands.h:122
pub const MI_SEMAPHORE_WAIT_TOKEN: u32 = mi_instr(0x1c, 3); // intel_gpu_commands.h:123
pub const MI_SEMAPHORE_REGISTER_POLL: u32 = 1 << 16; // intel_gpu_commands.h:124
pub const MI_SEMAPHORE_POLL: u32 = 1 << 15; // intel_gpu_commands.h:125
/// `MI_SEMAPHORE_SAD_GT_SDD` is zero-valued (intel_gpu_commands.h:126): pinned by being distinct from `..._GTE_SDD`.
pub const MI_SEMAPHORE_SAD_GT_SDD: u32 = 0 << 12; // intel_gpu_commands.h:126
pub const MI_SEMAPHORE_SAD_GTE_SDD: u32 = 1 << 12; // intel_gpu_commands.h:127
pub const MI_SEMAPHORE_SAD_LT_SDD: u32 = 2 << 12; // intel_gpu_commands.h:128
pub const MI_SEMAPHORE_SAD_LTE_SDD: u32 = 3 << 12; // intel_gpu_commands.h:129
pub const MI_SEMAPHORE_SAD_EQ_SDD: u32 = 4 << 12; // intel_gpu_commands.h:130
pub const MI_SEMAPHORE_SAD_NEQ_SDD: u32 = 5 << 12; // intel_gpu_commands.h:131

pub const MI_STORE_DATA_IMM: u32 = mi_instr(0x20, 0); // intel_gpu_commands.h:134
pub const MI_STORE_DWORD_IMM: u32 = mi_instr(0x20, 1); // intel_gpu_commands.h:135
pub const MI_STORE_DWORD_IMM_GEN4: u32 = mi_instr(0x20, 2); // intel_gpu_commands.h:136
pub const MI_STORE_QWORD_IMM_GEN8: u32 = mi_instr(0x20, 3) | (1 << 21); // intel_gpu_commands.h:137
pub const MI_MEM_VIRTUAL: u32 = 1 << 22; // intel_gpu_commands.h:138
pub const MI_USE_GGTT: u32 = 1 << 22; // intel_gpu_commands.h:139
pub const MI_STORE_DWORD_INDEX: u32 = mi_instr(0x21, 1); // intel_gpu_commands.h:140
pub const MI_ATOMIC: u32 = mi_instr(0x2f, 1); // intel_gpu_commands.h:141
pub const MI_ATOMIC_GLOBAL_GTT: u32 = 1 << 22; // intel_gpu_commands.h:143
pub const MI_ATOMIC_INLINE_DATA: u32 = 1 << 18; // intel_gpu_commands.h:144
pub const MI_ATOMIC_INLINE: u32 = mi_instr(0x2f, 9) | MI_ATOMIC_INLINE_DATA; // intel_gpu_commands.h:142
pub const MI_ATOMIC_CS_STALL: u32 = 1 << 17; // intel_gpu_commands.h:145
pub const MI_ATOMIC_MOVE: u32 = 0x4 << 8; // intel_gpu_commands.h:146

/// `MI_LOAD_REGISTER_IMM(x)` (intel_gpu_commands.h:155): `x` register/value pairs follow. Linux's own comment (:149-153): always
/// issue a `MI_NOOP` BEFORE it, and keep `x <= 16`. `x` must be at least 1.
pub const fn mi_load_register_imm(x: u32) -> u32 {
    assert!(x >= 1, "MI_LOAD_REGISTER_IMM(0) would encode length -1");
    mi_instr(0x22, 2 * x - 1)
}
pub const MI_LRI_LRM_CS_MMIO: u32 = 1 << 19; // intel_gpu_commands.h:157
pub const MI_LRI_MMIO_REMAP_EN: u32 = 1 << 17; // intel_gpu_commands.h:158
pub const MI_LRI_FORCE_POSTED: u32 = 1 << 12; // intel_gpu_commands.h:159
pub const MI_LOAD_REGISTER_IMM_MAX_REGS: u32 = 126; // intel_gpu_commands.h:160
pub const MI_STORE_REGISTER_MEM: u32 = mi_instr(0x24, 1); // intel_gpu_commands.h:161
pub const MI_STORE_REGISTER_MEM_GEN8: u32 = mi_instr(0x24, 2); // intel_gpu_commands.h:162
pub const MI_SRM_LRM_GLOBAL_GTT: u32 = 1 << 22; // intel_gpu_commands.h:163
pub const MI_FLUSH_DW: u32 = mi_instr(0x26, 1); // intel_gpu_commands.h:164
pub const MI_FLUSH_DW_PROTECTED_MEM_EN: u32 = 1 << 22; // intel_gpu_commands.h:165
pub const MI_FLUSH_DW_STORE_INDEX: u32 = 1 << 21; // intel_gpu_commands.h:166
pub const MI_INVALIDATE_TLB: u32 = 1 << 18; // intel_gpu_commands.h:167
pub const MI_FLUSH_DW_CCS: u32 = 1 << 16; // intel_gpu_commands.h:168
pub const MI_FLUSH_DW_OP_STOREDW: u32 = 1 << 14; // intel_gpu_commands.h:169
pub const MI_FLUSH_DW_OP_MASK: u32 = 3 << 14; // intel_gpu_commands.h:170
pub const MI_FLUSH_DW_LLC: u32 = 1 << 9; // intel_gpu_commands.h:171
pub const MI_FLUSH_DW_NOTIFY: u32 = 1 << 8; // intel_gpu_commands.h:172
pub const MI_INVALIDATE_BSD: u32 = 1 << 7; // intel_gpu_commands.h:173
pub const MI_FLUSH_DW_USE_GTT: u32 = 1 << 2; // intel_gpu_commands.h:174
pub const MI_FLUSH_DW_USE_PPGTT: u32 = 0 << 2; // intel_gpu_commands.h:175
pub const MI_LOAD_REGISTER_MEM: u32 = mi_instr(0x29, 1); // intel_gpu_commands.h:176
pub const MI_LOAD_REGISTER_MEM_GEN8: u32 = mi_instr(0x29, 2); // intel_gpu_commands.h:177
pub const MI_LOAD_REGISTER_REG: u32 = mi_instr(0x2a, 1); // intel_gpu_commands.h:178
pub const MI_LRR_SOURCE_CS_MMIO: u32 = 1 << 18; // intel_gpu_commands.h:179
pub const MI_BATCH_BUFFER: u32 = mi_instr(0x30, 1); // intel_gpu_commands.h:180
pub const MI_BATCH_NON_SECURE: u32 = 1; // intel_gpu_commands.h:181
pub const MI_BATCH_NON_SECURE_I965: u32 = 1 << 8; // intel_gpu_commands.h:183
pub const MI_BATCH_PPGTT_HSW: u32 = 1 << 8; // intel_gpu_commands.h:184
pub const MI_BATCH_NON_SECURE_HSW: u32 = 1 << 13; // intel_gpu_commands.h:185
pub const MI_BATCH_BUFFER_START: u32 = mi_instr(0x31, 0); // intel_gpu_commands.h:186
pub const MI_BATCH_GTT: u32 = 2 << 6; // intel_gpu_commands.h:187
pub const MI_BATCH_BUFFER_START_GEN8: u32 = mi_instr(0x31, 1); // intel_gpu_commands.h:188
pub const MI_BATCH_RESOURCE_STREAMER: u32 = 1 << 10; // intel_gpu_commands.h:189
pub const MI_BATCH_PREDICATE: u32 = 1 << 15; // intel_gpu_commands.h:190

/// `MI_OPCODE(x)` (intel_gpu_commands.h:192).
pub const fn mi_opcode(x: u32) -> u32 {
    (x >> 23) & 0x3f
}
/// `IS_MI_LRI_CMD(x)` (intel_gpu_commands.h:193).
pub const fn is_mi_lri_cmd(x: u32) -> bool {
    mi_opcode(x) == mi_opcode(mi_instr(0x22, 0))
}
/// `MI_LRI_LEN(x)` (intel_gpu_commands.h:194).
pub const fn mi_lri_len(x: u32) -> u32 {
    (x & 0xff) + 1
}

// ---- 3D instructions (intel_gpu_commands.h:196-223) ----------------------------------------------------------------------

/// `GFX_INSTR(opcode, flags)` (intel_gpu_commands.h:199).
pub const fn gfx_instr(opcode: u32, flags: u32) -> u32 {
    (0x3 << 29) | (opcode << 24) | flags
}
pub const GEN9_MEDIA_POOL_STATE: u32 = (0x3 << 29) | (0x2 << 27) | (0x5 << 16) | 4; // intel_gpu_commands.h:201
pub const GEN9_MEDIA_POOL_ENABLE: u32 = 1 << 31; // intel_gpu_commands.h:202
pub const GFX_OP_RASTER_RULES: u32 = (0x3 << 29) | (0x7 << 24); // intel_gpu_commands.h:203
pub const GFX_OP_SCISSOR: u32 = (0x3 << 29) | (0x1c << 24) | (0x10 << 19); // intel_gpu_commands.h:204
pub const GFX_OP_LOAD_INDIRECT: u32 = (0x3 << 29) | (0x1d << 24) | (0x7 << 16); // intel_gpu_commands.h:208
pub const GFX_OP_SCISSOR_INFO: u32 = (0x3 << 29) | (0x1d << 24) | (0x81 << 16) | 0x1; // intel_gpu_commands.h:209
pub const GFX_OP_SCISSOR_ENABLE: u32 = (0x3 << 29) | (0x1c << 24) | (0x10 << 19); // intel_gpu_commands.h:214
pub const GFX_OP_SCISSOR_RECT: u32 = (0x3 << 29) | (0x1d << 24) | (0x81 << 16) | 1; // intel_gpu_commands.h:215
pub const GFX_OP_COLOR_FACTOR: u32 = (0x3 << 29) | (0x1d << 24) | (0x1 << 16); // intel_gpu_commands.h:216
pub const GFX_OP_STIPPLE: u32 = (0x3 << 29) | (0x1d << 24) | (0x83 << 16); // intel_gpu_commands.h:217
pub const GFX_OP_MAP_INFO: u32 = (0x3 << 29) | (0x1d << 24) | 0x4; // intel_gpu_commands.h:218
pub const GFX_OP_DESTBUFFER_VARS: u32 = (0x3 << 29) | (0x1d << 24) | (0x85 << 16); // intel_gpu_commands.h:219
pub const GFX_OP_DESTBUFFER_INFO: u32 = (0x3 << 29) | (0x1d << 24) | (0x8e << 16) | 1; // intel_gpu_commands.h:220
pub const GFX_OP_DRAWRECT_INFO: u32 = (0x3 << 29) | (0x1d << 24) | (0x80 << 16) | 0x3; // intel_gpu_commands.h:221
pub const GFX_OP_DRAWRECT_INFO_I965: u32 = (0x7900 << 16) | 0x2; // intel_gpu_commands.h:222
pub const CMD_3DSTATE_MESH_CONTROL: u32 = (0x3 << 29) | (0x3 << 27) | (0x77 << 16) | 0x3; // intel_gpu_commands.h:223

// ---- Blitter (intel_gpu_commands.h:239-282) ------------------------------------------------------------------------------

pub const COLOR_BLT_CMD: u32 = (2 << 29) | (0x40 << 22) | (5 - 2); // intel_gpu_commands.h:239
pub const XY_COLOR_BLT_CMD: u32 = (2 << 29) | (0x50 << 22); // intel_gpu_commands.h:240
pub const XY_FAST_COLOR_BLT_CMD: u32 = (2 << 29) | (0x44 << 22); // intel_gpu_commands.h:241
pub const SRC_COPY_BLT_CMD: u32 = (2 << 29) | (0x43 << 22); // intel_gpu_commands.h:268
pub const GEN9_XY_FAST_COPY_BLT_CMD: u32 = (2 << 29) | (0x42 << 22); // intel_gpu_commands.h:269
pub const XY_SRC_COPY_BLT_CMD: u32 = (2 << 29) | (0x53 << 22); // intel_gpu_commands.h:270
pub const XY_MONO_SRC_COPY_IMM_BLT: u32 = (2 << 29) | (0x71 << 22) | 5; // intel_gpu_commands.h:271
pub const BLT_WRITE_A: u32 = 2 << 20; // intel_gpu_commands.h:272
pub const BLT_WRITE_RGB: u32 = 1 << 20; // intel_gpu_commands.h:273
pub const BLT_WRITE_RGBA: u32 = BLT_WRITE_RGB | BLT_WRITE_A; // intel_gpu_commands.h:274
pub const BLT_DEPTH_8: u32 = 0 << 24; // intel_gpu_commands.h:275
pub const BLT_DEPTH_16_565: u32 = 1 << 24; // intel_gpu_commands.h:276
pub const BLT_DEPTH_16_1555: u32 = 2 << 24; // intel_gpu_commands.h:277
pub const BLT_DEPTH_32: u32 = 3 << 24; // intel_gpu_commands.h:278
pub const BLT_ROP_SRC_COPY: u32 = 0xcc << 16; // intel_gpu_commands.h:279
pub const BLT_ROP_COLOR_COPY: u32 = 0xf0 << 16; // intel_gpu_commands.h:280
pub const XY_SRC_COPY_BLT_SRC_TILED: u32 = 1 << 15; // intel_gpu_commands.h:281
pub const XY_SRC_COPY_BLT_DST_TILED: u32 = 1 << 11; // intel_gpu_commands.h:282

// ---- PIPE_CONTROL (intel_gpu_commands.h:287-319) -------------------------------------------------------------------------

/// `GFX_OP_PIPE_CONTROL(len)` (intel_gpu_commands.h:287). `len` is the total dword count and must be at least 2.
pub const fn gfx_op_pipe_control(len: u32) -> u32 {
    assert!(len >= 2, "GFX_OP_PIPE_CONTROL(len): len-2 would underflow");
    (0x3 << 29) | (0x3 << 27) | (0x2 << 24) | (len - 2)
}
pub const PIPE_CONTROL_COMMAND_CACHE_INVALIDATE: u32 = 1 << 29; // intel_gpu_commands.h:288
pub const PIPE_CONTROL_TILE_CACHE_FLUSH: u32 = 1 << 28; // intel_gpu_commands.h:289
pub const PIPE_CONTROL_FLUSH_L3: u32 = 1 << 27; // intel_gpu_commands.h:290
pub const PIPE_CONTROL_AMFS_FLUSH: u32 = 1 << 25; // intel_gpu_commands.h:291
pub const PIPE_CONTROL_GLOBAL_GTT_IVB: u32 = 1 << 24; // intel_gpu_commands.h:292
pub const PIPE_CONTROL_MMIO_WRITE: u32 = 1 << 23; // intel_gpu_commands.h:293
pub const PIPE_CONTROL_STORE_DATA_INDEX: u32 = 1 << 21; // intel_gpu_commands.h:294
pub const PIPE_CONTROL_CS_STALL: u32 = 1 << 20; // intel_gpu_commands.h:295
pub const PIPE_CONTROL_GLOBAL_SNAPSHOT_RESET: u32 = 1 << 19; // intel_gpu_commands.h:296
pub const PIPE_CONTROL_TLB_INVALIDATE: u32 = 1 << 18; // intel_gpu_commands.h:297
pub const PIPE_CONTROL_PSD_SYNC: u32 = 1 << 17; // intel_gpu_commands.h:298
pub const PIPE_CONTROL_MEDIA_STATE_CLEAR: u32 = 1 << 16; // intel_gpu_commands.h:299
pub const PIPE_CONTROL_WRITE_TIMESTAMP: u32 = 3 << 14; // intel_gpu_commands.h:300
pub const PIPE_CONTROL_QW_WRITE: u32 = 1 << 14; // intel_gpu_commands.h:301
pub const PIPE_CONTROL_POST_SYNC_OP_MASK: u32 = 3 << 14; // intel_gpu_commands.h:302
pub const PIPE_CONTROL_DEPTH_STALL: u32 = 1 << 13; // intel_gpu_commands.h:303
pub const PIPE_CONTROL_CCS_FLUSH: u32 = 1 << 13; // intel_gpu_commands.h:304
pub const PIPE_CONTROL_WRITE_FLUSH: u32 = 1 << 12; // intel_gpu_commands.h:305
pub const PIPE_CONTROL_RENDER_TARGET_CACHE_FLUSH: u32 = 1 << 12; // intel_gpu_commands.h:306
pub const PIPE_CONTROL_INSTRUCTION_CACHE_INVALIDATE: u32 = 1 << 11; // intel_gpu_commands.h:307
pub const PIPE_CONTROL_TEXTURE_CACHE_INVALIDATE: u32 = 1 << 10; // intel_gpu_commands.h:308
pub const PIPE_CONTROL_INDIRECT_STATE_DISABLE: u32 = 1 << 9; // intel_gpu_commands.h:309
pub const PIPE_CONTROL0_HDC_PIPELINE_FLUSH: u32 = 1 << 9; // intel_gpu_commands.h:310
pub const PIPE_CONTROL_NOTIFY: u32 = 1 << 8; // intel_gpu_commands.h:311
pub const PIPE_CONTROL_FLUSH_ENABLE: u32 = 1 << 7; // intel_gpu_commands.h:312
pub const PIPE_CONTROL_DC_FLUSH_ENABLE: u32 = 1 << 5; // intel_gpu_commands.h:313
pub const PIPE_CONTROL_VF_CACHE_INVALIDATE: u32 = 1 << 4; // intel_gpu_commands.h:314
pub const PIPE_CONTROL_CONST_CACHE_INVALIDATE: u32 = 1 << 3; // intel_gpu_commands.h:315
pub const PIPE_CONTROL_STATE_CACHE_INVALIDATE: u32 = 1 << 2; // intel_gpu_commands.h:316
pub const PIPE_CONTROL_STALL_AT_SCOREBOARD: u32 = 1 << 1; // intel_gpu_commands.h:317
pub const PIPE_CONTROL_DEPTH_CACHE_FLUSH: u32 = 1 << 0; // intel_gpu_commands.h:318
pub const PIPE_CONTROL_GLOBAL_GTT: u32 = 1 << 2; // intel_gpu_commands.h:319
