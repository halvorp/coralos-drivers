// SPDX-License-Identifier: GPL-2.0-only
//! i915-gt-core: the Gen9 / Kaby Lake graphics-technology core, as PURE PLANNERS. No register is read or written here: every
//! function returns data - workaround lists, MOCS words, engine tables, command dwords, interrupt decodes, effect lists and
//! state machines - and only `devices/i915` executes effects through `coralos-hal`.
//!
//! Ported from Linux drivers/gpu/drm/i915/gt/{intel_workarounds.c, intel_mocs.c, intel_sseu.c, intel_engine_cs.c,
//! gen8_engine_cs.c, intel_gt_irq.c, intel_reset.c} and intel_clock_gating.c at v7.0-rc6, restricted to Gen9 big-core parts
//! (Skylake / Kaby Lake). Copyright (c) Intel Corporation and the i915 authors.
//!
//! | module | Linux | what it gives a consumer |
//! |---|---|---|
//! | [`step`] | intel_step.[ch] | graphics stepping from the PCI revid; `IS_GRAPHICS_STEP` |
//! | [`wa`], [`wa_tables`] | intel_workarounds.c | context / GT / engine / whitelist lists as DATA, merge rule, apply plan, verify |
//! | [`mocs`] | intel_mocs.c | the Gen9 MOCS table and the control / L3CC write iterators |
//! | [`sseu`] | intel_sseu.c | fuse topology (devinfo) + MCR steering choice |
//! | [`engine`] | intel_engine_cs.c | RCS0/BCS0/VCS0/VCS1/VECS0: mmio base, reset domain, context size |
//! | [`emit`] | gen8_engine_cs.[ch] | flush / bb_start / breadcrumb / LRI dword builders |
//! | [`irq`] | intel_gt_irq.c | Gen8 GT IIR decode, reset and postinstall plans |
//! | [`reset`] | intel_reset.c | RESET_CTL handshake + GDRST reset state machine, bounded |
//! | [`clock_gating`] | intel_clock_gating.c | `kbl_init_clock_gating` as RMW effects |
//!
//! Registers live in `i915-gt-regs-core` (intel_gt_regs.h, intel_engine_regs.h and the i915_reg.h registers GT code uses);
//! command encodings live in `i915-gpu-commands-core` (intel_gpu_commands.h). Nothing is re-typed here.

#![no_std]
#![forbid(unsafe_code)]

pub mod clock_gating;
pub mod effect;
pub mod emit;
pub mod engine;
pub mod error;
pub mod fixed;
pub mod irq;
pub mod mocs;
pub mod reset;
pub mod sseu;
pub mod step;
pub mod wa;
pub mod wa_tables;

pub use effect::{Effect, PollPlan, RmwPlan, WritePlan};
pub use error::{GtError, ResetStage};
pub use fixed::FixedVec;
