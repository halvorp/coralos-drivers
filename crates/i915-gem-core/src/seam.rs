// SPDX-License-Identifier: GPL-2.0-only
//! The seams to the executor (`devices/i915`): everything this crate cannot do or know itself is a typed request or an effect.
//! The planner never allocates memory, never touches a register, and never learns a CPU address.
use crate::ids::{ObjId, RequestId, VmId};
use i915_memory_core::{BindPlan, CacheLevel, UnbindPlan};
use i915_submit_core::{BatchSpec, RequestRef, SubmissionPlan, SubmitError};
use i915_uapi_core::SyncobjInfo;

/// A request for the backing pages of one object (`shmem_get_pages`, gem/i915_gem_shmem.c). The executor allocates
/// `bytes / 4096` pages through `coralos_hal::Dma::alloc` (zeroed, as `I915_BO_ALLOC_USER` requires, gem/i915_gem_create.c:124) and
/// keeps them until the planner emits [`Effect::FreePages`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageRequest {
    pub obj: ObjId,
    pub bytes: u64,
    pub cache: CacheLevel,
}

/// The executor refused the allocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PagesRefused;

/// Where object pages come from. Mirrors `i915_memory_core::TablePageSource`, but the pages are kept by the source: the planner
/// asks once ([`PageSource::alloc_pages`]) and then reads the DEVICE addresses ([`PageSource::pages`]) when it plans a bind.
pub trait PageSource {
    /// Make `req.bytes / 4096` zeroed pages resident for `req.obj`. Idempotent for an object that already has them.
    fn alloc_pages(&mut self, req: PageRequest) -> Result<(), PagesRefused>;
    /// The device address of each page of a resident object, in order; `None` when the object has none.
    fn pages(&self, obj: ObjId) -> Option<&[u64]>;
}

/// Engine-side services: request construction and the ELSP dequeue, over `i915_submit_core::Engine` per physical engine.
pub trait EngineSet {
    /// `Engine::build_request` on the engine `(class, instance)` for the engine context `engine_ctx` (flush, initial breadcrumb,
    /// `emit_bb_start`, fini breadcrumb). The implementation owns the ring's CPU mapping.
    fn build_request(&mut self, class: u16, instance: u16, engine_ctx: u32, batch: &BatchSpec) -> Result<RequestRef, SubmitError>;
    /// `Engine::dequeue` (`execlists_dequeue`): the ELSP plan if a submission is due.
    fn dequeue(&mut self, class: u16, instance: u16, now: u64) -> Result<Option<SubmissionPlan>, SubmitError>;
}

/// Syncobj / sync_file state the device keeps (generic `drm_syncobj.c` semantics; KBL-I915-PORT-PLAN section 2).
pub trait SyncTables {
    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo>;
    /// `sync_file_get_fence(fd)` for the flattened in-fence token.
    fn sync_file(&self, token: u32) -> bool;
}

/// Something the executor must do. All variants are `Copy` and small; plans that carry tables go through [`EffectSink`]'s
/// dedicated methods instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Release the backing pages of `obj` (`shmem_put_pages`, or the truncate of a purge, gem/i915_gem_shmem.c:290-302).
    FreePages { obj: ObjId },
    /// The object slot is free; nothing refers to it any more (`__i915_gem_free_object`, gem/i915_gem_object.c:386).
    ObjectFreed { obj: ObjId },
    /// `i915_gem_clflush_object` before the GPU reads a CPU-dirty, non-coherent object (gem/i915_gem_execbuffer.c:2117-2120).
    Clflush { obj: ObjId },
    /// Patch a relocation: write `value` (`width` bytes, little-endian) at `byte_offset` of `owner`'s pages, and report
    /// `presumed` back as `reloc.presumed_offset` for the user's `reloc_index`-th relocation (`relocate_entry`,
    /// gem/i915_gem_execbuffer.c:1409-1509).
    PatchReloc { owner: ObjId, byte_offset: u64, value: u64, width: u8, reloc_index: u32, presumed: u64 },
    /// `rq` must not start before `on` completes (`i915_request_await_object`, i915_request.c).
    AwaitRequest { rq: RequestId, on: RequestId },
    /// `rq` waits on a syncobj fence (`await_fence_array`, gem/i915_gem_execbuffer.c:3000-3016); `point` is the timeline point or 0.
    AwaitSyncobj { rq: RequestId, handle: u32, point: u64 },
    /// `rq`'s completion signals a syncobj (`signal_fence_array`, gem/i915_gem_execbuffer.c:3018-3045).
    SignalSyncobj { rq: RequestId, handle: u32, point: u64 },
    /// `rq` waits for the in-fence token to complete (`submit == false`, `FENCE_IN`) or to be submitted (`FENCE_SUBMIT`).
    AwaitInFence { rq: RequestId, token: u32, submit: bool },
    /// Create a sync_file for `rq` (`I915_EXEC_FENCE_OUT`, gem/i915_gem_execbuffer.c:3266-3270).
    OutFence { rq: RequestId },
    /// Write this submission to the ELSP (after its image stores).
    Submit { engine: u8, plan: SubmissionPlan },
    /// Revoke every CPU mapping of `obj` (`i915_gem_object_release_mmap_*`).
    RevokeMmap { obj: ObjId },
    /// A context left the table (`context_close` finished); the executor may drop its engine contexts.
    ContextFreed { ctx_slot: u32 },
    /// A VM left the table; the executor may free its page-table pages.
    VmFreed { vm: VmId },
}

/// Receives the effects, in order. Plans are handed over by reference and must be consumed (`BindPlan::writes`/`flushes`) before
/// returning: this is how a bind of many megabytes never has to be stored.
pub trait EffectSink {
    /// A bind of `obj` into `vm`: ops, entry stores, flushes (and the victims it evicted, `plan.evicted()`).
    fn bind(&mut self, vm: VmId, obj: ObjId, plan: &BindPlan<'_>);
    /// An unbind (including the initial clear of a new VM).
    fn unbind(&mut self, vm: VmId, plan: &UnbindPlan);
    fn effect(&mut self, e: Effect);
}
