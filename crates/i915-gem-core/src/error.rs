// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal of the GEM core: a NAMED error carrying the value and the bound, the Linux errno it maps to, and a stable token
//! for markers (`[I915-GEM REFUSED why=CROSS-SESSION-HANDLE ...]`). No bare `false`, no silent clamp (PORT_CONVENTIONS.md section 5).
use i915_memory_core::MemError;
use i915_submit_core::SubmitError;
use i915_uapi_core::UapiError;

/// Linux errno values (positive), from include/uapi/asm-generic/errno-base.h and errno.h. `UapiError::errno()` already covers the
/// subset the uAPI crate uses; these are the few more that the GEM semantics return.
pub mod errno {
    pub const EPERM: i32 = 1; // errno-base.h:5
    pub const ENOENT: i32 = 2; // errno-base.h:6
    pub const EIO: i32 = 5; // errno-base.h:9
    pub const ENXIO: i32 = 6; // errno-base.h:10
    pub const EAGAIN: i32 = 11; // errno-base.h:15
    pub const ENOMEM: i32 = 12; // errno-base.h:16
    pub const EACCES: i32 = 13; // errno-base.h:17
    pub const EFAULT: i32 = 14; // errno-base.h:18
    pub const EBUSY: i32 = 16; // errno-base.h:20
    pub const ENODEV: i32 = 19; // errno-base.h:23
    pub const EINVAL: i32 = 22; // errno-base.h:26
    pub const EFBIG: i32 = 27; // errno-base.h:31
    pub const ENOSPC: i32 = 28; // errno-base.h:32
    pub const ETIME: i32 = 62; // errno.h:45
    pub const EOPNOTSUPP: i32 = 95; // errno.h:79
}
use errno::*;

/// Why a GEM operation refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GemError {
    /// A table that tracks a runtime population could not grow (allocation failure or the device quota): the thing was NOT created
    /// and nothing was half-made. Doctrine RUNTIME-TABLES-HAVE-NO-CAP; the table is named. `-ENOMEM`.
    NoMem { table: &'static str },
    /// A user handle's index space is used up (`2^20 - 1` live handles in one session: the encoding of a `u32` handle, `ids.rs`).
    HandleSpaceExhausted { table: &'static str },

    // ---- sessions and handles ----------------------------------------------------------------------------------------------
    /// A `SessionId` that names no live session: closed, or never opened. Incarnations are never reused.
    SessionGone { session: u64 },
    /// The handle names nothing in the calling session's table (`drm_gem_object_lookup` found no object: `-ENOENT`).
    HandleNotInSession { handle: u32 },
    /// `GEM_CLOSE` of a handle that is not open in this session: `-EINVAL` (drm_gem.c:407-410), not `-ENOENT`. `cross` marks a
    /// live handle of ANOTHER session, which keeps its `CROSS-SESSION-HANDLE` name.
    CloseHandle { handle: u32, cross: bool },
    /// The handle is a LIVE handle of ANOTHER session. Refused exactly like an unknown handle (`-ENOENT`: a session's namespace
    /// is its own, `drm_gem_object_lookup` searches only `filp->object_idr`) but named, so theft attempts show in the log.
    CrossSessionHandle { handle: u32 },
    /// The handle's slot exists but the handle is from an earlier occupant of it (the generation differs): a stale handle.
    StaleHandle { handle: u32 },

    // ---- create / lifetime / madvise ----------------------------------------------------------------------------------------
    /// `round_up(size, ...) == 0` (gem/i915_gem_create.c:103-104).
    CreateZeroSize,
    /// The size does not fit the object (`i915_gem_object_size_2big`, gem/i915_gem_create.c:109-110) or rounding overflowed.
    CreateTooBig { size: u64, max: u64 },
    /// `args->flags` has bits besides `NEEDS_CPU_ACCESS` (gem/i915_gem_create.c:450-451).
    CreateFlags { flags: u32 },
    /// A placement other than SYSTEM: Kaby Lake has no device memory region (`set_placements`, gem/i915_gem_create.c:272).
    CreateRegionUnsupported { class: u16, instance: u16 },
    /// `NEEDS_CPU_ACCESS` with a single placement (gem/i915_gem_create.c:467-470).
    CreateNeedsCpuAccess,
    /// `I915_GEM_CREATE_EXT_PROTECTED_CONTENT`: no PXP on Gen9 (`ext_set_protected`, gem/i915_gem_create.c:389-390).
    CreateProtectedUnsupported,
    /// `I915_GEM_CREATE_EXT_SET_PAT`: only Xe_LPG and later (`ext_set_pat`, gem/i915_gem_create.c:409).
    CreateSetPatUnsupported,
    /// `madv` is neither WILLNEED nor DONTNEED (i915_gem.c:1037-1043).
    MadviseInvalid { madv: u32 },
    /// The object's backing store was purged; it can never be used again (gem/i915_gem_pages.c:109-112: `-EFAULT`).
    ObjectPurged { handle: u32 },
    /// The caller's `PageSource` refused to supply backing pages.
    PagesRefused { bytes: u64 },
    /// The `PageSource` returned a page list that is not `size / 4096` entries or has an unaligned entry.
    PagesMalformed { have: u64, want: u64 },

    // ---- caching / domain / tiling ------------------------------------------------------------------------------------------
    /// `I915_CACHING_*` selector out of range (gem/i915_gem_domain.c:369-370).
    CachingInvalid { caching: u32 },
    /// `I915_CACHING_CACHED` on a part with neither LLC nor snoop (gem/i915_gem_domain.c:361-362).
    CachingUnsupported,
    /// `set_domain` with a GPU domain bit (gem/i915_gem_domain.c:550-551).
    SetDomainGpuDomain { read: u32, write: u32 },
    /// `set_domain` with a write domain that is not the whole read set (gem/i915_gem_domain.c:557-558).
    SetDomainWriteNotRead { read: u32, write: u32 },
    /// `i915_tiling_ok` failed (gem/i915_gem_tiling.c:118-159).
    TilingNotOk { tiling: u32, stride: u32 },
    /// The device has no fence registers (`ggtt->num_fences == 0`, gem/i915_gem_tiling.c:351-352).
    TilingNoFences,
    /// The object is a scanout framebuffer; its tiling cannot change (`-EBUSY`, gem/i915_gem_tiling.c:247-248).
    TilingFramebuffer { handle: u32 },

    // ---- mmap ---------------------------------------------------------------------------------------------------------------
    /// `GEM_MMAP_OFFSET` flags is not one of the five mmap types (gem/i915_gem_mman.c:914-915).
    MmapFlags { flags: u64 },
    /// This mmap type cannot be offered here (GTT without an aperture, WC/UC without PAT, FIXED on a shmem object:
    /// gem/i915_gem_mman.c:772-800 and :886-912). `-ENODEV`.
    MmapTypeUnavailable { kind: &'static str },
    /// `GEM_MMAP` range outside the object (gem/i915_gem_mman.c:93-96).
    MmapRange { offset: u64, size: u64, object: u64 },
    /// The offset is not one this session was allowed to map (`drm_vma_node_is_allowed`, `-EACCES`).
    MmapOffsetDenied { offset: u64 },
    /// No such live lease.
    LeaseUnknown { lease: u32 },

    // ---- userptr ------------------------------------------------------------------------------------------------------------
    /// `GEM_USERPTR` is refused until revocable pins exist (KBL-I915-PORT-PLAN section 2). A zero-size probe answers `-EINVAL`
    /// like Linux (the iris probe, inventory section 7); everything else answers `-ENODEV`.
    UserptrRefused { size: u64 },

    // ---- contexts and VMs ---------------------------------------------------------------------------------------------------
    /// The context id names no live context of the session (`i915_gem_context_lookup`, gem/i915_gem_context.c:2340-2355).
    ContextMissing { id: u32 },
    /// The client's ban score reached `I915_CLIENT_SCORE_BANNED` (gem/i915_gem_context.c:2289-2292, :2384-2389: `-EIO`).
    ClientBanned { score: u32 },
    /// The context is banned (gem/i915_gem_execbuffer.c:2558-2559: `-EIO`).
    ContextBanned { id: u32 },
    /// `I915_CONTEXT_CREATE_FLAGS_*` has an unknown bit (gem/i915_gem_context.c:2376-2377).
    ContextCreateFlags { flags: u32 },
    /// The VM id names no live VM of the session (`-ENOENT`, gem/i915_gem_context.c:382-385).
    VmMissing { id: u32 },
    /// A new address space is needed (no `PARAM_VM` named one) but the executor supplied no `PpgttInit` pages.
    VmPagesMissing,
    /// Context param with a non-zero `size` where none is allowed (gem/i915_gem_context.c:2197-2198 and siblings).
    ParamSize { param: u64, size: u32 },
    /// Unknown or write-only/read-only context param (`default: ret = -EINVAL`, gem/i915_gem_context.c:2242-2255).
    ParamUnknown { param: u64 },
    /// `PARAM_PRIORITY` outside [-1023, 1023] (gem/i915_gem_context.c:184-186).
    PriorityRange { priority: i64 },
    /// Raising priority above default, or clearing BANNABLE, without the privilege (gem/i915_gem_context.c:188-190, :2209-2210).
    ParamPermission { param: u64 },
    /// `PARAM_ENGINES` set twice (gem/i915_gem_context.c:745-748).
    EnginesTwice,
    /// `PARAM_ENGINES` array longer than `I915_EXEC_RING_MASK + 1` (gem/i915_gem_context.c:758-760).
    EnginesTooMany { count: usize },
    /// An engine map entry names an engine the device does not have (gem/i915_gem_context.c:781-789: `-ENOENT`).
    EngineMissing { class: u16, instance: u16 },
    /// A feature Gen9 lacks (SSEU reconfiguration is Gen11-only, protected content needs PXP, non-persistence needs preemption): `-ENODEV`.
    ParamUnsupported { param: u64 },
    /// `GEM_VM_CREATE`/`DESTROY` flags or extensions non-zero.
    VmFlags { flags: u32 },

    // ---- activity: busy / wait / retire -------------------------------------------------------------------------------------
    /// `GEM_WAIT` flags non-zero (gem/i915_gem_wait.c:240-241).
    WaitFlags { flags: u32 },
    /// The object is still busy after the timeout (`-ETIME`, gem/i915_gem_wait.c:184).
    WaitTimeout,
    /// Busy beyond what the timeout accounting can express: restart (`-EAGAIN`, gem/i915_gem_wait.c:271-272).
    WaitRestart,
    /// A reset named a request the table does not hold (never made, or already retired).
    RequestUnknown { engine: usize, engine_ctx: u32, seqno: u32 },

    // ---- execbuffer ---------------------------------------------------------------------------------------------------------
    /// The shared uAPI validation refused (every `Exec*` refusal of `i915-uapi-core`).
    Uapi(UapiError),
    /// The same object twice in one submission. PORT RULE (Linux tolerates it by hashing handles into one `eb_vma`).
    ExecObjectDuplicate { handle: u32 },
    /// The context has no pinned engine context for the selected engine; the executor must create and attach one
    /// (`eb_pin_engine` -> `intel_context_pin_ww`, gem/i915_gem_execbuffer.c:2550-2599).
    ExecEngineContextMissing { ctx: u32, class: u16, instance: u16 },
    /// A relocation that needs patching was handed to a request whose owner object has no resident pages.
    ExecRelocOwnerNoPages { handle: u32 },
    /// Placement failed after every `eb_reserve` pass (gem/i915_gem_execbuffer.c:735-821).
    ExecReserve { handle: u32, why: MemError },
    /// A softpinned object is bound elsewhere and cannot be moved (it is in use by an unretired request).
    ExecPinnedMisplaced { handle: u32, want: u64, have: u64 },
    /// The memory planner refused for a reason that is not a placement failure.
    Memory(MemError),
    /// `Engine::build_request` / `dequeue` refused.
    Submit(SubmitError),
    /// The syncobj handle table could not answer (the `SyncTables` implementation refused).
    SyncRefused { what: &'static str },
    /// No such engine slot.
    EngineIndex { index: usize },
}

impl From<UapiError> for GemError {
    fn from(e: UapiError) -> Self {
        GemError::Uapi(e)
    }
}
impl From<MemError> for GemError {
    fn from(e: MemError) -> Self {
        GemError::Memory(e)
    }
}
impl From<SubmitError> for GemError {
    fn from(e: SubmitError) -> Self {
        GemError::Submit(e)
    }
}

impl GemError {
    /// The Linux errno (positive) this refusal maps to.
    pub fn errno(&self) -> i32 {
        use GemError::*;
        match self {
            NoMem { .. } | HandleSpaceExhausted { .. } | VmPagesMissing | PagesRefused { .. } => ENOMEM,
            SessionGone { .. } => ENODEV,
            RequestUnknown { .. } | HandleNotInSession { .. } | CrossSessionHandle { .. } | StaleHandle { .. } | ContextMissing { .. } | VmMissing { .. }
            | EngineMissing { .. } | LeaseUnknown { .. } => ENOENT,
            CloseHandle { .. } | CreateZeroSize | CreateFlags { .. } | CreateRegionUnsupported { .. } | CreateNeedsCpuAccess | MadviseInvalid { .. }
            | CachingInvalid { .. } | SetDomainGpuDomain { .. } | SetDomainWriteNotRead { .. } | TilingNotOk { .. } | MmapFlags { .. }
            | MmapRange { .. } | ContextCreateFlags { .. } | ParamSize { .. } | ParamUnknown { .. } | PriorityRange { .. }
            | EnginesTwice | EnginesTooMany { .. } | VmFlags { .. } | WaitFlags { .. }
            | ExecObjectDuplicate { .. } | PagesMalformed { .. } => EINVAL,
            CreateTooBig { .. } => 7, // E2BIG, errno-base.h:11
            CreateProtectedUnsupported | CreateSetPatUnsupported | CachingUnsupported | MmapTypeUnavailable { .. } | ParamUnsupported { .. } => ENODEV,
            ObjectPurged { .. } | ExecRelocOwnerNoPages { .. } => EFAULT,
            TilingNoFences => EOPNOTSUPP,
            TilingFramebuffer { .. } => EBUSY,
            MmapOffsetDenied { .. } => EACCES,
            UserptrRefused { size } => {
                if *size == 0 {
                    EINVAL
                } else {
                    ENODEV
                }
            }
            ClientBanned { .. } | ContextBanned { .. } | Submit(_) => EIO,
            ParamPermission { .. } => EPERM,
            WaitTimeout => ETIME,
            WaitRestart => EAGAIN,
            Uapi(e) => e.errno().code(),
            ExecEngineContextMissing { .. } | SyncRefused { .. } | EngineIndex { .. } => EINVAL,
            ExecReserve { why, .. } | Memory(why) => mem_errno(why),
            ExecPinnedMisplaced { .. } => EINVAL,
        }
    }

    /// The stable name used in markers.
    pub fn name(&self) -> &'static str {
        use GemError::*;
        match self {
            SessionGone { .. } => "SESSION-GONE",
            HandleNotInSession { .. } => "HANDLE-NOT-IN-SESSION",
            CloseHandle { cross: true, .. } => "CROSS-SESSION-HANDLE",
            CloseHandle { cross: false, .. } => "CLOSE-HANDLE-INVALID",
            CrossSessionHandle { .. } => "CROSS-SESSION-HANDLE",
            StaleHandle { .. } => "STALE-HANDLE",
            CreateZeroSize => "CREATE-ZERO-SIZE",
            CreateTooBig { .. } => "CREATE-TOO-BIG",
            CreateFlags { .. } => "CREATE-FLAGS",
            CreateRegionUnsupported { .. } => "CREATE-REGION-UNSUPPORTED",
            CreateNeedsCpuAccess => "CREATE-NEEDS-CPU-ACCESS",
            CreateProtectedUnsupported => "CREATE-PROTECTED-UNSUPPORTED",
            CreateSetPatUnsupported => "CREATE-SET-PAT-UNSUPPORTED",
            MadviseInvalid { .. } => "MADVISE-INVALID",
            ObjectPurged { .. } => "OBJECT-PURGED",
            PagesRefused { .. } => "PAGES-REFUSED",
            PagesMalformed { .. } => "PAGES-MALFORMED",
            CachingInvalid { .. } => "CACHING-INVALID",
            CachingUnsupported => "CACHING-UNSUPPORTED",
            SetDomainGpuDomain { .. } => "SET-DOMAIN-GPU-DOMAIN",
            SetDomainWriteNotRead { .. } => "SET-DOMAIN-WRITE-NOT-READ",
            TilingNotOk { .. } => "TILING-NOT-OK",
            TilingNoFences => "TILING-NO-FENCES",
            TilingFramebuffer { .. } => "TILING-FRAMEBUFFER",
            MmapFlags { .. } => "MMAP-FLAGS",
            MmapTypeUnavailable { .. } => "MMAP-TYPE-UNAVAILABLE",
            MmapRange { .. } => "MMAP-RANGE",
            MmapOffsetDenied { .. } => "MMAP-OFFSET-DENIED",
            LeaseUnknown { .. } => "LEASE-UNKNOWN",
            UserptrRefused { .. } => "USERPTR-REFUSED",
            ContextMissing { .. } => "CONTEXT-MISSING",
            ClientBanned { .. } => "CLIENT-BANNED",
            ContextBanned { .. } => "CONTEXT-BANNED",
            ContextCreateFlags { .. } => "CONTEXT-CREATE-FLAGS",
            VmMissing { .. } => "VM-MISSING",
            NoMem { .. } => "GEM-NOMEM",
            HandleSpaceExhausted { .. } => "HANDLE-SPACE-EXHAUSTED",
            VmPagesMissing => "VM-PAGES-MISSING",
            ParamSize { .. } => "PARAM-SIZE",
            ParamUnknown { .. } => "PARAM-UNKNOWN",
            PriorityRange { .. } => "PRIORITY-RANGE",
            ParamPermission { .. } => "PARAM-PERMISSION",
            EnginesTwice => "ENGINES-TWICE",
            EnginesTooMany { .. } => "ENGINES-TOO-MANY",
            EngineMissing { .. } => "ENGINE-MISSING",
            ParamUnsupported { .. } => "PARAM-UNSUPPORTED",
            VmFlags { .. } => "VM-FLAGS",
            WaitFlags { .. } => "WAIT-FLAGS",
            WaitTimeout => "WAIT-TIMEOUT",
            WaitRestart => "WAIT-RESTART",
            RequestUnknown { .. } => "REQUEST-UNKNOWN",
            Uapi(e) => e.name(),
            ExecObjectDuplicate { .. } => "EXEC-OBJECT-DUPLICATE",
            ExecEngineContextMissing { .. } => "EXEC-ENGINE-CONTEXT-MISSING",
            ExecRelocOwnerNoPages { .. } => "EXEC-RELOC-OWNER-NO-PAGES",
            ExecReserve { .. } => "EXEC-RESERVE",
            ExecPinnedMisplaced { .. } => "EXEC-PINNED-MISPLACED",
            Memory(e) => e.name(),
            Submit(e) => e.name(),
            SyncRefused { .. } => "SYNC-REFUSED",
            EngineIndex { .. } => "ENGINE-INDEX",
        }
    }
}

/// The errno a placement failure maps to: no room is `-ENOSPC` (`i915_gem_gtt_insert`, i915_gem_gtt.c:189-); a bad request `-EINVAL`.
fn mem_errno(e: &MemError) -> i32 {
    match e {
        MemError::NoSpace { .. } | MemError::RangeOccupied { .. } => ENOSPC,
        MemError::OutOfMemory { .. } | MemError::TablePageRefused { .. } => ENOMEM,
        _ => EINVAL,
    }
}
