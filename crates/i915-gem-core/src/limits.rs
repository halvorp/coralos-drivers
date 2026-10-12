// SPDX-License-Identifier: GPL-2.0-only
//! Every bound this crate declares. Each one is ENFORCED by the code that owns the table and has a test (tests/bounds.rs) that
//! fails if the check is removed. Linux's tables are unbounded (xarrays, slabs); a planner holding fixed arrays must say so.

/// Sessions (`struct drm_file`) the device serves at once.
pub const MAX_SESSIONS: usize = 8;
/// Handle slots per session (`drm_file::object_idr`). A handle is `gen << 16 | (slot + 1)`.
pub const MAX_HANDLES: usize = 256;
/// Objects across all sessions.
pub const MAX_OBJECTS: usize = 512;
/// Contexts per session (`drm_i915_file_private::context_xa`); id 0 is the default context.
pub const MAX_CTX_PER_SESSION: usize = 16;
/// Contexts across all sessions.
pub const MAX_CONTEXTS: usize = 32;
/// Address spaces (`GEM_VM_CREATE` plus the per-context default ones).
pub const MAX_VMS: usize = 8;
/// User-visible VM ids per session (`drm_i915_file_private::vm_xa`).
pub const MAX_VM_IDS_PER_SESSION: usize = 8;
/// Slots of a context's engine map: `I915_EXEC_RING_MASK + 1` (gem/i915_gem_context.c:758-760).
pub const MAX_USER_ENGINES: usize = 64;
/// Physical engines a carried Gen9 row can have (`i915_uapi_core::device::MAX_UABI_ENGINES`).
pub const MAX_ENGINES: usize = 5;
/// Requests tracked between creation and retirement.
pub const MAX_ACTIVE: usize = 64;
/// Objects one execbuffer2 may reference. Smaller than the wire's 1024 (`ExecLimits::DEFAULT`): the planner keeps them on the stack.
pub const MAX_EXEC_OBJECTS: usize = 128;
/// Live mmap offsets (`i915_mmap_offset`) across all objects.
pub const MAX_MMAP_OFFSETS: usize = 64;
/// Live CPU-mapping leases.
pub const MAX_LEASES: usize = 64;
/// Hardware seqno observations kept: one per (engine, engine context).
pub const MAX_HW_SEQNOS: usize = MAX_CONTEXTS * MAX_ENGINES;
/// Default capacities of an address space's VMA table and page-table table (the const generics of `Gtt`).
pub const DEFAULT_VMAS: usize = 128;
pub const DEFAULT_TABLES: usize = 256;

/// `ARRAY_SIZE(ctx->hang_timestamp)` (gem/i915_gem_context_types.h:388).
pub const HANG_HISTORY: usize = 2;
/// `CONTEXT_FAST_HANG_JIFFIES` = 120 s (gem/i915_gem_context_types.h:389), in milliseconds.
pub const CONTEXT_FAST_HANG_MS: u64 = 120_000;
/// `I915_CLIENT_FAST_HANG_JIFFIES` = 60 s (i915_file_private.h:101), in milliseconds.
pub const CLIENT_FAST_HANG_MS: u64 = 60_000;
/// `I915_CLIENT_SCORE_HANG_FAST` (i915_file_private.h:100).
pub const CLIENT_SCORE_HANG_FAST: u32 = 1;
/// `I915_CLIENT_SCORE_CONTEXT_BAN` (i915_file_private.h:102).
pub const CLIENT_SCORE_CONTEXT_BAN: u32 = 3;
/// `I915_CLIENT_SCORE_BANNED` (i915_file_private.h:103).
pub const CLIENT_SCORE_BANNED: u32 = 9;
/// PORT ADDITION (not in Linux): a context that has hung this many times is banned however slowly it hangs. Linux bans only on two
/// hangs inside `CONTEXT_FAST_HANG_MS`, so a client could hang the GPU forever at one hang per 121 s; this bounds that loop.
pub const MAX_GUILTY_BEFORE_BAN: u32 = 16;
