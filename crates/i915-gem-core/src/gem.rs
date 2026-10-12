// SPDX-License-Identifier: GPL-2.0-only
//! [`Gem`]: the whole GEM state of one device - sessions and their handle tables, the object arena, contexts, address spaces,
//! the active-request table and the mmap tables - as one planner. Methods are split by Linux file across the sibling modules
//! (`create`, `context`, `vm`, `active`, `mman`, `exec`, `shrink`); this module owns the structure and the session lifecycle.
use crate::active::Active;
use crate::context::Context;
use crate::error::GemError;
use crate::ids::{handle_of, ObjId, SessionId, HANDLE_IDX_MASK, HANDLE_MAX_GEN};
use crate::table::{self, Quota, Slab};
use alloc::vec::Vec;
use crate::mman::MmapTables;
use crate::object::Object;
use crate::seam::{Effect, EffectSink};
use crate::vm::Vm;
use i915_uapi_core::UapiDevice;

/// Device-level GEM policy that is not in the `UapiDevice`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GemConfig {
    /// `pat_enabled()`: x86 PAT is active, so WC and UC CPU mappings can be offered (gem/i915_gem_mman.c:889-890, :895-898).
    pub pat_enabled: bool,
}

/// What the calling process may do (`capable(CAP_SYS_ADMIN)`, `capable(CAP_SYS_NICE)`, `perfmon_capable()`), decided by the
/// device from the session's capability grants.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SessionCaps {
    pub sys_admin: bool,
    pub sys_nice: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Session {
    pub id: SessionId,
    pub caps: SessionCaps,
    /// The handle table: a generation-tagged slab, grown on demand (RUNTIME-TABLES-HAVE-NO-CAP).
    pub handles: Slab<ObjId>,
    /// User-visible context id (index; `context_xa`, id 0 is the default context) -> index into `Gem::contexts`.
    pub ctx: Vec<Option<u32>>,
    /// User-visible VM id (`id - 1`; `vm_xa`, ids start at 1) -> VM table index.
    pub vm_ids: Vec<Option<u32>>,
    /// `file_priv->ban_score` and `hang_timestamp` (i915_file_private.h).
    pub ban_score: u32,
    pub hang_timestamp: Option<u64>,
    /// `file_priv->bsd_engine`: the video instance this client was balanced onto (-1 until first use).
    pub bsd_engine: Option<u8>,
}

/// The GEM state of one device. `V`/`T` are the VMA and page-table capacities `i915-memory-core` gives each address space; every table
/// of THIS crate is a growable collection (see `table.rs`), so a `Gem` is a handful of empty vectors until it is used.
pub struct Gem {
    pub(crate) dev: UapiDevice,
    pub(crate) cfg: GemConfig,
    pub(crate) quota: Quota,
    pub(crate) next_session: u64,
    pub(crate) sessions: Vec<Option<Session>>,
    pub(crate) objects: Slab<Object>,
    pub(crate) contexts: Vec<Option<Context>>,
    pub(crate) vms: Vec<Option<Vm>>,
    pub(crate) active: Active,
    pub(crate) mmap: MmapTables,
    /// `i915_reset_count(&i915->gpu_error)`.
    pub(crate) reset_count: u32,
}

/// Put `x` in the first free slot of `v` or append (charged, fallible). Returns the index.
pub(crate) fn place<T>(v: &mut Vec<Option<T>>, x: T, q: &mut Quota, table: &'static str) -> Result<usize, GemError> {
    if let Some(i) = v.iter().position(|e| e.is_none()) {
        v[i] = Some(x);
        return Ok(i);
    }
    table::push(v, Some(x), q, table)
}

impl Gem {
    pub fn new(dev: UapiDevice, cfg: GemConfig) -> Self {
        Gem {
            dev,
            cfg,
            quota: Quota::unlimited(),
            next_session: 1,
            sessions: Vec::new(),
            objects: Slab::new(u32::MAX),
            contexts: Vec::new(),
            vms: Vec::new(),
            active: Active::new(),
            mmap: MmapTables::new(),
            reset_count: 0,
        }
    }

    pub fn device(&self) -> &UapiDevice {
        &self.dev
    }

    /// Set the budget of NEW table entries this device may still create (a memory quota); `None` removes it. When it is spent every
    /// table-growing call refuses with `GEM-NOMEM` before anything exists. The same refusal is what a failed `try_reserve` produces.
    pub fn set_table_budget(&mut self, entries: Option<usize>) {
        self.quota.set(entries);
    }

    pub fn table_budget(&self) -> Option<usize> {
        self.quota.remaining()
    }

    /// The session that owns an object (`None` once it is freed).
    pub fn object_owner(&self, obj: ObjId) -> Option<SessionId> {
        let o = self.obj(obj)?;
        self.sessions.get(o.owner as usize)?.as_ref().map(|s| s.id)
    }

    /// Live objects.
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }

    /// Live sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.iter().flatten().count()
    }

    /// `i915_gem_context_open` + `drm_file` creation (gem/i915_gem_context.c:1742-1779): a new session with its default context 0
    /// on a new address space. `default_vm` is the executor's pages for that address space (`PpgttInit`).
    pub fn open_session(&mut self, caps: SessionCaps, default_vm: i915_memory_core::PpgttInit, sink: &mut dyn EffectSink) -> Result<SessionId, GemError> {
        let id = SessionId(self.next_session);
        let s = Session {
            id,
            caps,
            handles: Slab::new(HANDLE_MAX_GEN),
            ctx: Vec::new(),
            vm_ids: Vec::new(),
            ban_score: 0,
            hang_timestamp: None,
            bsd_engine: None,
        };
        let slot = place(&mut self.sessions, s, &mut self.quota, "sessions")?;
        // Default context 0. If it cannot be made the session does not exist (the xa_destroy path of the `err:` label, gem/i915_gem_context.c:1776-1781).
        match self.create_context_internal(slot, Some(0), crate::context::ProtoContext::default(), Some(default_vm), sink) {
            Ok(_) => {
                self.next_session += 1;
                Ok(id)
            }
            Err(e) => {
                self.sessions[slot] = None;
                Err(e)
            }
        }
    }

    /// `i915_gem_context_close` + `drm_gem_release` (gem/i915_gem_context.c:1784-1803): close every handle and every context of the
    /// session. Objects with unretired requests stay until [`Gem::retire`] frees them.
    pub fn close_session(&mut self, session: SessionId, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        let slot = self.session_slot(session)?;
        let n = self.session(slot).handles.slot_count();
        for i in 0..n as u32 {
            let gen = self.session(slot).handles.gen_of(i).map_or(0, |g| g.0);
            if let Some(obj) = self.session(slot).handles.get(i, gen).copied() {
                self.release_handle(slot, i, obj, sink);
            }
        }
        for i in 0..self.session(slot).ctx.len() {
            if let Some(c) = self.session(slot).ctx[i] {
                self.close_context_slot(c as usize);
            }
        }
        for i in 0..self.session(slot).vm_ids.len() {
            if let Some(v) = self.session_mut(slot).vm_ids[i].take() {
                self.vm_unref(v as usize);
            }
        }
        self.sessions[slot] = None;
        self.sweep(sink);
        Ok(())
    }

    pub(crate) fn session_slot(&self, id: SessionId) -> Result<usize, GemError> {
        self.sessions.iter().position(|s| matches!(s, Some(s) if s.id == id)).ok_or(GemError::SessionGone { session: id.0 })
    }

    pub(crate) fn session(&self, slot: usize) -> &Session {
        self.sessions[slot].as_ref().expect("session slot validated by session_slot")
    }

    pub(crate) fn session_mut(&mut self, slot: usize) -> &mut Session {
        self.sessions[slot].as_mut().expect("session slot validated by session_slot")
    }

    pub(crate) fn obj(&self, id: ObjId) -> Option<&Object> {
        self.objects.get(id.slot(), id.generation())
    }

    pub(crate) fn obj_mut(&mut self, id: ObjId) -> Option<&mut Object> {
        self.objects.get_mut(id.slot(), id.generation())
    }

    /// `drm_gem_object_lookup` over the caller's OWN handle table only (`filp->object_idr`). A handle that is valid in another
    /// session is reported as [`GemError::CrossSessionHandle`]: the lookup itself never reads another session's table for data.
    pub(crate) fn lookup(&self, slot: usize, handle: u32) -> Result<ObjId, GemError> {
        let idx = (handle & HANDLE_IDX_MASK).wrapping_sub(1);
        let gen = handle >> crate::ids::HANDLE_IDX_BITS;
        if handle & HANDLE_IDX_MASK != 0 {
            if let Some(obj) = self.session(slot).handles.get(idx, gen) {
                return Ok(*obj);
            }
        }
        // Not ours. Classify, for the log only: the answer's errno is -ENOENT in every case.
        if handle & HANDLE_IDX_MASK != 0 {
            for (other, s) in self.sessions.iter().enumerate() {
                if other != slot && matches!(s, Some(s) if s.handles.get(idx, gen).is_some()) {
                    return Err(GemError::CrossSessionHandle { handle });
                }
            }
            // An earlier occupant of this slot (its generation has since advanced, or the slot is retired for good): stale.
            if let Some((cur, dead)) = self.session(slot).handles.gen_of(idx) {
                if gen != 0 && (gen < cur || (dead && gen <= cur)) {
                    return Err(GemError::StaleHandle { handle });
                }
            }
        }
        Err(GemError::HandleNotInSession { handle })
    }

    /// Install a handle for `obj` in the session (`drm_gem_handle_create`, gem/i915_gem_create.c:79).
    pub(crate) fn install_handle(&mut self, slot: usize, obj: ObjId) -> Result<u32, GemError> {
        let s = self.sessions[slot].as_mut().expect("session slot validated");
        let (idx, gen) = s.handles.insert(obj, &mut self.quota, "handles", HANDLE_IDX_MASK as usize)?;
        Ok(handle_of(gen, idx))
    }

    /// Drop handle `idx` of the session (`drm_gem_handle_delete` -> `i915_gem_close_object`, gem/i915_gem_object.c:239-293): the
    /// slot's generation advances, mmap offsets are revoked for the session, and the object is freed once nothing else holds it.
    pub(crate) fn release_handle(&mut self, slot: usize, idx: u32, obj: ObjId, sink: &mut dyn EffectSink) {
        self.session_mut(slot).handles.remove(idx);
        let mut closed = false;
        if let Some(o) = self.obj_mut(obj) {
            o.handles = o.handles.saturating_sub(1);
            closed = o.handles == 0;
        }
        if closed {
            self.revoke_mmap_offsets(obj, sink); // i915_gem_close_object: drm_vma_node_revoke, gem/i915_gem_object.c:267-270
            self.try_release_object(obj, sink);
        }
    }

    /// Free every closed object that is now idle, every closed context with no requests, every VM with no holders
    /// (the work `i915_gem_flush_free_objects` and the context/VM kref releases do in Linux).
    pub(crate) fn sweep(&mut self, sink: &mut dyn EffectSink) {
        for i in 0..self.objects.slot_count() as u32 {
            if let Some((gen, o)) = self.objects.at(i) {
                if o.handles == 0 {
                    self.try_release_object(ObjId::new(gen, i), sink);
                }
            }
        }
        self.sweep_contexts(sink);
        self.sweep_vms(sink);
    }

    /// Free `obj` if it is closed, idle (no unretired request touches it), and unmapped (no lease). Unbinds it from every VM and
    /// releases its pages. `__i915_gem_free_object` (gem/i915_gem_object.c:386-406): never while the GPU may still write it.
    pub(crate) fn try_release_object(&mut self, id: ObjId, sink: &mut dyn EffectSink) {
        let Some(o) = self.obj(id).copied() else { return };
        if o.handles != 0 || o.leases != 0 || self.active.refs_object(id) {
            return;
        }
        self.unbind_everywhere(id, sink);
        if o.pages {
            sink.effect(Effect::FreePages { obj: id });
        }
        self.objects.remove(id.slot());
        sink.effect(Effect::ObjectFreed { obj: id });
    }
}
