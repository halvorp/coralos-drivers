// SPDX-License-Identifier: GPL-2.0-only
//! i915-gem-core: GEM for Kaby Lake as PURE planners - session-scoped object handles and lifetime, contexts and address spaces,
//! EXECBUFFER2 execution planning (softpin placement with eviction, relocation, request construction, fences), busy/wait on
//! seqno, mmap policy, madvise/purge, and hang/ban accounting. Nothing here touches MMIO or memory: backing pages are REQUESTED
//! from the caller through [`PageSource`], and everything that must happen to the world is an [`Effect`] or a bind/unbind plan
//! handed to an [`EffectSink`]. `devices/i915` is the only thing that executes them.
//!
//! Ported from Linux drivers/gpu/drm/i915 at v7.0-rc6: gem/{i915_gem_object.c, i915_gem_create.c, i915_gem_execbuffer.c,
//! i915_gem_context.c, i915_gem_mman.c, i915_gem_wait.c, i915_gem_busy.c, i915_gem_domain.c, i915_gem_tiling.c,
//! i915_gem_shrinker.c, i915_gem_userptr.c}, i915_gem.c (madvise) and gt/intel_reset.c (guilty accounting).
//! Copyright (c) Intel Corporation and the i915 authors.
//!
//! | module | Linux | what it gives a consumer |
//! |---|---|---|
//! | [`gem`] | drm_file, drm_gem.c | [`Gem`]: sessions, session-scoped handles (cross-session use is a NAMED refusal) |
//! | [`create`] | i915_gem_create.c, i915_gem_object.c, i915_gem.c, i915_gem_domain.c, i915_gem_tiling.c | create/close, madvise, caching, set_domain, tiling, userptr REFUSED |
//! | [`context`] | i915_gem_context.c | contexts, engines map, params, VM per context |
//! | [`vm`] | i915_gem_context.c (VM ioctls), i915_vma.c | address spaces over `i915-memory-core::Gtt` |
//! | [`exec`] | i915_gem_execbuffer.c | `execbuffer2`: bind plan + request + ELSP plan |
//! | [`active`] | i915_gem_busy.c, i915_gem_wait.c, i915_request.c | busy, wait, retire on seqno |
//! | [`hang`] | gt/intel_reset.c | guilty/innocent, context and client ban |
//! | [`mman`] | i915_gem_mman.c | mmap offset policy and CPU-mapping leases |
//! | [`shrink`] | i915_gem_shrinker.c, i915_gem_shmem.c | purgeable-object reclaim |
//!
//! WHAT IS CARRIED: Gen9 (SKL/KBL) system-memory objects on an LLC part, full PPGTT, execlists. NOT PORTED, by name: userptr
//! (refused), local memory and placements other than SYSTEM, protected content (PXP), GuC, GVT, flink/PRIME (the device's session
//! tables), perf/OA, the Gen6/7 GGTT-global execbuffer paths, the command parser, parallel submission, error capture.
//!
//! BOUNDS (each enforced and tested, see `limits.rs`): sessions, handles per session, objects, contexts, VMs, engine-map slots,
//! active requests, objects per execbuffer, mmap offsets, leases, hardware-seqno observations, the two-slot hang history and the
//! client ban score.

#![no_std]
#![forbid(unsafe_code)]

pub mod active;
pub mod context;
pub mod create;
pub mod error;
pub mod exec;
pub mod exec_env;
pub mod gem;
pub mod hang;
pub mod ids;
pub mod limits;
pub mod mman;
pub mod object;
pub mod seam;
pub mod shrink;
pub mod vm;

pub use active::{wait_finish, Wait, WaitTimeout};
pub use context::{ContextParam, ParamValue, UserEngine};
pub use create::{CreateExt, Created, TilingReply};
pub use error::GemError;
pub use exec::ExecDone;
pub use exec_env::ExecView;
pub use gem::{Gem, GemConfig, SessionCaps};
pub use hang::{ResetStats, ResetVerdict};
pub use ids::{ObjId, RequestId, SessionId, VmId};
pub use mman::{CpuCache, MappingLease, MmapGrant};
pub use object::{Madv, MmapKind, ObjectState};
pub use seam::{Effect, EffectSink, EngineSet, PageRequest, PageSource, PagesRefused, SyncTables};
pub use shrink::ShrinkReport;
