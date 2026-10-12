// SPDX-License-Identifier: GPL-2.0-only
//! PRIME sharing and the slot accessors the executor needs. Linux shares a GEM object between `drm_file`s through a dma-buf that holds
//! a reference on the object (`drm_gem_prime_handle_to_fd` -> `drm_gem_dmabuf_export`, drm_prime.c; `drm_gem_prime_fd_to_handle` ->
//! `drm_gem_handle_create`, drm_prime.c). Here the DEVICE keeps the token table (it is `devices/i915`'s: KBL-I915-PORT-PLAN section 2)
//! and this crate supplies the three object-level operations: HOLD an object (the dma-buf's reference), IMPORT a held object into another
//! session's handle table, and RELEASE the hold. The object's pages and its bindings are shared; each session binds it in its own VMs.
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::{ObjId, SessionId};
use crate::seam::EffectSink;

impl Gem {
    /// The global slot of a live (not closed) context: what `Effect::ContextFreed { ctx_slot }` will name later.
    pub fn context_slot_of(&self, session: SessionId, ctx_id: u32) -> Result<u32, GemError> {
        let slot = self.session_slot(session)?;
        self.context_slot(slot, ctx_id).map(|c| c as u32)
    }

    /// Slots in the address-space table (live or free): the bound of a scan for a `VmId`.
    pub fn vm_slots(&self) -> usize {
        self.vms.len()
    }

    /// `dma_buf` export: the caller's object gains a hold that outlives the caller's handle. Returns the object.
    pub fn prime_export(&mut self, session: SessionId, handle: u32) -> Result<ObjId, GemError> {
        let id = self.lookup_object(session, handle)?;
        if let Some(o) = self.obj_mut(id) {
            o.handles = o.handles.saturating_add(1);
        }
        Ok(id)
    }

    /// Drop a hold taken by [`Gem::prime_export`] (the dma-buf was released, or its exporter died).
    pub fn prime_release(&mut self, obj: ObjId, sink: &mut dyn EffectSink) {
        let mut closed = false;
        if let Some(o) = self.obj_mut(obj) {
            o.handles = o.handles.saturating_sub(1);
            closed = o.handles == 0;
        }
        if closed {
            self.revoke_mmap_offsets(obj, sink);
            self.try_release_object(obj, sink);
        }
    }

    /// `drm_gem_prime_fd_to_handle`: a NEW handle in `session` for a held object. The object keeps its first owner for mmap policy.
    pub fn prime_import(&mut self, session: SessionId, obj: ObjId) -> Result<u32, GemError> {
        let slot = self.session_slot(session)?;
        match self.obj(obj) {
            Some(o) if o.handles > 0 => {}
            _ => return Err(GemError::HandleNotInSession { handle: obj.index() }),
        }
        let h = self.install_handle(slot, obj)?;
        if let Some(o) = self.obj_mut(obj) {
            o.handles += 1;
        }
        Ok(h)
    }
}
