// SPDX-License-Identifier: GPL-2.0-only
//! What this crate still fixes at compile time, and WHY. Doctrine RUNTIME-TABLES-HAVE-NO-CAP (owner ruling, 2026-10-12): a table that
//! tracks a runtime-growing population (sessions, handles, objects, contexts, VMs, VMAs, requests, leases) grows with memory and is
//! never sized by a constant here. Everything below is either defined by hardware or by an ABI, or is a policy threshold, not a table.

/// Physical engines a carried Gen9 row can have: RCS0 + BCS0 + VCS0..1 + VECS0 (`i915_uapi_core::device::MAX_UABI_ENGINES`).
/// HARDWARE-bounded: the engine set is fused into the part (gt/intel_engine_cs.c `intel_engines_init`).
pub const MAX_ENGINES: usize = 5;
/// Slots of a context's engine map: `I915_EXEC_RING_MASK + 1` (gem/i915_gem_context.c:758-760). ABI-bounded: the ring selector of
/// `execbuffer2.flags` is a 6-bit field (i915_drm.h:1431), so slot 64 cannot be named.
pub const MAX_USER_ENGINES: usize = 64;
/// Default capacities of an address space's VMA table and page-table table: the const generics of `i915_memory_core::Gtt`. Those
/// arrays belong to `i915-memory-core`; this crate only chooses their size per VM.
pub const DEFAULT_VMAS: usize = 128;
pub const DEFAULT_TABLES: usize = 256;

/// `ARRAY_SIZE(ctx->hang_timestamp)` (gem/i915_gem_context_types.h:388): a history of the last two hangs, a struct layout.
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
/// PORT ADDITION (not in Linux): a context that has hung this many times is banned however slowly it hangs. A POLICY THRESHOLD, not a
/// table size. Linux bans only on the third hang inside `CONTEXT_FAST_HANG_MS`, so a client could hang the GPU forever at one hang
/// per 121 s; this bounds that loop.
pub const MAX_GUILTY_BEFORE_BAN: u32 = 16;
