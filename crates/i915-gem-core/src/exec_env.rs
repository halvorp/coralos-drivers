// SPDX-License-Identifier: GPL-2.0-only
//! `i915_uapi_core::ExecEnv` over this crate's session tables: what `validate_execbuf2` may ask about contexts, objects and
//! fences while it makes Linux's pre-execution refusals. Every lookup is through the CALLING session's own tables, so a handle
//! that belongs to another session is "no such object" here exactly as in `drm_gem_object_lookup`.
use crate::gem::Gem;
use crate::seam::SyncTables;
use i915_uapi_core::{ContextInfo, ExecEnv, ObjectInfo, SyncobjInfo};

/// A read-only view of one session, bound to one submission's context and ring selector.
pub struct ExecView<'a, const V: usize, const T: usize> {
    pub(crate) gem: &'a Gem<V, T>,
    pub(crate) slot: usize,
    pub(crate) vm: Option<u8>,
    pub(crate) ring_slot: u32,
    pub(crate) sync: &'a dyn SyncTables,
}

impl<const V: usize, const T: usize> ExecEnv for ExecView<'_, V, T> {
    fn context(&self, id: u32) -> Option<ContextInfo> {
        self.gem.context_info(self.slot, id, self.ring_slot)
    }

    fn object(&self, handle: u32) -> Option<ObjectInfo> {
        let id = self.gem.lookup(self.slot, handle).ok()?;
        let size = self.gem.obj(id)?.size;
        let bound_offset = self.vm.and_then(|vm| self.gem.bound_offset(crate::ids::VmId(vm), id));
        Some(ObjectInfo { size, bound_offset })
    }

    fn syncobj(&self, handle: u32) -> Option<SyncobjInfo> {
        self.sync.syncobj(handle)
    }

    fn sync_file(&self, token: u32) -> bool {
        self.sync.sync_file(token)
    }
}
