// SPDX-License-Identifier: GPL-2.0-only
//! i915-submit-core: execlists submission for Gen9 / Kaby Lake as PURE state machines. No register is read or written here:
//! every function returns data - context images, descriptors, ring dwords, ELSP write lists, CSB decodes, retirement counts
//! and named hang verdicts - and only `devices/i915` executes the effects and owns the memory.
//!
//! Ported from Linux drivers/gpu/drm/i915/gt/{intel_lrc.c, intel_lrc.h, intel_lrc_reg.h, intel_execlists_submission.c,
//! intel_ring.c, intel_ring.h, intel_ring_types.h, intel_timeline.c, intel_engine.h, intel_sseu.c} and i915_request.c at
//! v7.0-rc6, restricted to the execlists path (no GuC, no legacy ringbuffer) on Gen9 big-core parts. Gen8 context tables and
//! the Gen8 workaround batch are carried as data. Copyright (c) Intel Corporation and the i915 authors.
//!
//! | module | Linux | what it gives a consumer |
//! |---|---|---|
//! | [`lrc_reg`] | intel_lrc_reg.h, intel_lrc.h, top of intel_execlists_submission.c, intel_engine.h | context-image dword indices, descriptor bits, CSB status bits, HWSP dword map |
//! | [`lrc_offsets`] | intel_lrc.c:49-770 | the Gen8/Gen9 RCS and XCS register-state tables as data, `set_offsets`, slot locators |
//! | [`lrc_state`] | intel_lrc.c:772-1042, :1521-1574 | `init_regs`, `update_regs`, `descriptor` over a 4 KiB [`lrc_state::RegState`] |
//! | [`wa_bb`] | intel_lrc.c:1636-1930 | the Gen8/Gen9 indirect-context workaround batch |
//! | [`rpcs`] | intel_sseu.c:654-760 | `intel_sseu_make_rpcs` for Gen9 (the render context's RPCS dword) |
//! | [`ring`] | intel_ring.[ch] | `Ring`: space, wrap, tail, `begin` with wrap fill, refusing where Linux would wait |
//! | [`request`] | i915_request.h, intel_timeline.c | `seqno_passed`, `Timeline`, `Request`, the context-tag allocator |
//! | [`execlists`] | intel_execlists_submission.c | `Execlists` port/CSB tracker, ELSP write order, CSB reset and resume plans |
//! | [`engine`] | i915_request.c, intel_lrc.c, execlists_request_alloc | `Engine`: contexts and request construction |
//! | [`submit`] | intel_execlists_submission.c | dequeue/submit, CSB consumption, retire, the stall bound |
//!
//! Registers live in `i915-gt-regs-core` (intel_engine_regs.h, intel_gt_regs.h); command encodings in `i915-gpu-commands-core`;
//! the emitters and `Effect`/`WritePlan` in `i915-gt-core`; the PML4 comes from `i915-memory-core`. Nothing is re-typed here.

#![no_std]
#![forbid(unsafe_code)]

pub mod engine;
pub mod error;
pub mod execlists;
pub mod lrc_offsets;
pub mod lrc_reg;
pub mod lrc_state;
pub mod request;
pub mod ring;
pub mod rpcs;
pub mod submit;
pub mod wa_bb;

pub use engine::{BatchSpec, ContextSpec, Engine, EngineConfig, RequestRef};
pub use error::SubmitError;
pub use execlists::{Execlists, Port};
pub use submit::{ImageStore, SubmissionPlan};
