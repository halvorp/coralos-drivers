// SPDX-License-Identifier: GPL-2.0-only
//! The FLATTENED `DRM_IOCTL_I915_GEM_EXECBUFFER2` request: a bounded, pointer-free, transactional decode.
//!
//! Linux reads `drm_i915_gem_execbuffer2` and then follows `buffers_ptr`, each object's `relocs_ptr`, `cliprects_ptr` and the
//! `next_extension` chain through user memory (gem/i915_gem_execbuffer.c:3555-3643). Those pointers are process-local, so the shim
//! flattens every array into ONE payload (KBL-I915-PORT-PLAN section 2) and this module decodes it, snapshotting the control words
//! once. Nothing is allocated; the decoded request borrows the payload. A refusal leaves no state behind.
//!
//! ```text
//!  off size field
//!    0   8  flags              drm_i915_gem_execbuffer2::flags
//!    8   8  rsvd1              context id in the low 32 bits (i915_execbuffer2_get_context_id)
//!   16   8  rsvd2              low 32 bits: in-fence token under FENCE_IN / FENCE_SUBMIT (high half is output-only, ignored)
//!   24   4  buffer_count
//!   28   4  batch_start_offset
//!   32   4  batch_len
//!   36   4  dr1
//!   40   4  dr4
//!   44   4  num_cliprects      fence count under FENCE_ARRAY
//!   48   4  cliprects_present  1 if userspace's cliprects_ptr was non-zero, else 0
//!   52   4  reloc_total        sum of every object's relocation_count
//!   56   4  ext_count          extension records
//!   60   4  reserved           zero
//!   64  ...  objects[buffer_count]  x 48     (ExecObject2)
//!       ...  relocs[reloc_total]    x 32     (RelocationEntry, in object order)
//!       ...  fences[num_cliprects]  x 8      (ExecFence; only with FENCE_ARRAY)
//!       ...  ext records: 24-byte header + body; TIMELINE_FENCES body = fence_count: u64, fences[] x 8, values[] x 8
//! ```
use crate::codec::{u32_at, u64_at, Reader};
use crate::consts::exec as ef;
use crate::consts::exec_fence as xf;
use crate::error::UapiError;
use crate::types::{ExecFence, ExecObject2, ExtensionHeader, RelocationEntry};

pub const EXEC_HEADER_BYTES: usize = 64;
/// KEPT AS VALIDATION (D10 / RUNTIME-TABLES-HAVE-NO-CAP): Linux itself enforces this depth (the 513th extension is `-E2BIG`) to refuse a malformed chain; nothing is stored.
/// `stackdepth = 512` (i915_user_extensions.c:21): the 513th extension of one chain is `-E2BIG`.
pub const MAX_EXTENSIONS: u32 = 512; // i915_user_extensions.c:21
/// KEPT AS VALIDATION (D10 / RUNTIME-TABLES-HAVE-NO-CAP): Linux's own per-object relocation-count validation limit; nothing is stored.
/// `N_RELOC(INT_MAX)` = `INT_MAX / sizeof(struct drm_i915_gem_relocation_entry)` (i915_gem_execbuffer.c:1514,1520,1628).
pub const MAX_RELOCS_PER_OBJECT_LINUX: u32 = (i32::MAX as u32) / (RelocationEntry::WIRE_BYTES as u32); // 67108863

/// The bounds this port enforces on one request, on top of Linux's own. Every one is checked and tested.
/// KEPT AS VALIDATION (D10 / RUNTIME-TABLES-HAVE-NO-CAP): each refuses an oversized or malformed ONE request (the decoder borrows the payload and allocates nothing); none is a table of
/// objects the device keeps. Linux bounds the same quantities by `INT_MAX`/`kvmalloc` failure (`-ENOMEM`); these are the port's explicit
/// form of that, and the per-device populations (objects, VMAs, requests) live in `i915-gem-core`, which grows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecLimits {
    /// Objects per submit (Linux allows up to `INT_MAX` and runs out of memory; the payload cap makes ~1365 the hard ceiling).
    pub max_buffers: u32,
    /// Relocation entries per submit, summed over every object.
    pub max_relocs_total: u32,
    /// Syncobj fences per submit: the fence array plus every timeline block.
    pub max_fences: u32,
    /// Bytes of payload (never above `i915_wire::region::MAX_REGION_PAYLOAD`: a payload larger than one inline frame arrives through a
    /// payload region, ABI 1.2; the inline limit itself is the frame decoder's).
    pub max_payload: u32,
}

impl ExecLimits {
    pub const DEFAULT: ExecLimits = ExecLimits { max_buffers: 1024, max_relocs_total: 1024, max_fences: 256, max_payload: i915_wire::region::MAX_REGION_PAYLOAD };
}

impl Default for ExecLimits {
    fn default() -> Self {
        ExecLimits::DEFAULT
    }
}

/// A borrowed, bounds-checked table of fixed-size records.
#[derive(Clone, Copy, Debug)]
pub struct Table<'a> {
    bytes: &'a [u8],
    stride: usize,
}

impl<'a> Table<'a> {
    pub fn len(&self) -> usize {
        self.bytes.len() / self.stride
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    fn at(&self, i: usize) -> Option<&'a [u8]> {
        self.bytes.get(i.checked_mul(self.stride)?..)?.get(..self.stride)
    }
}

macro_rules! table_get {
    ($name:ident, $ty:ty, $n:expr) => {
        impl<'a> $name<'a> {
            pub fn len(&self) -> usize {
                self.0.len()
            }
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
            pub fn get(&self, i: usize) -> Option<$ty> {
                let b = self.0.at(i)?;
                let mut a = [0u8; $n];
                a.copy_from_slice(b);
                Some(<$ty>::decode(&a))
            }
            pub fn iter(&self) -> impl Iterator<Item = $ty> + '_ {
                (0..self.len()).filter_map(move |i| self.get(i))
            }
        }
    };
}

#[derive(Clone, Copy, Debug)]
pub struct Objects<'a>(Table<'a>);
#[derive(Clone, Copy, Debug)]
pub struct Relocs<'a>(Table<'a>);
#[derive(Clone, Copy, Debug)]
pub struct Fences<'a>(Table<'a>);
table_get!(Objects, ExecObject2, 48);
table_get!(Relocs, RelocationEntry, 32);
table_get!(Fences, ExecFence, 8);

/// One `drm_i915_gem_execbuffer_ext_timeline_fences` block (i915_drm.h:1354): `fence_count` fences and as many points.
#[derive(Clone, Copy, Debug)]
pub struct TimelineBlock<'a> {
    pub fences: Fences<'a>,
    values: &'a [u8],
}

impl<'a> TimelineBlock<'a> {
    pub fn len(&self) -> usize {
        self.fences.len()
    }
    pub fn is_empty(&self) -> bool {
        self.fences.is_empty()
    }
    /// The timeline point of fence `i`.
    pub fn value(&self, i: usize) -> Option<u64> {
        let b = self.values.get(i.checked_mul(8)?..)?.get(..8)?;
        Some(u64_at(b, 0))
    }
}

/// The decoded request. It borrows the payload.
#[derive(Clone, Copy, Debug)]
pub struct ExecbufRequest<'a> {
    pub flags: u64,
    pub rsvd1: u64,
    pub rsvd2: u64,
    pub batch_start_offset: u32,
    pub batch_len: u32,
    pub dr1: u32,
    pub dr4: u32,
    pub num_cliprects: u32,
    pub cliprects_present: bool,
    pub objects: Objects<'a>,
    pub relocs: Relocs<'a>,
    /// The `FENCE_ARRAY` fences (empty without the flag).
    pub fences: Fences<'a>,
    ext_bytes: &'a [u8],
    pub ext_count: u32,
}

impl<'a> ExecbufRequest<'a> {
    /// The context id: `lower_32_bits(rsvd1)` (`i915_execbuffer2_get_context_id`, i915_drm.h:1570).
    pub fn context_id(&self) -> u32 {
        self.rsvd1 as u32
    }
    /// The in-fence token: `lower_32_bits(rsvd2)` (i915_gem_execbuffer.c:3446).
    pub fn in_fence_token(&self) -> u32 {
        self.rsvd2 as u32
    }
    /// Walk the extension records, yielding each TIMELINE_FENCES block. The chain was fully checked by [`decode_execbuf2`].
    pub fn timeline_blocks(&self) -> impl Iterator<Item = TimelineBlock<'a>> + '_ {
        let mut off = 0usize;
        let bytes = self.ext_bytes;
        core::iter::from_fn(move || {
            if off >= bytes.len() {
                return None;
            }
            let body = off + ExtensionHeader::WIRE_BYTES;
            let count = u64_at(bytes, body) as usize; // bounded by decode
            let f0 = body + 8;
            let v0 = f0 + count * ExecFence::WIRE_BYTES;
            let end = v0 + count * 8;
            let blk = TimelineBlock {
                fences: Fences(Table { bytes: &bytes[f0..v0], stride: ExecFence::WIRE_BYTES }),
                values: &bytes[v0..end],
            };
            off = end;
            Some(blk)
        })
    }
    /// Total syncobj fences carried (array + every timeline block).
    pub fn fence_total(&self) -> u64 {
        self.fences.len() as u64 + self.timeline_blocks().map(|b| b.len() as u64).sum::<u64>()
    }
}

/// `check_buffer_count` (i915_gem_execbuffer.c:3541-3553).
fn check_buffer_count(count: u32) -> Result<(), UapiError> {
    // `count < 1 || count > INT_MAX`; the SIZE_MAX / sz term is unreachable for a u32 on a 64-bit size_t.
    if count < 1 || count > i32::MAX as u32 {
        return Err(UapiError::ExecBufferCount { count });
    }
    Ok(())
}

/// `i915_gem_check_execbuffer` (i915_gem_execbuffer.c:2180-2203), over the decoded header words. `DR4 == 0xffffffff` is the UXA
/// garbage value Linux fixes up to zero before the test (:2193-2196).
fn check_execbuffer(flags: u64, num_cliprects: u32, cliprects_present: bool, dr1: u32, dr4: u32, start: u32, len: u32) -> Result<(), UapiError> {
    if flags & ILLEGAL_FLAGS != 0 {
        return Err(UapiError::ExecIllegalFlags { flags: flags & ILLEGAL_FLAGS }); // :2183-2184
    }
    if flags & (ef::I915_EXEC_FENCE_ARRAY | ef::I915_EXEC_USE_EXTENSIONS) == 0 && (num_cliprects != 0 || cliprects_present) {
        return Err(UapiError::ExecCliprects { num: num_cliprects }); // :2187-2191
    }
    let dr4 = if dr4 == 0xffff_ffff { 0 } else { dr4 };
    if dr1 != 0 || dr4 != 0 {
        return Err(UapiError::ExecDrNonZero { dr1, dr4 }); // :2197-2198
    }
    if (start | len) & 0x7 != 0 {
        return Err(UapiError::ExecBatchUnaligned { start, len }); // :2200-2201
    }
    Ok(())
}

/// `__I915_EXEC_UNKNOWN_FLAGS` = `-(I915_EXEC_USE_EXTENSIONS << 1)` (i915_drm.h:1550).
pub const UNKNOWN_FLAGS: u64 = (ef::I915_EXEC_USE_EXTENSIONS << 1).wrapping_neg(); // i915_drm.h:1550
/// `__I915_EXEC_ILLEGAL_FLAGS` (i915_gem_execbuffer.c:73-76): unknown bits, `CONSTANTS_MASK`, `RESOURCE_STREAMER`.
pub const ILLEGAL_FLAGS: u64 = UNKNOWN_FLAGS | ef::I915_EXEC_CONSTANTS_MASK | ef::I915_EXEC_RESOURCE_STREAMER; // i915_gem_execbuffer.c:73

/// Decode one flattened EXECBUFFER2 payload against `limits`. The order of refusals follows Linux's: buffer count, then
/// `i915_gem_check_execbuffer`, then the tables, the fence array and the extension chain.
pub fn decode_execbuf2<'a>(payload: &'a [u8], limits: &ExecLimits) -> Result<ExecbufRequest<'a>, UapiError> {
    let cap = limits.max_payload.min(i915_wire::region::MAX_REGION_PAYLOAD) as usize;
    if payload.len() > cap {
        return Err(UapiError::LimitExceeded { what: "execbuf.payload_bytes", got: payload.len() as u64, max: cap as u64 });
    }
    let mut r = Reader::new(payload);
    let h = r.take("execbuf.header", EXEC_HEADER_BYTES)?;
    let flags = u64_at(h, 0);
    let rsvd1 = u64_at(h, 8);
    let rsvd2 = u64_at(h, 16);
    let buffer_count = u32_at(h, 24);
    let batch_start_offset = u32_at(h, 28);
    let batch_len = u32_at(h, 32);
    let dr1 = u32_at(h, 36);
    let dr4 = u32_at(h, 40);
    let num_cliprects = u32_at(h, 44);
    let cliprects_present_raw = u32_at(h, 48);
    let reloc_total = u32_at(h, 52);
    let ext_count = u32_at(h, 56);
    let reserved = u32_at(h, 60);
    if reserved != 0 {
        return Err(UapiError::ReservedNonZero { what: "execbuf.reserved", got: u64::from(reserved) });
    }
    if cliprects_present_raw > 1 {
        return Err(UapiError::ReservedNonZero { what: "execbuf.cliprects_present", got: u64::from(cliprects_present_raw) });
    }
    check_buffer_count(buffer_count)?;
    if buffer_count > limits.max_buffers {
        return Err(UapiError::LimitExceeded { what: "execbuf.buffer_count", got: u64::from(buffer_count), max: u64::from(limits.max_buffers) });
    }
    check_execbuffer(flags, num_cliprects, cliprects_present_raw == 1, dr1, dr4, batch_start_offset, batch_len)?;

    let objects = Table { bytes: r.take("execbuf.objects", buffer_count as usize * ExecObject2::WIRE_BYTES)?, stride: ExecObject2::WIRE_BYTES };
    // Per-object relocation counts: Linux's own bound (i915_gem_execbuffer.c:1520, :1628) and this port's total.
    let mut sum: u64 = 0;
    for i in 0..objects.len() {
        let b = objects.at(i).ok_or(UapiError::Truncated { what: "execbuf.objects", need: (i + 1) * ExecObject2::WIRE_BYTES, have: objects.bytes.len() })?;
        let (handle, n) = (u32_at(b, 0), u32_at(b, 4));
        if n > MAX_RELOCS_PER_OBJECT_LINUX {
            return Err(UapiError::ExecRelocCount { handle, count: n, max: MAX_RELOCS_PER_OBJECT_LINUX });
        }
        sum += u64::from(n);
    }
    if sum > u64::from(limits.max_relocs_total) {
        return Err(UapiError::LimitExceeded { what: "execbuf.relocation_total", got: sum, max: u64::from(limits.max_relocs_total) });
    }
    if sum != u64::from(reloc_total) {
        return Err(UapiError::WireCountMismatch { what: "execbuf.reloc_total", declared: u64::from(reloc_total), actual: sum });
    }
    let relocs = Table { bytes: r.take("execbuf.relocs", sum as usize * RelocationEntry::WIRE_BYTES)?, stride: RelocationEntry::WIRE_BYTES };

    // add_fence_array (:2919-2991): the array exists only with FENCE_ARRAY; `num_cliprects == 0` is "no fences".
    let nfence = if flags & ef::I915_EXEC_FENCE_ARRAY != 0 { u64::from(num_cliprects) } else { 0 };
    check_fence_count(nfence, 0, limits)?;
    let fences = Table { bytes: r.take("execbuf.fences", nfence as usize * ExecFence::WIRE_BYTES)?, stride: ExecFence::WIRE_BYTES };

    // parse_execbuf2_extensions (:3138-3157).
    let ext_start = r.pos();
    if flags & ef::I915_EXEC_USE_EXTENSIONS == 0 {
        if ext_count != 0 {
            return Err(UapiError::WireCountMismatch { what: "execbuf.ext_count", declared: u64::from(ext_count), actual: 0 });
        }
    } else {
        if flags & ef::I915_EXEC_FENCE_ARRAY != 0 {
            return Err(UapiError::ExecExtensionsWithFenceArray); // :3147-3148
        }
        if num_cliprects != 0 {
            return Err(UapiError::ExecExtensionsWithCliprects { num: num_cliprects }); // :3150-3151
        }
        if ext_count > MAX_EXTENSIONS {
            return Err(UapiError::ExecExtensionChainTooLong { count: ext_count }); // i915_user_extensions.c:28-29
        }
        let mut fences_so_far = nfence;
        for _ in 0..ext_count {
            let hb = r.take("execbuf.ext_header", ExtensionHeader::WIRE_BYTES)?;
            let mut a = [0u8; ExtensionHeader::WIRE_BYTES];
            a.copy_from_slice(hb);
            let eh = ExtensionHeader::decode(&a);
            // check_user_mbz on flags and rsvd[] (i915_user_extensions.c:31-39), BEFORE the name is looked at (:41-49).
            if eh.flags != 0 || eh.rsvd != [0; 4] {
                return Err(UapiError::ExecExtensionReserved { name: eh.name });
            }
            if eh.name != xf::DRM_I915_GEM_EXECBUFFER_EXT_TIMELINE_FENCES {
                return Err(UapiError::ExecExtensionUnknown { name: eh.name });
            }
            let count = r.u64("execbuf.timeline.fence_count")?;
            check_fence_count(count, fences_so_far, limits)?;
            fences_so_far += count;
            r.take("execbuf.timeline.fences", count as usize * ExecFence::WIRE_BYTES)?;
            r.take("execbuf.timeline.values", count as usize * 8)?;
        }
    }
    let ext_bytes = &payload[ext_start..r.pos()];
    if r.remaining() != 0 {
        return Err(UapiError::TrailingBytes { extra: r.remaining() });
    }
    Ok(ExecbufRequest {
        flags,
        rsvd1,
        rsvd2,
        batch_start_offset,
        batch_len,
        dr1,
        dr4,
        num_cliprects,
        cliprects_present: cliprects_present_raw == 1,
        objects: Objects(objects),
        relocs: Relocs(relocs),
        fences: Fences(fences),
        ext_bytes,
        ext_count,
    })
}

/// The fence-array bound: Linux's overflow guard (`nfences > min(ULONG_MAX / sizeof(...), ...) - num_fences`, :2801-2804, :2934-2937)
/// is "does not fit"; this port's cap is `limits.max_fences` over the running total.
fn check_fence_count(adding: u64, have: u64, limits: &ExecLimits) -> Result<(), UapiError> {
    if adding > u64::from(u32::MAX) {
        return Err(UapiError::ExecFenceCount { count: adding });
    }
    let total = have + adding;
    if total > u64::from(limits.max_fences) {
        return Err(UapiError::LimitExceeded { what: "execbuf.fences", got: total, max: u64::from(limits.max_fences) });
    }
    Ok(())
}
