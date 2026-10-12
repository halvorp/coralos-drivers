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
