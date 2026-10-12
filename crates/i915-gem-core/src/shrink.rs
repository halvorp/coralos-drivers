// SPDX-License-Identifier: GPL-2.0-only
//! Purgeable-object reclaim: when the executor needs memory back it asks [`Gem::shrink`], which discards the backing store of
//! objects userspace marked `I915_MADV_DONTNEED` (and only those: this port has no swap to move WILLNEED contents to). Ported from
//! gem/i915_gem_shrinker.c (`can_release_pages` gem/i915_gem_shrinker.c:27, `drop_pages` gem/i915_gem_shrinker.c:41, `try_to_writeback` gem/i915_gem_shrinker.c:60, `i915_gem_shrink` gem/i915_gem_shrinker.c:104-259) and
//! gem/i915_gem_shmem.c (`shmem_shrink` gem/i915_gem_shmem.c:339, `shmem_truncate` gem/i915_gem_shmem.c:291), v7.0-rc6.
//!
//! The order is Linux's: the object must be idle (an active object cannot be unbound without `I915_SHRINK_ACTIVE`), it is unbound
//! from every address space, its pages are released, and a DONTNEED object is truncated, which makes it `__I915_MADV_PURGED`:
//! `madvise` then answers `retained = false` and any later use answers `-EFAULT`.
use crate::gem::Gem;
use crate::ids::ObjId;
use crate::object::Madv;
use crate::seam::{Effect, EffectSink};

/// What a shrink pass did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShrinkReport {
    pub purged: u32,
    pub pages_freed: u64,
    /// Purgeable objects left alone because the GPU may still use them.
    pub skipped_active: u32,
    /// Purgeable objects left alone because they are scanout buffers (`i915_gem_object_is_framebuffer`, gem/i915_gem_shrinker.c:207).
    pub skipped_framebuffer: u32,
}

impl<const V: usize, const T: usize> Gem<V, T> {
    /// `i915_gem_shrink` over the purge list: release purgeable objects until `target_pages` are free or none are left.
    pub fn shrink(&mut self, target_pages: u64, sink: &mut dyn EffectSink) -> ShrinkReport {
        let mut rep = ShrinkReport::default();
        for i in 0..self.objects.slot_count() as u32 {
            if rep.pages_freed >= target_pages {
                break; // `count < target`, gem/i915_gem_shrinker.c:197
            }
            let Some((gen, o)) = self.objects.at(i).map(|(g, o)| (g, *o)) else { continue };
            if o.madv != Madv::DontNeed || !o.pages {
                continue; // can_release_pages: only DONTNEED objects with pages, gem/i915_gem_shrinker.c:27-38
            }
            let id = ObjId::new(gen, i);
            if o.framebuffer {
                rep.skipped_framebuffer += 1; // gem/i915_gem_shrinker.c:207-208
                continue;
            }
            if self.active.refs_object(id) {
                rep.skipped_active += 1; // drop_pages without UNBIND_ACTIVE fails with -EBUSY, gem/i915_gem_shrinker.c:41-58
                continue;
            }
            self.unbind_everywhere(id, sink); // i915_gem_object_unbind, gem/i915_gem_shrinker.c:54
            if o.leases != 0 {
                sink.effect(Effect::RevokeMmap { obj: id }); // truncate zaps the mappings (shmem_truncate_range, gem/i915_gem_shmem.c:300)
            }
            sink.effect(Effect::FreePages { obj: id }); // __i915_gem_object_put_pages, gem/i915_gem_shrinker.c:229
            if let Some(o) = self.obj_mut(id) {
                o.pages = false;
                o.madv = Madv::Purged; // shmem_shrink -> shmem_truncate, gem/i915_gem_shmem.c:339-347, gem/i915_gem_shmem.c:301
            }
            rep.purged += 1;
            rep.pages_freed += o.size / i915_memory_core::GTT_PAGE;
        }
        rep
    }
}
