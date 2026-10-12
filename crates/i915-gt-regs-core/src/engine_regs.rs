// SPDX-License-Identifier: GPL-2.0-only
//! Per-ring register offsets (intel_engine_regs.h): the ones the reset handshake, the non-privileged whitelist and the CS error path touch.
//! Ported from Linux drivers/gpu/drm/i915/gt/intel_engine_regs.h at v7.0-rc6; every line carries `file:line`.
//! Copyright (c) Intel Corporation and the i915 authors.

use i915_common_core::reg::{bit, genmask};
use i915_common_core::Reg;

pub const RESET_CTL_CAT_ERROR: u32 = bit(2); // intel_engine_regs.h:104
pub const RESET_CTL_READY_TO_RESET: u32 = bit(1); // intel_engine_regs.h:105
pub const RESET_CTL_REQUEST_RESET: u32 = bit(0); // intel_engine_regs.h:106
pub const RING_FORCE_TO_NONPRIV_DENY: u32 = bit(30); // intel_engine_regs.h:214
pub const RING_FORCE_TO_NONPRIV_ADDRESS_MASK: u32 = genmask(25, 2); // intel_engine_regs.h:215
pub const RING_FORCE_TO_NONPRIV_ACCESS_RW: u32 = 0 << 28; // intel_engine_regs.h:216
pub const RING_FORCE_TO_NONPRIV_ACCESS_RD: u32 = 1 << 28; // intel_engine_regs.h:217
pub const RING_FORCE_TO_NONPRIV_ACCESS_WR: u32 = 2 << 28; // intel_engine_regs.h:218
pub const RING_FORCE_TO_NONPRIV_ACCESS_INVALID: u32 = 3 << 28; // intel_engine_regs.h:219
pub const RING_FORCE_TO_NONPRIV_ACCESS_MASK: u32 = 3 << 28; // intel_engine_regs.h:220
pub const RING_FORCE_TO_NONPRIV_RANGE_1: u32 = 0 << 0; // intel_engine_regs.h:221
pub const RING_FORCE_TO_NONPRIV_RANGE_4: u32 = 1 << 0; // intel_engine_regs.h:222
pub const RING_FORCE_TO_NONPRIV_RANGE_16: u32 = 2 << 0; // intel_engine_regs.h:223
pub const RING_FORCE_TO_NONPRIV_RANGE_64: u32 = 3 << 0; // intel_engine_regs.h:224
pub const RING_FORCE_TO_NONPRIV_RANGE_MASK: u32 = 3 << 0; // intel_engine_regs.h:225
pub const RING_MAX_NONPRIV_SLOTS: u32 = 12; // intel_engine_regs.h:230

// ---- Per-ring registers: `base` is the engine's mmio_base -------------------------------------------------------------------

/// `RING_NOPID(base)` (intel_engine_regs.h:71).
pub const fn ring_nopid(base: u32) -> Reg {
    Reg::new(base + 0x94)
}
/// `RING_EIR(base)` (intel_engine_regs.h:81).
pub const fn ring_eir(base: u32) -> Reg {
    Reg::new(base + 0xb0)
}
/// `RING_EMR(base)` (intel_engine_regs.h:82).
pub const fn ring_emr(base: u32) -> Reg {
    Reg::new(base + 0xb4)
}
/// `RING_RESET_CTL(base)` (intel_engine_regs.h:103).
pub const fn ring_reset_ctl(base: u32) -> Reg {
    Reg::new(base + 0xd0)
}
/// `RING_SEMA_WAIT_POLL(base)` (intel_engine_regs.h:189).
pub const fn ring_sema_wait_poll(base: u32) -> Reg {
    Reg::new(base + 0x24c)
}
/// `RING_FORCE_TO_NONPRIV(base, i)` (intel_engine_regs.h:213): slot `i` of the non-privileged-access whitelist.
pub const fn ring_force_to_nonpriv(base: u32, i: u32) -> Reg {
    Reg::new(base + 0x4d0 + i * 4)
}
/// `RING_FORCE_TO_NONPRIV_MASK_VALID` (intel_engine_regs.h:226-229).
pub const RING_FORCE_TO_NONPRIV_MASK_VALID: u32 =
    RING_FORCE_TO_NONPRIV_RANGE_MASK | RING_FORCE_TO_NONPRIV_ACCESS_MASK | RING_FORCE_TO_NONPRIV_DENY; // intel_engine_regs.h:226

// ---- Ring and logical-ring-context registers (added by i915-submit-core, 2026-10-12) ----------------------------------------
// The execlists submission path needs the ring pointer registers, the ELSP / execlist status / context status pointer, the
// context-control register the logical ring context image carries, and the RPCS (render power clock state) fields.

/// `RING_TAIL(base)` (intel_engine_regs.h:12).
pub const fn ring_tail(base: u32) -> Reg {
    Reg::new(base + 0x30)
}
/// `TAIL_ADDR` (intel_engine_regs.h:13): the tail offset bits (qword aligned, 21 bits).
pub const TAIL_ADDR: u32 = 0x001f_fff8; // intel_engine_regs.h:13
/// `RING_HEAD(base)` (intel_engine_regs.h:14).
pub const fn ring_head(base: u32) -> Reg {
    Reg::new(base + 0x34)
}
pub const HEAD_WRAP_COUNT: u32 = 0xffe0_0000; // intel_engine_regs.h:15
pub const HEAD_WRAP_ONE: u32 = 0x0020_0000; // intel_engine_regs.h:16
/// `HEAD_ADDR` (intel_engine_regs.h:17): the head offset bits (dword aligned, 21 bits).
pub const HEAD_ADDR: u32 = 0x001f_fffc; // intel_engine_regs.h:17
/// `RING_START(base)` (intel_engine_regs.h:19).
pub const fn ring_start(base: u32) -> Reg {
    Reg::new(base + 0x38)
}
/// `RING_CTL(base)` (intel_engine_regs.h:20).
pub const fn ring_ctl(base: u32) -> Reg {
    Reg::new(base + 0x3c)
}
/// `RING_NR_PAGES` (intel_engine_regs.h:22): the size field of RING_CTL, in 4 KiB pages minus one.
pub const RING_NR_PAGES: u32 = 0x001f_f000; // intel_engine_regs.h:22
/// `RING_VALID` (intel_engine_regs.h:28): the ring-enable bit of RING_CTL.
pub const RING_VALID: u32 = 0x0000_0001; // intel_engine_regs.h:28
/// `RING_HWS_PGA(base)` (intel_engine_regs.h:66): the status page's GGTT address.
pub const fn ring_hws_pga(base: u32) -> Reg {
    Reg::new(base + 0x80)
}
/// `RING_HWSTAM(base)` (intel_engine_regs.h:72): which interrupt-status bits the hardware mirrors into the HWSP.
pub const fn ring_hwstam(base: u32) -> Reg {
    Reg::new(base + 0x98)
}
/// `RING_MI_MODE(base)` (intel_engine_regs.h:73).
pub const fn ring_mi_mode(base: u32) -> Reg {
    Reg::new(base + 0x9c)
}
pub const MODE_IDLE: u32 = bit(9); // intel_engine_regs.h:77
pub const STOP_RING: u32 = bit(8); // intel_engine_regs.h:78
/// `RING_ESR(base)` (intel_engine_regs.h:83).
pub const fn ring_esr(base: u32) -> Reg {
    Reg::new(base + 0xb8)
}
/// `GEN8_R_PWR_CLK_STATE(base)` (intel_engine_regs.h:88): the RPCS register of the render ring.
pub const fn gen8_r_pwr_clk_state(base: u32) -> Reg {
    Reg::new(base + 0xc8)
}
pub const GEN8_RPCS_ENABLE: u32 = 1 << 31; // intel_engine_regs.h:89
pub const GEN8_RPCS_S_CNT_ENABLE: u32 = 1 << 18; // intel_engine_regs.h:90
pub const GEN8_RPCS_S_CNT_SHIFT: u32 = 15; // intel_engine_regs.h:91
pub const GEN8_RPCS_S_CNT_MASK: u32 = 0x7 << 15; // intel_engine_regs.h:92
pub const GEN8_RPCS_SS_CNT_ENABLE: u32 = 1 << 11; // intel_engine_regs.h:95
pub const GEN8_RPCS_SS_CNT_SHIFT: u32 = 8; // intel_engine_regs.h:96
pub const GEN8_RPCS_SS_CNT_MASK: u32 = 0x7 << 8; // intel_engine_regs.h:97
pub const GEN8_RPCS_EU_MAX_SHIFT: u32 = 4; // intel_engine_regs.h:98
pub const GEN8_RPCS_EU_MAX_MASK: u32 = 0xf << 4; // intel_engine_regs.h:99
pub const GEN8_RPCS_EU_MIN_SHIFT: u32 = 0; // intel_engine_regs.h:100
pub const GEN8_RPCS_EU_MIN_MASK: u32 = 0xf; // intel_engine_regs.h:101
/// `RING_BB_PER_CTX_PTR(base)` (intel_engine_regs.h:119).
pub const fn ring_bb_per_ctx_ptr(base: u32) -> Reg {
    Reg::new(base + 0x1c0)
}
pub const PER_CTX_BB_FORCE: u32 = bit(2); // intel_engine_regs.h:121
pub const PER_CTX_BB_VALID: u32 = bit(0); // intel_engine_regs.h:122
/// `RING_INDIRECT_CTX(base)` (intel_engine_regs.h:124).
pub const fn ring_indirect_ctx(base: u32) -> Reg {
    Reg::new(base + 0x1c4)
}
/// `RING_INDIRECT_CTX_OFFSET(base)` (intel_engine_regs.h:125).
pub const fn ring_indirect_ctx_offset(base: u32) -> Reg {
    Reg::new(base + 0x1c8)
}
/// `RING_ELSP(base)` (intel_engine_regs.h:178): the execlist submit port (write the upper then the lower descriptor dword).
pub const fn ring_elsp(base: u32) -> Reg {
    Reg::new(base + 0x230)
}
/// `RING_EXECLIST_STATUS_LO(base)` (intel_engine_regs.h:179).
pub const fn ring_execlist_status_lo(base: u32) -> Reg {
    Reg::new(base + 0x234)
}
/// `RING_EXECLIST_STATUS_HI(base)` (intel_engine_regs.h:180).
pub const fn ring_execlist_status_hi(base: u32) -> Reg {
    Reg::new(base + 0x234 + 4)
}
/// `RING_CONTEXT_CONTROL(base)` (intel_engine_regs.h:181).
pub const fn ring_context_control(base: u32) -> Reg {
    Reg::new(base + 0x244)
}
pub const CTX_CTRL_ENGINE_CTX_RESTORE_INHIBIT: u32 = bit(0); // intel_engine_regs.h:182
pub const CTX_CTRL_RS_CTX_ENABLE: u32 = bit(1); // intel_engine_regs.h:183
pub const CTX_CTRL_ENGINE_CTX_SAVE_INHIBIT: u32 = bit(2); // intel_engine_regs.h:184
pub const CTX_CTRL_INHIBIT_SYN_CTX_SWITCH: u32 = bit(3); // intel_engine_regs.h:185
/// `RING_MODE_GEN7(base)` (intel_engine_regs.h:192).
pub const fn ring_mode_gen7(base: u32) -> Reg {
    Reg::new(base + 0x29c)
}
pub const GFX_RUN_LIST_ENABLE: u32 = 1 << 15; // intel_engine_regs.h:193
/// `RING_CONTEXT_STATUS_PTR(base)` (intel_engine_regs.h:209): the CSB read (bits 10:8) / write (bits 2:0) pointers.
pub const fn ring_context_status_ptr(base: u32) -> Reg {
    Reg::new(base + 0x3a0)
}
