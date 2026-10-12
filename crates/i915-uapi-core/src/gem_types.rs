// SPDX-License-Identifier: GPL-2.0-only
//! The remaining i915_drm.h structs Mesa iris and anv fill on a Gen9 device (see docs/I915-UAPI-INVENTORY.md), as PLAIN DATA ONLY.
//! Their semantics belong to `i915-gem-core` (object lifetime, mmap leases, context/VM tables); this crate carries the field
//! sets so the shim, the device and that crate compile against one definition. Pointer fields are absent (flattened, see
//! `types`). `SIZEOF_*` are the C `sizeof` of the original struct, pinned by tests/types.rs from the header's field list.

/// `struct drm_i915_gem_create` (i915_drm.h:915).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemCreate {
    pub size: u64,
    pub handle: u32,
    pub pad: u32,
}
/// `struct drm_i915_gem_create_ext` (i915_drm.h:3657); the extension chain is flattened after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemCreateExt {
    pub size: u64,
    pub handle: u32,
    pub flags: u32,
}
/// `struct drm_i915_gem_mmap_offset` (i915_drm.h:1013).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemMmapOffset {
    pub handle: u32,
    pub pad: u32,
    pub offset: u64,
    pub flags: u64,
    pub extensions: u64,
}
/// `struct drm_i915_gem_mmap` (i915_drm.h:963): the legacy CPU map; `addr_ptr` is a session mapping lease here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemMmap {
    pub handle: u32,
    pub pad: u32,
    pub offset: u64,
    pub size: u64,
    pub addr_ptr: u64,
    pub flags: u64,
}
/// `struct drm_i915_gem_set_domain` (i915_drm.h:1097).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemSetDomain {
    pub handle: u32,
    pub read_domains: u32,
    pub write_domain: u32,
}
/// `struct drm_i915_gem_set_tiling` (i915_drm.h:1742).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemSetTiling {
    pub handle: u32,
    pub tiling_mode: u32,
    pub stride: u32,
    pub swizzle_mode: u32,
}
/// `struct drm_i915_gem_get_tiling` (i915_drm.h:1773).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemGetTiling {
    pub handle: u32,
    pub tiling_mode: u32,
    pub swizzle_mode: u32,
    pub phys_swizzle_mode: u32,
}
/// `struct drm_i915_gem_madvise` (i915_drm.h:1819).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemMadvise {
    pub handle: u32,
    pub madv: u32,
    pub retained: u32,
}
/// `struct drm_i915_gem_caching` (i915_drm.h:1682).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemCaching {
    pub handle: u32,
    pub caching: u32,
}
/// `struct drm_i915_gem_busy` (i915_drm.h:1591).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemBusy {
    pub handle: u32,
    pub busy: u32,
}
/// `struct drm_i915_gem_wait` (i915_drm.h:1933).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemWait {
    pub bo_handle: u32,
    pub flags: u32,
    pub timeout_ns: i64,
}
/// `struct drm_i915_gem_context_create` and `_destroy` (i915_drm.h:1941).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemContextId {
    pub ctx_id: u32,
    pub pad: u32,
}
/// `struct drm_i915_gem_context_create_ext` (i915_drm.h:1949); the extension chain is flattened after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemContextCreateExt {
    pub ctx_id: u32,
    pub flags: u32,
}
/// `struct drm_i915_gem_context_param` (i915_drm.h:1990); `value` of `ENGINES`/`SSEU` is a flattened table on the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemContextParam {
    pub ctx_id: u32,
    pub size: u32,
    pub param: u64,
    pub value: u64,
}
/// `struct drm_i915_gem_vm_control` (i915_drm.h:2635; GEM_VM_CREATE / GEM_VM_DESTROY).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemVmControl {
    pub extensions: u64,
    pub flags: u32,
    pub vm_id: u32,
}
/// `struct drm_i915_reset_stats` (i915_drm.h:2674).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResetStats {
    pub ctx_id: u32,
    pub flags: u32,
    pub reset_count: u32,
    pub batch_active: u32,
    pub batch_pending: u32,
    pub pad: u32,
}
/// `struct drm_i915_gem_get_aperture` (i915_drm.h:1796).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GemGetAperture {
    pub aper_size: u64,
    pub aper_available_size: u64,
}
/// `struct drm_i915_reg_read` (i915_drm.h:2646).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegRead {
    pub offset: u64,
    pub val: u64,
}

/// C `sizeof` of each struct above, from the header's field lists.
pub mod sizeof {
    pub const GEM_CREATE: usize = 16;
    pub const GEM_CREATE_EXT: usize = 24;
    pub const GEM_MMAP_OFFSET: usize = 32;
    pub const GEM_MMAP: usize = 40;
    pub const GEM_SET_DOMAIN: usize = 12;
    pub const GEM_SET_TILING: usize = 16;
    pub const GEM_GET_TILING: usize = 16;
    pub const GEM_MADVISE: usize = 12;
    pub const GEM_CACHING: usize = 8;
    pub const GEM_BUSY: usize = 8;
    pub const GEM_WAIT: usize = 16;
    pub const GEM_CONTEXT_ID: usize = 8;
    pub const GEM_CONTEXT_CREATE_EXT: usize = 16;
    pub const GEM_CONTEXT_PARAM: usize = 24;
    pub const GEM_VM_CONTROL: usize = 16;
    pub const RESET_STATS: usize = 24;
    pub const GEM_GET_APERTURE: usize = 16;
    pub const REG_READ: usize = 16;
}
