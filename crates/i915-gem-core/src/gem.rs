// SPDX-License-Identifier: GPL-2.0-only
//! [`Gem`]: the whole GEM state of one device - sessions and their handle tables, the object arena, contexts, address spaces,
//! the active-request table and the mmap tables - as one planner. Methods are split by Linux file across the sibling modules
//! (`create`, `context`, `vm`, `active`, `mman`, `exec`, `shrink`); this module owns the structure and the session lifecycle.
use crate::active::Active;
use crate::context::Context;
use crate::error::GemError;
use crate::ids::{handle_of, ObjId, SessionId};
use crate::limits::*;
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

#[derive(Clone, Copy, Debug)]
pub(crate) struct HandleSlot {
    /// Generation of the NEXT (or current) occupant; starts at 1 and advances on every free.
    pub gen: u16,
    /// The generation counter wrapped: this slot is never used again (a stale handle must stay stale).
    pub dead: bool,
    pub obj: Option<ObjId>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ObjSlot {
    pub gen: u16,
    pub dead: bool,
    pub obj: Option<Object>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Session {
    pub id: SessionId,
    pub caps: SessionCaps,
    pub handles: [HandleSlot; MAX_HANDLES],
    /// User-visible context id -> index into `Gem::contexts` (`context_xa`; id 0 is the default context).
    pub ctx: [Option<u8>; MAX_CTX_PER_SESSION],
    /// User-visible VM id (`id - 1`) -> VM arena slot (`vm_xa`, ids start at 1).
    pub vm_ids: [Option<u8>; MAX_VM_IDS_PER_SESSION],
    /// `file_priv->ban_score` and `hang_timestamp` (i915_file_private.h).
    pub ban_score: u32,
    pub hang_timestamp: Option<u64>,
    /// `file_priv->bsd_engine`: the video instance this client was balanced onto (-1 until first use).
    pub bsd_engine: Option<u8>,
}

/// The GEM state of one device. `V`/`T` are the VMA and page-table capacities of each address space.
pub struct Gem<const V: usize = DEFAULT_VMAS, const T: usize = DEFAULT_TABLES> {
    pub(crate) dev: UapiDevice,
    pub(crate) cfg: GemConfig,
    pub(crate) next_session: u64,
    pub(crate) sessions: [Option<Session>; MAX_SESSIONS],
    pub(crate) objects: [ObjSlot; MAX_OBJECTS],
    pub(crate) contexts: [Option<Context>; MAX_CONTEXTS],
    pub(crate) vms: [Option<Vm<V, T>>; MAX_VMS],
    pub(crate) active: Active,
    pub(crate) mmap: MmapTables,
    /// `i915_reset_count(&i915->gpu_error)`.
    pub(crate) reset_count: u32,
}

impl<const V: usize, const T: usize> Gem<V, T> {
    pub fn new(dev: UapiDevice, cfg: GemConfig) -> Self {
        Gem {
            dev,
            cfg,
            next_session: 1,
            sessions: core::array::from_fn(|_| None),
            objects: [ObjSlot { gen: 1, dead: false, obj: None }; MAX_OBJECTS],
            contexts: core::array::from_fn(|_| None),
            vms: core::array::from_fn(|_| None),
            active: Active::new(),
            mmap: MmapTables::new(),
            reset_count: 0,
        }
    }

    pub fn device(&self) -> &UapiDevice {
        &self.dev
    }

    /// The session that owns an object (`None` once it is freed).
    pub fn object_owner(&self, obj: ObjId) -> Option<SessionId> {
        let o = self.obj(obj)?;
        self.sessions.get(o.owner as usize)?.as_ref().map(|s| s.id)
    }

    /// `i915_gem_context_open` + `drm_file` creation (gem/i915_gem_context.c:1742-1779): a new session with its default context 0
    /// on a new address space. `default_vm` is the executor's pages for that address space (`PpgttInit`).
    pub fn open_session(&mut self, caps: SessionCaps, default_vm: i915_memory_core::PpgttInit, sink: &mut dyn EffectSink) -> Result<SessionId, GemError> {
        let slot = self.sessions.iter().position(|s| s.is_none()).ok_or(GemError::SessionTableFull { cap: MAX_SESSIONS })?;
        let id = SessionId(self.next_session);
        let s = Session {
            id,
            caps,
            handles: [HandleSlot { gen: 1, dead: false, obj: None }; MAX_HANDLES],
            ctx: [None; MAX_CTX_PER_SESSION],
            vm_ids: [None; MAX_VM_IDS_PER_SESSION],
            ban_score: 0,
            hang_timestamp: None,
            bsd_engine: None,
        };
        self.sessions[slot] = Some(s);
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

    /// `i915_gem_context_close` + `drm_gem_release` (gem/i915_gem_context.c:1784-1803-): close every handle and every context of the
    /// session. Objects with unretired requests stay until [`Gem::retire`] frees them.
    pub fn close_session(&mut self, session: SessionId, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        let slot = self.session_slot(session)?;
        for i in 0..MAX_HANDLES {
            if let Some(obj) = self.sessions[slot].as_ref().and_then(|s| s.handles[i].obj) {
                self.release_handle(slot, i, obj, sink);
            }
        }
        for id in 0..MAX_CTX_PER_SESSION {
            if let Some(c) = self.sessions[slot].as_ref().and_then(|s| s.ctx[id]) {
                self.close_context_slot(c as usize);
            }
        }
        for id in 0..MAX_VM_IDS_PER_SESSION {
            if let Some(v) = self.sessions[slot].as_mut().and_then(|s| s.vm_ids[id].take()) {
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
        let s = self.objects.get(id.slot())?;
        if s.gen == id.generation() {
            s.obj.as_ref()
        } else {
            None
        }
    }

    pub(crate) fn obj_mut(&mut self, id: ObjId) -> Option<&mut Object> {
        let s = self.objects.get_mut(id.slot())?;
        if s.gen == id.generation() {
            s.obj.as_mut()
        } else {
            None
        }
    }

    /// `drm_gem_object_lookup` over the caller's OWN handle table only (`filp->object_idr`). A handle that is valid in another
    /// session is reported as [`GemError::CrossSessionHandle`]: the lookup itself never reads another session's table for data.
    pub(crate) fn lookup(&self, slot: usize, handle: u32) -> Result<ObjId, GemError> {
        let own = self.session(slot);
        let idx = (handle & 0xffff).wrapping_sub(1) as usize;
        let gen = (handle >> 16) as u16;
        if let Some(h) = own.handles.get(idx) {
            if let (Some(obj), true) = (h.obj, h.gen == gen) {
                return Ok(obj);
            }
        }
        // Not ours. Classify, for the log only: the answer's errno is -ENOENT in every case.
        for (other, s) in self.sessions.iter().enumerate() {
            if other == slot {
                continue;
            }
            if let Some(s) = s {
                if let Some(h) = s.handles.get(idx) {
                    if h.obj.is_some() && h.gen == gen {
                        return Err(GemError::CrossSessionHandle { handle });
                    }
                }
            }
        }
        match own.handles.get(idx) {
            // An earlier occupant of this slot (its generation has since advanced, or the slot is retired for good): stale.
            Some(h) if gen != 0 && (gen < h.gen || (h.dead && gen <= h.gen)) => Err(GemError::StaleHandle { handle }),
            _ => Err(GemError::HandleNotInSession { handle }),
        }
    }

    /// Install a handle for `obj` in the session (`drm_gem_handle_create`, gem/i915_gem_create.c:79).
    pub(crate) fn install_handle(&mut self, slot: usize, obj: ObjId) -> Result<u32, GemError> {
        let s = self.session_mut(slot);
        let i = s.handles.iter().position(|h| h.obj.is_none() && !h.dead).ok_or(GemError::HandleTableFull { cap: MAX_HANDLES })?;
        s.handles[i].obj = Some(obj);
        Ok(handle_of(s.handles[i].gen, i))
    }

    /// Drop handle `i` of the session (`drm_gem_handle_delete` -> `i915_gem_close_object`, gem/i915_gem_object.c:239-293): the
    /// slot's generation advances, mmap offsets are revoked for the session, and the object is freed once nothing else holds it.
    pub(crate) fn release_handle(&mut self, slot: usize, i: usize, obj: ObjId, sink: &mut dyn EffectSink) {
        let s = self.session_mut(slot);
        let h = &mut s.handles[i];
        h.obj = None;
        match h.gen.checked_add(1) {
            Some(g) => h.gen = g,
            None => h.dead = true,
        }
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
        for i in 0..MAX_OBJECTS {
            let s = self.objects[i];
            if let Some(o) = s.obj {
                if o.handles == 0 {
                    self.try_release_object(ObjId::new(s.gen, i as u16), sink);
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
        let s = &mut self.objects[id.slot()];
        s.obj = None;
        match s.gen.checked_add(1) {
            Some(g) => s.gen = g,
            None => s.dead = true,
        }
        sink.effect(Effect::ObjectFreed { obj: id });
    }
}
