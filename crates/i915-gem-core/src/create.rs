// SPDX-License-Identifier: GPL-2.0-only
//! Object creation and the per-object ioctls that need no GPU activity: GEM_CREATE(_EXT), GEM_CLOSE, GEM_MADVISE, caching,
//! set/get tiling, set_domain, and the refusal of userptr. Ported from gem/i915_gem_create.c, gem/i915_gem_object.c,
//! i915_gem.c (madvise), gem/i915_gem_domain.c, gem/i915_gem_tiling.c and gem/i915_gem_userptr.c (v7.0-rc6).
use crate::error::GemError;
use crate::gem::Gem;
use crate::ids::{ObjId, SessionId};
use crate::object::{Madv, MmapKind, Object, ObjectState, DOMAIN_GTT, DOMAIN_WC};
use crate::seam::{Effect, EffectSink, PageRequest, PageSource};
use i915_memory_core::{CacheLevel, GTT_PAGE};
use i915_uapi_core::consts::{domain as d, misc as m};

/// `GEN7_FENCE_MAX_PITCH_VAL` (i915_reg.h:219).
pub const GEN7_FENCE_MAX_PITCH_VAL: u32 = 0x0800; // i915_reg.h:219

/// The extensions of `DRM_IOCTL_I915_GEM_CREATE_EXT`, decoded (gem/i915_gem_create.c:272-441).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CreateExt<'a> {
    /// `I915_GEM_CREATE_EXT_MEMORY_REGIONS`: `(memory_class, memory_instance)` per placement; empty means SYSTEM.
    pub regions: &'a [(u16, u16)],
    /// `I915_GEM_CREATE_EXT_PROTECTED_CONTENT` present.
    pub protected: bool,
    /// `I915_GEM_CREATE_EXT_SET_PAT` present with this index.
    pub set_pat: Option<u32>,
}

/// What `GEM_CREATE` returns: the handle and the size actually allocated (rounded up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Created {
    pub handle: u32,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TilingReply {
    pub tiling_mode: u32,
    pub stride: u32,
    pub swizzle_mode: u32,
}

impl<const V: usize, const T: usize> Gem<V, T> {
    /// `i915_gem_create_ioctl` (gem/i915_gem_create.c:224-244): a SYSTEM object.
    pub fn create(&mut self, session: SessionId, size: u64) -> Result<Created, GemError> {
        self.create_object(session, size)
    }

    /// `i915_gem_create_ext_ioctl` (gem/i915_gem_create.c:441-503).
    pub fn create_ext(&mut self, session: SessionId, size: u64, flags: u32, ext: &CreateExt<'_>) -> Result<Created, GemError> {
        if flags & !m::I915_GEM_CREATE_EXT_FLAG_NEEDS_CPU_ACCESS != 0 {
            return Err(GemError::CreateFlags { flags }); // gem/i915_gem_create.c:450-451
        }
        if ext.protected {
            return Err(GemError::CreateProtectedUnsupported); // ext_set_protected: no PXP, gem/i915_gem_create.c:389-390
        }
        if ext.set_pat.is_some() {
            return Err(GemError::CreateSetPatUnsupported); // ext_set_pat: Xe_LPG and later only, gem/i915_gem_create.c:409-410
        }
        for &(class, instance) in ext.regions {
            // set_placements: only the SYSTEM region (class 0, instance 0) exists on a Gen9 iGPU.
            if class != 0 || instance != 0 {
                return Err(GemError::CreateRegionUnsupported { class, instance }); // gem/i915_gem_create.c:272-365
            }
        }
        if flags & m::I915_GEM_CREATE_EXT_FLAG_NEEDS_CPU_ACCESS != 0 {
            // A single placement cannot satisfy NEEDS_CPU_ACCESS (gem/i915_gem_create.c:467-470): there is no other region to spill to.
            return Err(GemError::CreateNeedsCpuAccess);
        }
        self.create_object(session, size)
    }

    /// `__i915_gem_object_create_user_ext` + `i915_gem_publish` (gem/i915_gem_create.c:90-155, gem/i915_gem_create.c:71-88).
    fn create_object(&mut self, session: SessionId, size: u64) -> Result<Created, GemError> {
        let slot = self.session_slot(session)?;
        let size = size.checked_add(GTT_PAGE - 1).ok_or(GemError::CreateTooBig { size, max: self.max_object_bytes() })? & !(GTT_PAGE - 1); // gem/i915_gem_create.c:102
        if size == 0 {
            return Err(GemError::CreateZeroSize); // gem/i915_gem_create.c:103-104
        }
        let max = self.max_object_bytes();
        if size > max {
            return Err(GemError::CreateTooBig { size, max }); // gem/i915_gem_create.c:109-110, bounded by system RAM (port bound, see max_object_bytes)
        }
        let obj = Object::new(slot as u32, size, self.dev.desc.info.has_llc);
        let (idx, gen) = self.objects.insert(obj, &mut self.quota, "objects", u32::MAX as usize)?;
        let id = ObjId::new(gen, idx);
        match self.install_handle(slot, id) {
            Ok(handle) => Ok(Created { handle, size }),
            Err(e) => {
                self.objects.remove(idx); // nothing half-created: the object goes again
                Err(e)
            }
        }
    }

    /// PORT BOUND: Linux only fails `i915_gem_object_size_2big` on `size_t` overflow and lets shmem fail later; a planner that is
    /// handed a request for more than the machine's RAM refuses it up front, by name.
    pub fn max_object_bytes(&self) -> u64 {
        self.dev.system_memory_bytes
    }

    /// `drm_gem_close_ioctl` -> `drm_gem_handle_delete` -> `i915_gem_close_object` (drm_gem.c:400-430, gem/i915_gem_object.c:239-293).
    pub fn close(&mut self, session: SessionId, handle: u32, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        let slot = self.session_slot(session)?;
        let obj = self.lookup(slot, handle).map_err(|e| GemError::CloseHandle { handle, cross: matches!(e, GemError::CrossSessionHandle { .. }) })?;
        let i = (handle & crate::ids::HANDLE_IDX_MASK) - 1;
        self.release_handle(slot, i, obj, sink);
        Ok(())
    }

    /// The size and bound state of an object by handle (`drm_gem_object_lookup` + `obj->base.size`).
    pub fn object_size(&self, session: SessionId, handle: u32) -> Result<u64, GemError> {
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        Ok(self.obj(id).map_or(0, |o| o.size))
    }

    /// A read-only snapshot of an object's state (size, madvise, cache, tiling, CPU/GPU domains).
    pub fn object_state(&self, session: SessionId, handle: u32) -> Result<ObjectState, GemError> {
        let slot = self.session_slot(session)?;
        let o = *self.obj(self.lookup(slot, handle)?).ok_or(GemError::HandleNotInSession { handle })?;
        Ok(ObjectState {
            size: o.size,
            madv: o.madv,
            pages_resident: o.pages,
            cache: o.cache,
            cache_dirty: o.cache_dirty,
            coherent_for_read: o.coherent & crate::object::COHERENT_FOR_READ != 0,
            coherent_for_write: o.coherent & crate::object::COHERENT_FOR_WRITE != 0,
            tiling: o.tiling,
            stride: o.stride,
            read_domains: o.read_domains,
            write_domain: o.write_domain,
            leases: o.leases,
        })
    }

    /// Resolve a handle to its object id (the session-scoped lookup every ioctl starts with).
    pub fn lookup_object(&self, session: SessionId, handle: u32) -> Result<ObjId, GemError> {
        let slot = self.session_slot(session)?;
        self.lookup(slot, handle)
    }

    /// `i915_gem_object_pin_pages` for a CPU or GPU user: make the pages resident, refusing a purged object
    /// (gem/i915_gem_pages.c:109-112). The pages are requested from the executor with a typed [`PageRequest`].
    pub(crate) fn ensure_pages<P: PageSource>(&mut self, id: ObjId, handle: u32, src: &mut P) -> Result<(), GemError> {
        let o = self.obj(id).copied().ok_or(GemError::HandleNotInSession { handle })?;
        if o.pages {
            return Ok(()); // already resident: pinning just takes a count, get_pages is not called again
        }
        if o.madv != Madv::WillNeed {
            return Err(GemError::ObjectPurged { handle }); // ____i915_gem_object_get_pages, gem/i915_gem_pages.c:109-112
        }
        src.alloc_pages(PageRequest { obj: id, bytes: o.size, cache: o.cache }).map_err(|_| GemError::PagesRefused { bytes: o.size })?;
        let have = src.pages(id).ok_or(GemError::PagesMalformed { have: 0, want: o.size / GTT_PAGE })?;
        let want = o.size / GTT_PAGE;
        if have.len() as u64 != want || have.iter().any(|p| p % GTT_PAGE != 0) {
            return Err(GemError::PagesMalformed { have: have.len() as u64, want });
        }
        if let Some(o) = self.obj_mut(id) {
            o.pages = true;
        }
        Ok(())
    }

    // ---- madvise ------------------------------------------------------------------------------------------------------------

    /// `i915_gem_madvise_ioctl` (i915_gem.c:1029-1100). Returns `retained`: false once the backing store is gone.
    pub fn madvise(&mut self, session: SessionId, handle: u32, madv: u32) -> Result<bool, GemError> {
        let want = match madv {
            m::I915_MADV_WILLNEED => Madv::WillNeed,
            m::I915_MADV_DONTNEED => Madv::DontNeed,
            _ => return Err(GemError::MadviseInvalid { madv }), // i915_gem.c:1037-1043
        };
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let o = self.obj_mut(id).ok_or(GemError::HandleNotInSession { handle })?;
        if o.madv != Madv::Purged {
            o.madv = want; // i915_gem.c:1068-1072
        }
        if o.madv == Madv::DontNeed && !o.pages {
            // "if the object is no longer attached, discard its backing storage" (i915_gem.c:1092-1095): shmem_truncate, gem/i915_gem_shmem.c:290-302.
            o.madv = Madv::Purged;
        }
        Ok(o.madv != Madv::Purged) // i915_gem.c:1097
    }

    // ---- caching ------------------------------------------------------------------------------------------------------------

    /// `i915_gem_get_caching_ioctl` (gem/i915_gem_domain.c:297-333).
    pub fn get_caching(&self, session: SessionId, handle: u32) -> Result<u32, GemError> {
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let o = self.obj(id).ok_or(GemError::HandleNotInSession { handle })?;
        Ok(match o.cache {
            CacheLevel::Llc | CacheLevel::L3Llc => m::I915_CACHING_CACHED, // gem/i915_gem_domain.c:323-325
            CacheLevel::Wt => m::I915_CACHING_DISPLAY,                       // gem/i915_gem_domain.c:326-327
            CacheLevel::None => m::I915_CACHING_NONE,
        })
    }

    /// `i915_gem_set_caching_ioctl` (gem/i915_gem_domain.c:335-413) + `i915_gem_object_set_cache_level` (gem/i915_gem_domain.c:266-295). On an LLC part
    /// every level is allowed; the mapping is rebound with the new PTE cache bits ("applied when each vma is rebound", gem/i915_gem_domain.c:293).
    pub fn set_caching(&mut self, session: SessionId, handle: u32, caching: u32, sink: &mut dyn EffectSink) -> Result<(), GemError> {
        let level = match caching {
            m::I915_CACHING_NONE => CacheLevel::None,
            m::I915_CACHING_CACHED => {
                if !self.dev.desc.info.has_llc {
                    return Err(GemError::CachingUnsupported); // gem/i915_gem_domain.c:361-362 (no HAS_SNOOP row is carried)
                }
                CacheLevel::Llc
            }
            m::I915_CACHING_DISPLAY => CacheLevel::None, // HAS_WT: no eDRAM row carried, gem/i915_gem_domain.c:367
            _ => return Err(GemError::CachingInvalid { caching }), // gem/i915_gem_domain.c:369-370
        };
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let has_llc = self.dev.desc.info.has_llc;
        let o = self.obj_mut(id).ok_or(GemError::HandleNotInSession { handle })?;
        if o.cache == level {
            return Ok(()); // i915_gem_object_has_cache_level, gem/i915_gem_domain.c:277-278
        }
        o.set_cache_coherency(level, has_llc); // gem/i915_gem_domain.c:288; also sets cache_dirty, gem/i915_gem_domain.c:289
        o.cache_dirty = true;
        self.unbind_everywhere(id, sink); // i915_gem_object_unbind(ACTIVE|BARRIER), gem/i915_gem_domain.c:292
        Ok(())
    }

    // ---- set_domain ---------------------------------------------------------------------------------------------------------

    /// `i915_gem_set_domain_ioctl` (gem/i915_gem_domain.c:537-657) on an LLC part: the object is coherent, so moving it to a CPU
    /// domain needs no flush; the checks, the page pin and the domain bookkeeping are Linux's. The wait on the GPU is the caller's
    /// ([`Gem::wait_plan`]) - this returns whether one is needed (`i915_gem_object_wait`, gem/i915_gem_domain.c:572-576).
    pub fn set_domain<P: PageSource>(&mut self, session: SessionId, handle: u32, read: u32, write: u32, src: &mut P) -> Result<bool, GemError> {
        let gpu = d::I915_GEM_DOMAIN_RENDER | d::I915_GEM_DOMAIN_SAMPLER | d::I915_GEM_DOMAIN_COMMAND | d::I915_GEM_DOMAIN_INSTRUCTION | d::I915_GEM_DOMAIN_VERTEX;
        if (write | read) & gpu != 0 {
            return Err(GemError::SetDomainGpuDomain { read, write }); // gem/i915_gem_domain.c:550-551
        }
        if write != 0 && read != write {
            return Err(GemError::SetDomainWriteNotRead { read, write }); // gem/i915_gem_domain.c:557-558
        }
        if read == 0 {
            return Ok(false); // gem/i915_gem_domain.c:560-561
        }
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let busy = self.object_active(id);
        self.ensure_pages(id, handle, src)?; // i915_gem_object_pin_pages, gem/i915_gem_domain.c:619
        let o = self.obj_mut(id).ok_or(GemError::HandleNotInSession { handle })?;
        if o.write_domain == read {
            return Ok(busy); // already in that write domain, gem/i915_gem_domain.c:633-634
        }
        // The move itself: WC first, then GTT, else CPU (gem/i915_gem_domain.c:636-641).
        if read & d::I915_GEM_DOMAIN_WC != 0 {
            o.set_to_uncached_cpu_domain(DOMAIN_WC, write != 0);
        } else if read & d::I915_GEM_DOMAIN_GTT != 0 {
            o.set_to_uncached_cpu_domain(DOMAIN_GTT, write != 0);
        } else {
            o.set_to_cpu_domain(write != 0);
        }
        Ok(busy)
    }

    // ---- tiling -------------------------------------------------------------------------------------------------------------

    /// `i915_tiling_ok` (gem/i915_gem_tiling.c:118-159) on Gen9.
    fn tiling_ok(&self, tiling: u32, stride: u32) -> bool {
        if tiling == m::I915_TILING_NONE {
            return true; // gem/i915_gem_tiling.c:124-125
        }
        if tiling > m::I915_TILING_LAST {
            return false; // gem/i915_gem_tiling.c:127-128
        }
        // GRAPHICS_VER >= 7: the pitch must fit the fence register (gem/i915_gem_tiling.c:134-136).
        if stride / 128 > GEN7_FENCE_MAX_PITCH_VAL {
            return false;
        }
        // HAS_128_BYTE_Y_TILING (i915_drv.h:589) is true on Gen9, so Y tiles are 128 bytes wide; X tiles are 512 (gem/i915_gem_tiling.c:147-153).
        let tile_width = if tiling == m::I915_TILING_Y { 128 } else { 512 };
        !(stride == 0 || !stride.is_multiple_of(tile_width)) // gem/i915_gem_tiling.c:155-156
    }

    fn num_fences(&self) -> u32 {
        // intel_ggtt_init_fences (gt/intel_ggtt_fencing.c:853-857): no aperture -> 0; GRAPHICS_VER >= 7 -> 32.
        if !self.dev.ggtt_has_aperture {
            0
        } else {
            32
        }
    }

    /// `i915_gem_set_tiling_ioctl` (gem/i915_gem_tiling.c:343-421) + `i915_gem_object_set_tiling` (gem/i915_gem_tiling.c:231-324). Swizzling is NONE
    /// on Gen8+ (`detect_bit_6_swizzle`, gt/intel_ggtt_fencing.c:574-590).
    pub fn set_tiling(&mut self, session: SessionId, handle: u32, tiling: u32, stride: u32, sink: &mut dyn EffectSink) -> Result<TilingReply, GemError> {
        if self.num_fences() == 0 {
            return Err(GemError::TilingNoFences); // gem/i915_gem_tiling.c:351-352
        }
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        if !self.tiling_ok(tiling, stride) {
            return Err(GemError::TilingNotOk { tiling, stride }); // gem/i915_gem_tiling.c:367-370
        }
        let (tiling, stride) = if tiling == m::I915_TILING_NONE { (m::I915_TILING_NONE, 0) } else { (tiling, stride) }; // gem/i915_gem_tiling.c:373-376
        let o = self.obj_mut(id).ok_or(GemError::HandleNotInSession { handle })?;
        if (o.tiling, o.stride) != (tiling, stride) {
            if o.framebuffer {
                return Err(GemError::TilingFramebuffer { handle }); // gem/i915_gem_tiling.c:247-248
            }
            o.tiling = tiling;
            o.stride = stride;
            let mapped = o.mmap_kinds & (1 << MmapKind::Gtt as u8) != 0;
            if mapped {
                sink.effect(Effect::RevokeMmap { obj: id }); // i915_gem_object_release_mmap_gtt, gem/i915_gem_tiling.c:323
            }
        }
        let o = self.obj(id).ok_or(GemError::HandleNotInSession { handle })?;
        Ok(TilingReply { tiling_mode: o.tiling, stride: o.stride, swizzle_mode: m::I915_BIT_6_SWIZZLE_NONE })
    }

    /// `i915_gem_get_tiling_ioctl` (gem/i915_gem_tiling.c:426-470).
    pub fn get_tiling(&self, session: SessionId, handle: u32) -> Result<TilingReply, GemError> {
        if self.num_fences() == 0 {
            return Err(GemError::TilingNoFences); // gem/i915_gem_tiling.c:434-435
        }
        let slot = self.session_slot(session)?;
        let id = self.lookup(slot, handle)?;
        let o = self.obj(id).ok_or(GemError::HandleNotInSession { handle })?;
        Ok(TilingReply { tiling_mode: o.tiling, stride: o.stride, swizzle_mode: m::I915_BIT_6_SWIZZLE_NONE })
    }

    /// The display owner marks a scanout object (`i915_gem_object_is_framebuffer`), after which its tiling is immutable.
    pub fn set_framebuffer(&mut self, id: ObjId, on: bool) -> Result<(), GemError> {
        let o = self.obj_mut(id).ok_or(GemError::HandleNotInSession { handle: id.index() })?;
        o.framebuffer = on;
        Ok(())
    }

    // ---- userptr ------------------------------------------------------------------------------------------------------------

    /// `i915_gem_userptr_ioctl` (gem/i915_gem_userptr.c:462-): REFUSED by name until revocable pins exist (KBL-I915-PORT-PLAN
    /// section 2). A user pointer is process memory the device cannot revoke; a pin that cannot be taken back would let a client
    /// keep device-visible memory after it dies.
    pub fn userptr(&self, _session: SessionId, size: u64) -> Result<(), GemError> {
        Err(GemError::UserptrRefused { size })
    }
}
