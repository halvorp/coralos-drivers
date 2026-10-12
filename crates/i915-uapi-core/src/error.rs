// SPDX-License-Identifier: GPL-2.0-only
//! Every refusal of this crate: a NAMED error carrying the value and the bound, the Linux errno it maps to, and the Linux check it
//! ports (`file:line`, v7.0-rc6). Nothing is a bare `false` and nothing is a silent clamp (PORT_CONVENTIONS.md section 5).
use crate::errno::Errno;

/// Why a uAPI request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UapiError {
    // ---- the flattened wire form (no Linux counterpart: the shim flattens, the device decodes) ----------------------------
    /// The payload ended before a field or table the request declared. Linux: the user buffer is unreadable (`access_ok` /
    /// `copy_from_user` fail) -> `-EFAULT` (i915_gem_execbuffer.c:2807-2808, i915_query.c:27-29).
    Truncated { what: &'static str, need: usize, have: usize },
    /// Bytes remain after the last declared table: the request is not exactly what it says it is.
    TrailingBytes { extra: usize },
    /// A field the wire requires to be zero is not.
    ReservedNonZero { what: &'static str, got: u64 },
    /// A count in the fixed header disagrees with the tables that follow (declared vs actual).
    WireCountMismatch { what: &'static str, declared: u64, actual: u64 },
    /// A count the request declares exceeds this port's bound for it (a resource cap, not a Linux rule; Linux would run out of
    /// memory in `kvmalloc` for the same request, which is `-ENOMEM`). Names the quantity, the value and the bound.
    LimitExceeded { what: &'static str, got: u64, max: u64 },

    // ---- DRM_IOCTL_I915_GETPARAM (i915_getparam.c) -------------------------------------------------------------------
    /// Old UMS/DRI1 parameters are rejected outright: `-ENODEV` (i915_getparam.c:28-33).
    ParamLegacyUms { param: u32 },
    /// The parameter is not known to this kernel: `-EINVAL` (i915_getparam.c:201-203).
    ParamUnknown { param: u32 },
    /// The parameter is known but this device answers it with an error (HuC off, no PXP, no GuC, SSEU unknown, Xe_HP-only ...).
    /// `errno` is exactly what Linux returns; `why` names the Linux condition.
    ParamRefused { param: u32, errno: Errno, why: &'static str },
    /// The parameter exists in Linux but its machinery is deliberately not ported (i915 perf / OA): refused as an unknown
    /// parameter, which is what an older kernel answers and what Mesa treats as "perf unavailable". NOT a Linux value.
    ParamNotPorted { param: u32, why: &'static str },

    // ---- DRM_IOCTL_I915_QUERY (i915_query.c) -------------------------------------------------------------------------
    /// `drm_i915_query::flags != 0`: `-EINVAL` (i915_query.c:607-608).
    QueryFlagsNonZero { flags: u32 },
    /// `item.query_id == 0`: `-EINVAL` for the whole ioctl (i915_query.c:618-619).
    QueryIdZero,
    /// An id beyond the function table: the ITEM's length becomes `-EINVAL` (i915_query.c:624-630).
    QueryUnknownId { query_id: u64 },
    /// `item.flags` must be zero for this query (i915_query.c:97-98 topology, :475-476 memregions, :142-143 engine info).
    QueryItemFlags { query_id: u64, flags: u32 },
    /// `item.length` is non-zero but smaller than the data: `-EINVAL` (copy_query_item, i915_query.c:24-25).
    QueryLengthTooSmall { query_id: u64, have: i32, need: u32 },
    /// The header userspace passed in must be all zero (engine info i915_query.c:154-156, memory regions :490-495).
    QueryHeaderNotZero { query_id: u64 },
    /// The device has no such data: `-ENODEV` (fill_topology_info i915_query.c:46-47; geometry subslices :110-111; hwconfig :540-541;
    /// GuC submission version :567-568).
    QueryNoDevice { query_id: u64, why: &'static str },
    /// The caller's reply buffer cannot hold what the query produced (Linux: `copy_to_user` fails, `-EFAULT`).
    QueryReplyTooSmall { need: usize, have: usize },

    // ---- DRM_IOCTL_I915_GEM_EXECBUFFER2 (gem/i915_gem_execbuffer.c) --------------------------------------------------
    /// `buffer_count < 1 || > INT_MAX` (check_buffer_count, i915_gem_execbuffer.c:3541-3552).
    ExecBufferCount { count: u32 },
    /// `flags & __I915_EXEC_ILLEGAL_FLAGS` (unknown bits, `CONSTANTS_MASK`, `RESOURCE_STREAMER`; :2183-2184).
    ExecIllegalFlags { flags: u64 },
    /// `num_cliprects`/`cliprects_ptr` without `FENCE_ARRAY` or `USE_EXTENSIONS` ("kernel clipping was a DRI1 misfeature", :2187-2191).
    ExecCliprects { num: u32 },
    /// `DR1 || DR4` non-zero (`DR4 == 0xffffffff` is first fixed up to zero for UXA; :2193-2198).
    ExecDrNonZero { dr1: u32, dr4: u32 },
    /// `(batch_start_offset | batch_len) & 7` (:2200-2201).
    ExecBatchUnaligned { start: u32, len: u32 },
    /// `I915_EXEC_SECURE` on Gen11+: `-ENODEV` (:3385-3386).
    ExecSecureGen11 { ver: u8 },
    /// `I915_EXEC_SECURE` where secure batches do not exist (Gen6+): `-EPERM` "to trigger fallback code" (:3389-3390).
    ExecSecureUnsupported { ver: u8 },
    /// `USE_EXTENSIONS` together with `FENCE_ARRAY`: both want `cliprects_ptr` (:3147-3148).
    ExecExtensionsWithFenceArray,
    /// `USE_EXTENSIONS` with `num_cliprects != 0` (:3150-3151).
    ExecExtensionsWithCliprects { num: u32 },
    /// More than 512 extensions in the chain: `-E2BIG` (i915_user_extensions.c:21,28-29).
    ExecExtensionChainTooLong { count: u32 },
    /// Extension `flags`/`rsvd` not zero (`check_user_mbz`, i915_user_extensions.c:31-39).
    ExecExtensionReserved { name: u32 },
    /// An extension name this kernel has no handler for (i915_user_extensions.c:44-49; only TIMELINE_FENCES exists, i915_gem_execbuffer.c:3133-3135).
    ExecExtensionUnknown { name: u32 },
    /// Fence `flags` outside WAIT|SIGNAL (`__I915_EXEC_FENCE_UNKNOWN_FLAGS`, :2837, :2959).
    ExecFenceFlags { flags: u32 },
    /// A fence array longer than `ULONG_MAX / sizeof(...)`-style bounds, or than this port's bound (:2801-2804, :2934-2937).
    ExecFenceCount { count: u64 },
    /// `drm_syncobj_find` failed: `-ENOENT` (:2843-2847, :2962-2967).
    ExecSyncobjMissing { handle: u32 },
    /// A WAIT fence on a syncobj that holds no fence: `-EINVAL` (:2852-2858, :2969-2977).
    ExecSyncobjNoFence { handle: u32 },
    /// Wait and signal on the same non-zero timeline point (:2886-2897).
    ExecTimelineWaitAndSignal { handle: u32, point: u64 },
    /// Timeline wait on a point the syncobj does not have (and no SIGNAL): the `dma_fence_chain_find_seqno` error (:2860-2870).
    ExecTimelinePointMissing { handle: u32, point: u64 },
    /// `FENCE_IN` and `FENCE_SUBMIT` both set (:3409-3411).
    ExecInFenceBoth,
    /// `sync_file_get_fence` found nothing for the in-fence (:3413-3417).
    ExecInFenceMissing { token: u32 },
    /// `i915_gem_context_lookup(rsvd1)`: no such context, `-ENOENT` (:827-829, i915_gem_context.c:2340-2355).
    ExecContextMissing { id: u32 },
    /// The ring selector names no engine this device/context has: `-EINVAL` (eb_select_legacy_ring :2626-2667,
    /// i915_gem_context_get_engine i915_gem_context.h:213-216).
    ExecEngineInvalid { flags: u64, why: &'static str },
    /// `eb_lookup_vma`: no object with this handle in the file: `-ENOENT` (:907).
    ExecObjectMissing { handle: u32 },
    /// `entry->relocation_count` on Gen12+ except TGL (:507-509).
    ExecRelocsGen12 { ver: u8 },
    /// `entry->flags & invalid_flags` (unknown bits; `NEEDS_GTT` under full PPGTT; :511-512, :3367, :833).
    ExecObjectFlags { handle: u32, flags: u64, invalid: u64 },
    /// `alignment` not a power of two (:514-516).
    ExecAlignmentNotPow2 { handle: u32, alignment: u64 },
    /// `EXEC_OBJECT_PINNED` with an offset that is not page aligned or not canonical (:522-524).
    ExecPinnedOffset { handle: u32, offset: u64 },
    /// `EXEC_OBJECT_PAD_TO_SIZE` with `pad_to_size` not page aligned (:527-529).
    ExecPadToSize { handle: u32, pad_to_size: u64 },
    /// A softpinned range that does not fit the address space or the 4 GiB zone, or is not aligned: `-EINVAL`
    /// (i915_vma_insert, i915_vma.c:854-858, with `end` from :828-832).
    ExecPinnedRange { handle: u32, offset: u64, size: u64, end: u64 },
    /// Two softpinned objects of one submission overlap: the second reservation fails (`-ENOSPC`: i915_gem_gtt_reserve,
    /// i915_gem_gtt.c:115-120, can only evict neighbours that are not part of this submission).
    ExecPinnedOverlap { a: u32, b: u32 },
    /// The batch object is flagged `EXEC_OBJECT_WRITE` ("self-modifying batch buffer", :602-606).
    ExecBatchWritable { handle: u32 },
    /// `batch_start_offset + batch_len` exceeds the batch object (:608-614).
    ExecBatchOutOfBounds { start: u32, len: u32, size: u64 },
    /// The batch length resolves to zero (:621-624).
    ExecBatchLenZero,
    /// A per-object relocation count above `N_RELOC(INT_MAX)` (:1520-1521, :1628-1629) or this port's bound.
    ExecRelocCount { handle: u32, count: u32, max: u32 },
    /// The relocation target is not in the object list (a LUT index out of range, or an unknown handle): `-ENOENT` (:1418-1420).
    ExecRelocTargetMissing { handle: u32, target: u32 },
    /// A relocation with more than one bit in `write_domain` (:1423-1433).
    ExecRelocWriteDomains { write_domain: u32 },
    /// A relocation with a domain outside `I915_GEM_GPU_DOMAINS` (:1434-1445).
    ExecRelocDomain { read_domains: u32, write_domain: u32 },
    /// `reloc->offset` beyond the object minus the pointer width (:1480-1488).
    ExecRelocBounds { handle: u32, offset: u64, size: u64 },
    /// `reloc->offset & 3` (:1489-1494).
    ExecRelocUnaligned { handle: u32, offset: u64 },
}

impl UapiError {
    /// The Linux errno this refusal maps to.
    pub const fn errno(&self) -> Errno {
        use Errno::*;
        use UapiError::*;
        match self {
            Truncated { .. } => EFAULT,
            TrailingBytes { .. } | ReservedNonZero { .. } | WireCountMismatch { .. } => EINVAL,
            LimitExceeded { .. } => ENOMEM,
            ParamLegacyUms { .. } => ENODEV,
            ParamUnknown { .. } | ParamNotPorted { .. } => EINVAL,
            ParamRefused { errno, .. } => *errno,
            QueryFlagsNonZero { .. } | QueryIdZero | QueryUnknownId { .. } | QueryItemFlags { .. } | QueryLengthTooSmall { .. }
            | QueryHeaderNotZero { .. } => EINVAL,
            QueryNoDevice { .. } => ENODEV,
            QueryReplyTooSmall { .. } => EFAULT,
            ExecBufferCount { .. } | ExecIllegalFlags { .. } | ExecCliprects { .. } | ExecDrNonZero { .. } | ExecBatchUnaligned { .. } => EINVAL,
            ExecSecureGen11 { .. } => ENODEV,
            ExecSecureUnsupported { .. } => EPERM,
            ExecExtensionsWithFenceArray | ExecExtensionsWithCliprects { .. } => EINVAL,
            ExecExtensionChainTooLong { .. } => E2BIG,
            ExecExtensionReserved { .. } | ExecExtensionUnknown { .. } | ExecFenceFlags { .. } | ExecFenceCount { .. } => EINVAL,
            ExecSyncobjMissing { .. } => ENOENT,
            ExecSyncobjNoFence { .. } | ExecTimelineWaitAndSignal { .. } | ExecTimelinePointMissing { .. } | ExecInFenceBoth
            | ExecInFenceMissing { .. } => EINVAL,
            ExecContextMissing { .. } => ENOENT,
            ExecEngineInvalid { .. } => EINVAL,
            ExecObjectMissing { .. } => ENOENT,
            ExecRelocsGen12 { .. } | ExecObjectFlags { .. } | ExecAlignmentNotPow2 { .. } | ExecPinnedOffset { .. } | ExecPadToSize { .. }
            | ExecPinnedRange { .. } => EINVAL,
            ExecPinnedOverlap { .. } => ENOSPC,
            ExecBatchWritable { .. } | ExecBatchOutOfBounds { .. } | ExecBatchLenZero | ExecRelocCount { .. } => EINVAL,
            ExecRelocTargetMissing { .. } => ENOENT,
            ExecRelocWriteDomains { .. } | ExecRelocDomain { .. } | ExecRelocBounds { .. } | ExecRelocUnaligned { .. } => EINVAL,
        }
    }

    /// The stable name used in markers (`[I915-UAPI REFUSED why=EXEC-RELOC-BOUNDS ...]`).
    pub const fn name(&self) -> &'static str {
        use UapiError::*;
        match self {
            Truncated { .. } => "WIRE-TRUNCATED",
            TrailingBytes { .. } => "WIRE-TRAILING-BYTES",
            ReservedNonZero { .. } => "WIRE-RESERVED-NONZERO",
            WireCountMismatch { .. } => "WIRE-COUNT-MISMATCH",
            LimitExceeded { .. } => "LIMIT-EXCEEDED",
            ParamLegacyUms { .. } => "PARAM-LEGACY-UMS",
            ParamUnknown { .. } => "PARAM-UNKNOWN",
            ParamRefused { .. } => "PARAM-REFUSED",
            ParamNotPorted { .. } => "PARAM-NOT-PORTED",
            QueryFlagsNonZero { .. } => "QUERY-FLAGS-NONZERO",
            QueryIdZero => "QUERY-ID-ZERO",
            QueryUnknownId { .. } => "QUERY-UNKNOWN-ID",
            QueryItemFlags { .. } => "QUERY-ITEM-FLAGS",
            QueryLengthTooSmall { .. } => "QUERY-LENGTH-TOO-SMALL",
            QueryHeaderNotZero { .. } => "QUERY-HEADER-NOT-ZERO",
            QueryNoDevice { .. } => "QUERY-NO-DEVICE",
            QueryReplyTooSmall { .. } => "QUERY-REPLY-TOO-SMALL",
            ExecBufferCount { .. } => "EXEC-BUFFER-COUNT",
            ExecIllegalFlags { .. } => "EXEC-ILLEGAL-FLAGS",
            ExecCliprects { .. } => "EXEC-CLIPRECTS",
            ExecDrNonZero { .. } => "EXEC-DR-NONZERO",
            ExecBatchUnaligned { .. } => "EXEC-BATCH-UNALIGNED",
            ExecSecureGen11 { .. } => "EXEC-SECURE-GEN11",
            ExecSecureUnsupported { .. } => "EXEC-SECURE-UNSUPPORTED",
            ExecExtensionsWithFenceArray => "EXEC-EXT-WITH-FENCE-ARRAY",
            ExecExtensionsWithCliprects { .. } => "EXEC-EXT-WITH-CLIPRECTS",
            ExecExtensionChainTooLong { .. } => "EXEC-EXT-CHAIN-TOO-LONG",
            ExecExtensionReserved { .. } => "EXEC-EXT-RESERVED",
            ExecExtensionUnknown { .. } => "EXEC-EXT-UNKNOWN",
            ExecFenceFlags { .. } => "EXEC-FENCE-FLAGS",
            ExecFenceCount { .. } => "EXEC-FENCE-COUNT",
            ExecSyncobjMissing { .. } => "EXEC-SYNCOBJ-MISSING",
            ExecSyncobjNoFence { .. } => "EXEC-SYNCOBJ-NO-FENCE",
            ExecTimelineWaitAndSignal { .. } => "EXEC-TIMELINE-WAIT-AND-SIGNAL",
            ExecTimelinePointMissing { .. } => "EXEC-TIMELINE-POINT-MISSING",
            ExecInFenceBoth => "EXEC-IN-FENCE-BOTH",
            ExecInFenceMissing { .. } => "EXEC-IN-FENCE-MISSING",
            ExecContextMissing { .. } => "EXEC-CONTEXT-MISSING",
            ExecEngineInvalid { .. } => "EXEC-ENGINE-INVALID",
            ExecObjectMissing { .. } => "EXEC-OBJECT-MISSING",
            ExecRelocsGen12 { .. } => "EXEC-RELOCS-GEN12",
            ExecObjectFlags { .. } => "EXEC-OBJECT-FLAGS",
            ExecAlignmentNotPow2 { .. } => "EXEC-ALIGNMENT-NOT-POW2",
            ExecPinnedOffset { .. } => "EXEC-PINNED-OFFSET",
            ExecPadToSize { .. } => "EXEC-PAD-TO-SIZE",
            ExecPinnedRange { .. } => "EXEC-PINNED-RANGE",
            ExecPinnedOverlap { .. } => "EXEC-PINNED-OVERLAP",
            ExecBatchWritable { .. } => "EXEC-BATCH-WRITABLE",
            ExecBatchOutOfBounds { .. } => "EXEC-BATCH-OUT-OF-BOUNDS",
            ExecBatchLenZero => "EXEC-BATCH-LEN-ZERO",
            ExecRelocCount { .. } => "EXEC-RELOC-COUNT",
            ExecRelocTargetMissing { .. } => "EXEC-RELOC-TARGET-MISSING",
            ExecRelocWriteDomains { .. } => "EXEC-RELOC-WRITE-DOMAINS",
            ExecRelocDomain { .. } => "EXEC-RELOC-DOMAIN",
            ExecRelocBounds { .. } => "EXEC-RELOC-BOUNDS",
            ExecRelocUnaligned { .. } => "EXEC-RELOC-UNALIGNED",
        }
    }
}
