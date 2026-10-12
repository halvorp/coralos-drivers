// SPDX-License-Identifier: GPL-2.0-only
//! CPU mapping POLICY: which object may be mapped how, and who holds a mapping. No mapping is made here. GEM CPU mmap on a
//! microkernel is a shared-memory LEASE, not a page fault (KBL-I915-PORT-PLAN section 2): the executor maps the object's pages into
//! the session under a lease this table records, with the CPU cache mode this policy names. Closing a handle does NOT revoke an
//! outstanding lease (gem/i915_gem_mman.c:921-928: `vm_open` holds the object); the object is freed only when the last lease ends.
//! Ported from gem/i915_gem_mman.c (v7.0-rc6): `i915_gem_mmap_ioctl` (gem/i915_gem_mman.c:60), `mmap_offset_attach` (gem/i915_gem_mman.c:719), `__assign_mmap_offset`
//! (gem/i915_gem_mman.c:772), `i915_gem_mmap_offset_ioctl` (gem/i915_gem_mman.c:865), and gem/i915_gem_object.c:239-293 (`i915_gem_close_object`).
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::{ObjId, SessionId};
use crate::table;
use alloc::vec::Vec;
use crate::object::{MmapKind, Madv};
use crate::seam::{EffectSink, PageSource};
use i915_uapi_core::consts::misc as m;

/// `I915_MMAP_WC` of the legacy `GEM_MMAP` (i915_drm.h:988).
pub const I915_MMAP_WC: u64 = 0x1; // i915_drm.h:988
/// `DRM_FILE_PAGE_OFFSET_START`: the first fake offset the vma manager hands out (drm_vma_manager.h): 4 GiB on 64-bit.
pub const FAKE_OFFSET_START: u64 = 0x1_0000_0000;

/// The CPU cache attribute the executor must give the mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CpuCache {
    /// Write-back, cacheable: coherent with the GPU on an LLC part (the KBL PAT's `WB` entry).
    Wb,
    /// Write-combining through PAT.
    Wc,
    /// Uncached through PAT.
    Uc,
    /// Through the GGTT aperture (BAR2): inherently write-combining. Only where an aperture exists.
    Aperture,
}

impl MmapKind {
    pub const fn cpu_cache(self) -> CpuCache {
        match self {
            MmapKind::Wb => CpuCache::Wb,
            MmapKind::Wc => CpuCache::Wc,
            MmapKind::Uc => CpuCache::Uc,
            MmapKind::Gtt | MmapKind::Fixed => CpuCache::Aperture,
        }
    }
}

/// One assigned mmap offset (`struct i915_mmap_offset`): the session may map `obj` of this kind at this fake offset.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MmapOffset {
    pub obj: ObjId,
    pub kind: MmapKind,
    pub offset: u64,
    pub session: u32,
}

/// A live CPU-mapping lease.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Lease {
    pub id: u32,
    pub obj: ObjId,
}

pub(crate) struct MmapTables {
    pub offsets: Vec<Option<MmapOffset>>,
    pub leases: Vec<Option<Lease>>,
    pub next_page: u64,
    pub next_lease: u32,
}

impl MmapTables {
    pub(crate) const fn new() -> MmapTables {
        MmapTables { offsets: Vec::new(), leases: Vec::new(), next_page: FAKE_OFFSET_START >> 12, next_lease: 1 }
    }
}

/// What `GEM_MMAP_OFFSET` returns, plus the cache attribute the mapping must carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MmapGrant {
    pub offset: u64,
    pub kind: MmapKind,
    pub cache: CpuCache,
}

/// A granted CPU mapping: the executor maps `len` bytes of the object from `start` into the session with `cache`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MappingLease {
    pub lease: u32,
    pub obj: ObjId,
    pub start: u64,
    pub len: u64,
    pub cache: CpuCache,
}

impl Gem {
    /// `i915_gem_mmap_offset_ioctl` (gem/i915_gem_mman.c:865-915) over `__assign_mmap_offset` (gem/i915_gem_mman.c:772-805): assign (or find) the
    /// fake offset for `(object, type)` and allow the session to use it. KBL policy:
    /// GTT needs the aperture (gem/i915_gem_mman.c:889-890); WC and UC need PAT (gem/i915_gem_mman.c:895-898, gem/i915_gem_mman.c:905-906); WB is always available; FIXED belongs to
    /// objects with their own `mmap_offset` op, which shmem objects lack (gem/i915_gem_mman.c:789-790); anything else is `-EINVAL` (gem/i915_gem_mman.c:914-915).
    pub fn mmap_offset(&mut self, session: SessionId, handle: u32, flags: u64) -> Result<MmapGrant, GemError> {
        let kind = match flags {
            f if f == m::I915_MMAP_OFFSET_GTT as u64 => {
                if !self.dev.ggtt_has_aperture {
                    return Err(GemError::MmapTypeUnavailable { kind: "GTT without an aperture" });
                }
                MmapKind::Gtt
            }
            f if f == m::I915_MMAP_OFFSET_WC as u64 => {
                if !self.cfg.pat_enabled {
                    return Err(GemError::MmapTypeUnavailable { kind: "WC without PAT" });
                }
                MmapKind::Wc
            }
            f if f == m::I915_MMAP_OFFSET_WB as u64 => MmapKind::Wb,
            f if f == m::I915_MMAP_OFFSET_UC as u64 => {
                if !self.cfg.pat_enabled {
                    return Err(GemError::MmapTypeUnavailable { kind: "UC without PAT" });
                }
                MmapKind::Uc
            }
            f if f == m::I915_MMAP_OFFSET_FIXED as u64 => return Err(GemError::MmapTypeUnavailable { kind: "FIXED on a shmem object" }),
            _ => return Err(GemError::MmapFlags { flags }),
        };
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?; // __assign_mmap_offset_handle, gem/i915_gem_mman.c:806-826
        // mmap_offset_attach (gem/i915_gem_mman.c:719-770): reuse the existing node for (object, type), else add one (drm_vma_offset_add).
        if let Some(o) = self.mmap.offsets.iter().flatten().find(|o| o.obj == id && o.kind == kind && o.session as usize == slot) {
            return Ok(MmapGrant { offset: o.offset, kind, cache: kind.cpu_cache() }); // lookup_mmo + drm_vma_node_allow_once, gem/i915_gem_mman.c:727-729, gem/i915_gem_mman.c:762-763; the node is per (object, type, SESSION): a PRIME importer is allowed on its own
        }
        let size = self.obj(id).map_or(0, |o| o.size);
        let at = match self.mmap.offsets.iter().position(|o| o.is_none()) {
            Some(i) => i,
            None => table::push(&mut self.mmap.offsets, None, &mut self.quota, "mmap-offsets")?,
        };
        let offset = self.mmap.next_page << 12;
        self.mmap.next_page += size >> 12;
        self.mmap.offsets[at] = Some(MmapOffset { obj: id, kind, offset, session: slot as u32 });
        if let Some(o) = self.obj_mut(id) {
            o.mmap_kinds |= 1 << kind as u8;
        }
        Ok(MmapGrant { offset, kind, cache: kind.cpu_cache() })
    }

    fn open_lease<P: PageSource>(&mut self, id: ObjId, handle: u32, kind: MmapKind, start: u64, len: u64, src: &mut P) -> Result<MappingLease, GemError> {
        let at = match self.mmap.leases.iter().position(|l| l.is_none()) {
            Some(i) => i,
            None => table::push(&mut self.mmap.leases, None, &mut self.quota, "mmap-leases")?,
        };
        // The fault path pins the pages first (vm_fault_cpu -> i915_gem_object_pin_pages, gem/i915_gem_mman.c:269): a purged object cannot be mapped.
        self.ensure_pages(id, handle, src)?;
        let lease = self.mmap.next_lease;
        self.mmap.next_lease += 1;
        self.mmap.leases[at] = Some(Lease { id: lease, obj: id });
        if let Some(o) = self.obj_mut(id) {
            o.leases += 1; // vm_open: i915_gem_object_get (gem/i915_gem_mman.c:921-928)
        }
        Ok(MappingLease { lease, obj: id, start, len, cache: kind.cpu_cache() })
    }

    /// The `mmap(2)` of a fake offset on the device fd: `drm_vma_node_is_allowed` (`-EACCES`) then `vm_open`. `len` must lie within
    /// the object's node.
    pub fn mmap_open<P: PageSource>(&mut self, session: SessionId, offset: u64, len: u64, src: &mut P) -> Result<MappingLease, GemError> {
        let slot = self.session_slot(session)?;
        let o = self
            .mmap
            .offsets
            .iter()
            .flatten()
            .copied()
            .find(|o| o.session as usize == slot && o.offset == offset)
            .ok_or(GemError::MmapOffsetDenied { offset })?;
        let size = self.obj(o.obj).map_or(0, |x| x.size);
        if len == 0 || len > size {
            return Err(GemError::MmapRange { offset: 0, size: len, object: size });
        }
        self.open_lease(o.obj, o.obj.index(), o.kind, 0, len, src)
    }

    /// `i915_gem_mmap_ioctl` (gem/i915_gem_mman.c:60-125): the legacy CPU map. `-ENODEV` for WC without PAT (gem/i915_gem_mman.c:78-79), `-EINVAL`
    /// for unknown flags (gem/i915_gem_mman.c:75-76) and ranges outside the object (gem/i915_gem_mman.c:93-96).
    pub fn mmap_legacy<P: PageSource>(&mut self, session: SessionId, handle: u32, offset: u64, size: u64, flags: u64, src: &mut P) -> Result<MappingLease, GemError> {
        if flags & !I915_MMAP_WC != 0 {
            return Err(GemError::MmapFlags { flags }); // gem/i915_gem_mman.c:75-76
        }
        if flags & I915_MMAP_WC != 0 && !self.cfg.pat_enabled {
            return Err(GemError::MmapTypeUnavailable { kind: "WC without PAT" }); // gem/i915_gem_mman.c:78-79
        }
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let osz = self.obj(id).map_or(0, |o| o.size);
        if offset > osz || size > osz - offset {
            return Err(GemError::MmapRange { offset, size, object: osz }); // range_overflows, gem/i915_gem_mman.c:93-96
        }
        let kind = if flags & I915_MMAP_WC != 0 { MmapKind::Wc } else { MmapKind::Wb };
        self.open_lease(id, handle, kind, offset, size, src)
    }

    /// The mapping ended (`vm_close` -> `i915_gem_object_put`, gem/i915_gem_mman.c:930-937): the object may now be freed.
    pub fn mmap_close(&mut self, lease: u32, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        let at = self.mmap.leases.iter().position(|l| matches!(l, Some(l) if l.id == lease)).ok_or(GemError::LeaseUnknown { lease })?;
        let l = self.mmap.leases[at].take().expect("found");
        if let Some(o) = self.obj_mut(l.obj) {
            o.leases = o.leases.saturating_sub(1);
        }
        self.try_release_object(l.obj, sink);
        Ok(())
    }

    /// `drm_vma_node_revoke(&mmo->vma_node, file)` for every offset of a closed object (gem/i915_gem_object.c:267-270): no NEW
    /// mapping can be made; existing leases stay.
    pub(crate) fn revoke_mmap_offsets(&mut self, obj: ObjId, _sink: &mut dyn EffectSink) {
        for o in self.mmap.offsets.iter_mut() {
            if matches!(o, Some(x) if x.obj == obj) {
                *o = None;
            }
        }
        if let Some(o) = self.obj_mut(obj) {
            o.mmap_kinds = 0;
        }
    }

    /// Live leases on an object.
    pub fn lease_count(&self, obj: ObjId) -> u32 {
        self.obj(obj).map_or(0, |o| o.leases)
    }

    /// Is the object's backing store purged (a mapping can never be made again)?
    pub fn is_purged(&self, obj: ObjId) -> bool {
        self.obj(obj).is_some_and(|o| o.madv == Madv::Purged)
    }
}
