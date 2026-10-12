// SPDX-License-Identifier: GPL-2.0-only
//! i915-firmware-core: parse and validate the DMC (display microcontroller, "CSR") firmware package for Kaby
//! Lake, and turn it into a PLAN of register writes. This crate performs NO MMIO: the device applies the plan.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dmc.c, intel_dmc_regs.h and intel_display_regs.h
//! (the pinned reference `references/linux-ref`, v7.0-rc6); every constant carries FILE:LINE.
//! Copyright (c) Intel Corporation and the i915 authors.
//!
//! The firmware BLOB itself is Intel-licensed (LICENSE.i915: binary redistribution only, no reverse
//! engineering) and is deliberately NOT part of this repo; callers supply the bytes.
#![no_std]
extern crate alloc;

pub mod format;
pub mod plan;
pub mod select;

pub use format::{parse_package, DmcError, DmcId, Stepping};
pub use plan::{build_plan, LoadPlan, RegWrite, DC_STATE_DEBUG_MASK_SET};
pub use select::{kbl_firmware, FirmwareChoice};
