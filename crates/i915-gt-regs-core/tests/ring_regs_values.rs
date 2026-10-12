// SPDX-License-Identifier: GPL-2.0-only
//! The ring / execlist / RPCS registers added for the submission path, and the three chicken-register rows the Gen9
//! indirect-context workaround batch needs: each against the value GCC computes from the REAL Linux headers
//! (intel_engine_regs.h, intel_gt_regs.h compiled with kernel-macro stubs), written out literally. The expected side is
//! Linux's number; the actual side travels through the crate's constant or function.
use i915_gt_regs_core::*;

#[test]
fn ring_and_execlist_registers_equal_the_values_gcc_computes_from_the_linux_headers() {
    assert_eq!(ring_tail(0x2000).offset(), 0x2030, "ring_tail(0x2000)");
    assert_eq!(TAIL_ADDR, 0x001ffff8, "TAIL_ADDR");
    assert_eq!(ring_head(0x2000).offset(), 0x2034, "ring_head(0x2000)");
    assert_eq!(HEAD_WRAP_COUNT, 0xffe00000, "HEAD_WRAP_COUNT");
    assert_eq!(HEAD_WRAP_ONE, 0x00200000, "HEAD_WRAP_ONE");
    assert_eq!(HEAD_ADDR, 0x001ffffc, "HEAD_ADDR");
    assert_eq!(ring_start(0x2000).offset(), 0x2038, "ring_start(0x2000)");
    assert_eq!(ring_ctl(0x2000).offset(), 0x203c, "ring_ctl(0x2000)");
    assert_eq!(RING_NR_PAGES, 0x001ff000, "RING_NR_PAGES");
    assert_eq!(RING_VALID, 0x00000001, "RING_VALID");
    assert_eq!(ring_hws_pga(0x2000).offset(), 0x2080, "ring_hws_pga(0x2000)");
    assert_eq!(ring_hwstam(0x2000).offset(), 0x2098, "ring_hwstam(0x2000)");
    assert_eq!(ring_mi_mode(0x2000).offset(), 0x209c, "ring_mi_mode(0x2000)");
    assert_eq!(MODE_IDLE, 0x00000200, "MODE_IDLE");
    assert_eq!(STOP_RING, 0x00000100, "STOP_RING");
    assert_eq!(ring_esr(0x2000).offset(), 0x20b8, "ring_esr(0x2000)");
    assert_eq!(gen8_r_pwr_clk_state(0x2000).offset(), 0x20c8, "gen8_r_pwr_clk_state(0x2000)");
    assert_eq!(GEN8_RPCS_ENABLE, 0x80000000, "GEN8_RPCS_ENABLE");
    assert_eq!(GEN8_RPCS_S_CNT_ENABLE, 0x00040000, "GEN8_RPCS_S_CNT_ENABLE");
    assert_eq!(GEN8_RPCS_S_CNT_SHIFT, 0x0000000f, "GEN8_RPCS_S_CNT_SHIFT");
    assert_eq!(GEN8_RPCS_S_CNT_MASK, 0x00038000, "GEN8_RPCS_S_CNT_MASK");
    assert_eq!(GEN8_RPCS_SS_CNT_ENABLE, 0x00000800, "GEN8_RPCS_SS_CNT_ENABLE");
    assert_eq!(GEN8_RPCS_SS_CNT_SHIFT, 0x00000008, "GEN8_RPCS_SS_CNT_SHIFT");
    assert_eq!(GEN8_RPCS_SS_CNT_MASK, 0x00000700, "GEN8_RPCS_SS_CNT_MASK");
    assert_eq!(GEN8_RPCS_EU_MAX_SHIFT, 0x00000004, "GEN8_RPCS_EU_MAX_SHIFT");
    assert_eq!(GEN8_RPCS_EU_MAX_MASK, 0x000000f0, "GEN8_RPCS_EU_MAX_MASK");
    assert_eq!(GEN8_RPCS_EU_MIN_SHIFT, 0000000000, "GEN8_RPCS_EU_MIN_SHIFT");
    assert_eq!(GEN8_RPCS_EU_MIN_MASK, 0x0000000f, "GEN8_RPCS_EU_MIN_MASK");
    assert_eq!(ring_bb_per_ctx_ptr(0x2000).offset(), 0x21c0, "ring_bb_per_ctx_ptr(0x2000)");
    assert_eq!(PER_CTX_BB_FORCE, 0x00000004, "PER_CTX_BB_FORCE");
    assert_eq!(PER_CTX_BB_VALID, 0x00000001, "PER_CTX_BB_VALID");
    assert_eq!(ring_indirect_ctx(0x2000).offset(), 0x21c4, "ring_indirect_ctx(0x2000)");
    assert_eq!(ring_indirect_ctx_offset(0x2000).offset(), 0x21c8, "ring_indirect_ctx_offset(0x2000)");
    assert_eq!(ring_elsp(0x2000).offset(), 0x2230, "ring_elsp(0x2000)");
    assert_eq!(ring_execlist_status_lo(0x2000).offset(), 0x2234, "ring_execlist_status_lo(0x2000)");
    assert_eq!(ring_execlist_status_hi(0x2000).offset(), 0x2238, "ring_execlist_status_hi(0x2000)");
    assert_eq!(ring_context_control(0x2000).offset(), 0x2244, "ring_context_control(0x2000)");
    assert_eq!(CTX_CTRL_ENGINE_CTX_RESTORE_INHIBIT, 0x00000001, "CTX_CTRL_ENGINE_CTX_RESTORE_INHIBIT");
    assert_eq!(CTX_CTRL_RS_CTX_ENABLE, 0x00000002, "CTX_CTRL_RS_CTX_ENABLE");
    assert_eq!(CTX_CTRL_ENGINE_CTX_SAVE_INHIBIT, 0x00000004, "CTX_CTRL_ENGINE_CTX_SAVE_INHIBIT");
    assert_eq!(CTX_CTRL_INHIBIT_SYN_CTX_SWITCH, 0x00000008, "CTX_CTRL_INHIBIT_SYN_CTX_SWITCH");
    assert_eq!(ring_mode_gen7(0x2000).offset(), 0x229c, "ring_mode_gen7(0x2000)");
    assert_eq!(GFX_RUN_LIST_ENABLE, 0x00008000, "GFX_RUN_LIST_ENABLE");
    assert_eq!(ring_context_status_ptr(0x2000).offset(), 0x23a0, "ring_context_status_ptr(0x2000)");
}

#[test]
fn chicken_registers_for_the_indirect_ctx_batch_equal_the_values_gcc_computes() {
    assert_eq!(FF_SLICE_CHICKEN.offset(), 0x2088, "FF_SLICE_CHICKEN");
    assert_eq!(FF_SLICE_CHICKEN_CL_PROVOKING_VERTEX_FIX, 0x00000002, "FF_SLICE_CHICKEN_CL_PROVOKING_VERTEX_FIX");
    assert_eq!(_3D_CHICKEN3.offset(), 0x2090, "_3D_CHICKEN3");
    assert_eq!(_3D_CHICKEN_SF_PROVOKING_VERTEX_FIX, 0x00001000, "_3D_CHICKEN_SF_PROVOKING_VERTEX_FIX");
    assert_eq!(GEN9_DISABLE_GATHER_AT_SET_SHADER_COMMON_SLICE, 0x00001000, "GEN9_DISABLE_GATHER_AT_SET_SHADER_COMMON_SLICE");
}
