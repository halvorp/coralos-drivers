// SPDX-License-Identifier: GPL-2.0-only
//! `struct drm_i915_gem_object`, reduced to what a system-memory object on a Gen9 LLC part has: size, backing-page state,
//! madvise state, cache level and coherency, tiling, CPU domains, and the counts that decide when it may be freed.
//! Ported from gem/i915_gem_object_types.h and gem/i915_gem_object.c (v7.0-rc6).
use i915_memory_core::CacheLevel;

/// `obj->mm.madv` (i915_drm.h:1815-1816, plus `__I915_MADV_PURGED`, gem/i915_gem_object_types.h).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Madv {
    WillNeed,
    DontNeed,
    /// `__I915_MADV_PURGED`: the backing store was discarded (gem/i915_gem_shmem.c:301).
    Purged,
}

/// `I915_BO_CACHE_COHERENT_FOR_READ` (gem/i915_gem_object_types.h:485).
pub const COHERENT_FOR_READ: u8 = 1 << 0; // i915_gem_object_types.h:485
/// `I915_BO_CACHE_COHERENT_FOR_WRITE` (gem/i915_gem_object_types.h:486).
pub const COHERENT_FOR_WRITE: u8 = 1 << 1; // i915_gem_object_types.h:486

/// The mmap types of `enum i915_mmap_type` (gem/i915_gem_mman.c:772-800), in `I915_MMAP_OFFSET_*` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MmapKind {
    Gtt = 0,
    Wc = 1,
    Wb = 2,
    Uc = 3,
    Fixed = 4,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Object {
    /// The owning session's slot. Objects are not shared between sessions in this crate (PRIME/flink are `devices/i915`'s).
    pub owner: u8,
    pub size: u64,
    /// Handles naming this object (`kref` held by the handle idr). Zero means closed.
    pub handles: u16,
    pub madv: Madv,
    /// Backing pages resident (`i915_gem_object_has_pages`).
    pub pages: bool,
    /// `obj->pat_index` on pre-Gen12: the `i915_cache_level` (memory-core `CacheLevel` values ARE the pat index, pte.rs).
    pub cache: CacheLevel,
    pub coherent: u8,
    pub cache_dirty: bool,
    /// `obj->tiling_and_stride` split: `I915_TILING_*` and the stride in bytes.
    pub tiling: u32,
    pub stride: u32,
    pub read_domains: u32,
    pub write_domain: u32,
    /// Scanout: `i915_gem_object_is_framebuffer`. Set by the display owner through [`crate::Gem::set_framebuffer`].
    pub framebuffer: bool,
    /// Bit `1 << MmapKind` per mmap offset assigned (`obj->mmo.offsets`).
    pub mmap_kinds: u8,
    /// Live CPU-mapping leases (each holds the object, `vm_open`, gem/i915_gem_mman.c:921-926).
    pub leases: u16,
}

impl Object {
    pub(crate) fn new(owner: u8, size: u64, has_llc: bool) -> Object {
        // shmem_object_init (gem/i915_gem_shmem.c:585-606): CPU domain, LLC where the part has one.
        let mut o = Object {
            owner,
            size,
            handles: 1,
            madv: Madv::WillNeed,
            pages: false,
            cache: CacheLevel::None,
            coherent: 0,
            cache_dirty: false,
            tiling: 0,
            stride: 0,
            read_domains: 1, // I915_GEM_DOMAIN_CPU, i915_drm.h:1171
            write_domain: 1,
            framebuffer: false,
            mmap_kinds: 0,
            leases: 0,
        };
        o.set_cache_coherency(if has_llc { CacheLevel::Llc } else { CacheLevel::None }, has_llc);
        o
    }

    /// `i915_gem_object_set_cache_coherency` (gem/i915_gem_object.c:156-176).
    pub(crate) fn set_cache_coherency(&mut self, level: CacheLevel, has_llc: bool) {
        self.cache = level;
        self.coherent = if level != CacheLevel::None {
            COHERENT_FOR_READ | COHERENT_FOR_WRITE // gem/i915_gem_object.c:163-165
        } else if has_llc {
            COHERENT_FOR_READ // gem/i915_gem_object.c:166-167
        } else {
            0
        };
        self.cache_dirty = self.coherent & COHERENT_FOR_WRITE == 0; // gem/i915_gem_object.c:171-173, !IS_DGFX
    }

    /// `gpu_write_needs_clflush` (gem/i915_gem_domain.c:19-35) on a non-DGFX part.
    fn gpu_write_needs_clflush(&self) -> bool {
        !(self.cache == CacheLevel::None || self.cache == CacheLevel::Wt)
    }

    /// `i915_gem_cpu_write_needs_clflush` (gem/i915_gem_domain.c:37-52).
    fn cpu_write_needs_clflush(&self) -> bool {
        if self.cache_dirty {
            return false;
        }
        if self.coherent & COHERENT_FOR_WRITE == 0 {
            return true;
        }
        self.framebuffer // "Currently in use by HW (display engine)? Keep flushed."
    }

    /// `flush_write_domain` (gem/i915_gem_domain.c:54-89): leave the write domain if it is one of `flush_domains`.
    pub(crate) fn flush_write_domain(&mut self, flush_domains: u32) {
        if self.write_domain & flush_domains == 0 {
            return; // gem/i915_gem_domain.c:61-62
        }
        if self.write_domain == GPU_RENDER && self.gpu_write_needs_clflush() {
            self.cache_dirty = true; // gem/i915_gem_domain.c:82-84 (the GTT, WC and CPU cases flush hardware this model does not carry)
        }
        self.write_domain = 0; // gem/i915_gem_domain.c:88
    }

    /// `__start_cpu_write` (gem/i915_gem_object.h:795-801).
    fn start_cpu_write(&mut self) {
        self.read_domains = DOMAIN_CPU;
        self.write_domain = DOMAIN_CPU;
        if self.cpu_write_needs_clflush() {
            self.cache_dirty = true;
        }
    }

    /// `i915_gem_object_set_to_cpu_domain` (gem/i915_gem_domain.c:493-526).
    pub(crate) fn set_to_cpu_domain(&mut self, write: bool) {
        self.flush_write_domain(!DOMAIN_CPU); // gem/i915_gem_domain.c:506
        if self.read_domains & DOMAIN_CPU == 0 {
            self.read_domains |= DOMAIN_CPU; // after the (synchronous) clflush, gem/i915_gem_domain.c:509-512
        }
        if write {
            self.start_cpu_write(); // gem/i915_gem_domain.c:522-523
        }
    }

    /// `i915_gem_object_set_to_wc_domain` / `..._to_gtt_domain` (gem/i915_gem_domain.c:129-179, gem/i915_gem_domain.c:191-249), for `domain` WC or GTT.
    pub(crate) fn set_to_uncached_cpu_domain(&mut self, domain: u32, write: bool) {
        if self.write_domain == domain {
            return; // gem/i915_gem_domain.c:142-143, gem/i915_gem_domain.c:204-205
        }
        self.flush_write_domain(!domain); // gem/i915_gem_domain.c:157, gem/i915_gem_domain.c:219
        self.read_domains |= domain; // gem/i915_gem_domain.c:170, gem/i915_gem_domain.c:232
        if write {
            self.read_domains = domain; // gem/i915_gem_domain.c:172-173, gem/i915_gem_domain.c:236-237
            self.write_domain = domain;
        }
    }

    /// The domain bookkeeping of `_i915_vma_move_to_active` (i915_vma.c:2022-2039) for a GPU access.
    pub(crate) fn move_to_active(&mut self, write: bool) {
        if write {
            self.write_domain = GPU_RENDER; // i915_vma.c:2024
            self.read_domains = 0; // i915_vma.c:2025
        } else {
            self.write_domain = 0; // i915_vma.c:2028
        }
        self.read_domains |= GPU_DOMAINS_MASK; // i915_vma.c:2038
    }
}

/// `I915_GEM_DOMAIN_CPU` (i915_drm.h:1171) and the other domain bits this file names.
pub const DOMAIN_CPU: u32 = 0x1; // i915_drm.h:1171
pub const DOMAIN_GTT: u32 = 0x40; // i915_drm.h:1183
pub const DOMAIN_WC: u32 = 0x80; // i915_drm.h:1185
const GPU_RENDER: u32 = 0x2; // i915_drm.h:1173
/// `I915_GEM_GPU_DOMAINS` (i915_gem.h:42-47): RENDER | SAMPLER | COMMAND | INSTRUCTION | VERTEX.
const GPU_DOMAINS_MASK: u32 = 0x2 | 0x4 | 0x8 | 0x10 | 0x20; // i915_drm.h:1173-1181

/// A read-only snapshot of an object, for the device and for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectState {
    pub size: u64,
    pub madv: Madv,
    pub pages_resident: bool,
    pub cache: CacheLevel,
    pub cache_dirty: bool,
    pub coherent_for_read: bool,
    pub coherent_for_write: bool,
    pub tiling: u32,
    pub stride: u32,
    pub read_domains: u32,
    pub write_domain: u32,
    pub leases: u16,
}
