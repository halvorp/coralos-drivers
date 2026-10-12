// SPDX-License-Identifier: GPL-2.0-only
//! i915-common-core: the CONTRACT every i915 port crate builds on (frozen as `CONTRACT_ABI` = 1; see docs/I915-CONTRACTS.md).
//!
//! 1. MMIO access is `coralos_hal::Mmio` and nothing else - no crate in the i915 family touches a pointer. Registers are
//!    named by [`Reg`], never by a bare integer, and read/written through [`RegIo`].
//! 2. Registers have ONE HOME: the crate named for the Linux header that defines them (`i915-gt-regs-core` for
//!    gt/intel_gt_regs.h and the GT-visible part of intel_mchbar_regs.h). Another crate imports the constant; it never
//!    re-types the offset.
//! 3. Every failure is an [`I915Error`] variant with a stable name ([`I915Error::name`]); nothing refuses with a bare `false`.
//! 4. The ONLY way a device writes the GPU is through a [`WriteGuard`] built over a `&'static` allow-list of registers.
//!
//! Ported from Linux drivers/gpu/drm/i915/i915_reg_defs.h at v7.0-rc6. Copyright (c) Intel Corporation.

#![no_std]
#![forbid(unsafe_code)]

pub mod error;
pub mod guard;
pub mod reg;

pub use error::I915Error;
pub use guard::{WriteGuard, WriteRefusal};
pub use reg::{Reg, RegIo};

/// The contract revision. Bumped (and docs/I915-CONTRACTS.md amended) by any change to a frozen shape; the marker the
/// device prints is `[I915-CONTRACT abi=1 ...]`.
pub const CONTRACT_ABI: u32 = 1;
