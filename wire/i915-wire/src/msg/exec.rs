//! EXECBUFFER2, flattened. One RPC per submission: the request is ONE payload holding the header, the object table, the relocation
//! table, the fence array and the extension records. GPU data (the batch, the buffers) stays in the objects' own memory; only
//! tables travel. Objects are named by session handle, the batch is one of them, and every pointer of `drm_i915_gem_execbuffer2`
//! (`buffers_ptr`, `relocs_ptr`, `cliprects_ptr`, `next_extension`) is replaced by the table that follows.
//!
//! ```text
//!  off size field                (request header, 64 bytes)
//!    0   8  flags                drm_i915_gem_execbuffer2::flags
//!    8   8  rsvd1                context id in the low 32 bits
//!   16   8  rsvd2                low 32 bits: in-fence sync_file token under FENCE_IN / FENCE_SUBMIT
//!   24   4  buffer_count
//!   28   4  batch_start_offset
//!   32   4  batch_len
//!   36   4  dr1
//!   40   4  dr4
//!   44   4  num_cliprects        the fence count under FENCE_ARRAY
//!   48   4  cliprects_present    1 if the client's cliprects pointer was non-zero, else 0
//!   52   4  reloc_total          sum of every object's relocation_count
//!   56   4  ext_count            extension records
//!   60   4  reserved             zero
//!   64  ...  objects[buffer_count]  x 48   ([`ExecObject`])
//!       ...  relocs[reloc_total]    x 32   ([`Reloc`], in object order)
//!       ...  fences[num_cliprects]  x 8    ([`Fence`]; only with FENCE_ARRAY)
//!       ...  ext records: 24-byte header ([`ExtHeader`]) + body; TIMELINE_FENCES body = fence_count: u64, fences[] x 8, values[] x 8
//! ```
//! The reply is `status`, `errno`, an [`ExecRepHead`], `n_offsets` canonical `u64` offsets (the final placement of each object, in
//! request order) and `n_reloc_updates` [`RelocUpdate`]s (the `presumed_offset` Linux writes back into the client's relocations).
use super::{msg, Pad32};
use crate::codec::{Reader, Writer};
use crate::{Status, WireError};

pub const HEADER_BYTES: usize = 64;
pub const OBJECT_BYTES: usize = 48;
pub const RELOC_BYTES: usize = 32;
pub const FENCE_BYTES: usize = 8;
pub const EXT_HEADER_BYTES: usize = 24;
/// `DRM_I915_GEM_EXECBUFFER_EXT_TIMELINE_FENCES`.
pub const EXT_TIMELINE_FENCES: u32 = 0;
/// VALIDATION limits of one submission (the device checks the same numbers; see `i915-uapi-core::ExecLimits::DEFAULT`).
pub const MAX_BUFFERS: u32 = 1024;
pub const MAX_RELOCS: u32 = 1024;
pub const MAX_FENCES: u32 = 256;

/// `execbuffer2.flags` bits the shim sets (the rest of `i915_drm.h:1409-1550` is validated by the device).
pub mod flags {
    pub const RING_MASK: u64 = 0x3f;
    pub const NO_RELOC: u64 = 1 << 11;
    pub const HANDLE_LUT: u64 = 1 << 12;
    pub const BATCH_FIRST: u64 = 1 << 18;
    pub const FENCE_ARRAY: u64 = 1 << 19;
    pub const FENCE_OUT: u64 = 1 << 17;
    pub const FENCE_IN: u64 = 1 << 16;
    pub const FENCE_SUBMIT: u64 = 1 << 20;
    pub const USE_EXTENSIONS: u64 = 1 << 21;
}

msg!(
    /// The request header (see the module docs for the offsets).
    ExecHeader {
        flags: u64,
        rsvd1: u64,
        rsvd2: u64,
        buffer_count: u32,
        batch_start_offset: u32,
        batch_len: u32,
        dr1: u32,
        dr4: u32,
        num_cliprects: u32,
        cliprects_present: u32,
        reloc_total: u32,
        ext_count: u32,
        _reserved: Pad32,
    }
);

msg!(
    /// `drm_i915_gem_exec_object2` without `relocs_ptr`. `rsvd1` is `pad_to_size` under `EXEC_OBJECT_PAD_TO_SIZE`.
    ExecObject { handle: u32, relocation_count: u32, alignment: u64, offset: u64, flags: u64, rsvd1: u64, rsvd2: u64 }
);
msg!(
    /// `drm_i915_gem_relocation_entry`.
    Reloc { target_handle: u32, delta: u32, offset: u64, presumed_offset: u64, read_domains: u32, write_domain: u32 }
);
msg!(
    /// `drm_i915_gem_exec_fence`: `handle` is a syncobj handle of the session; `flags` is `I915_EXEC_FENCE_WAIT` (1) / `SIGNAL` (2).
    Fence { handle: u32, flags: u32 }
);
msg!(
    /// `i915_user_extension` without `next_extension`: the chain is the order of the records.
    ExtHeader { name: u32, flags: u32, rsvd0: u32, rsvd1: u32, rsvd2: u32, rsvd3: u32 }
);

msg!(
    /// `flags` bit 0: the request was handed to the engine now (else it is queued behind unsignalled fences); bit 1: an out
    /// sync_file was made and `out_fence` names it.
    ExecRepHead { request_id: u64, seqno: u32, flags: u32, out_fence: u32, n_offsets: u32, n_reloc_updates: u32, _pad: Pad32 }
);
pub const REP_SUBMITTED: u32 = 1 << 0;
pub const REP_OUT_FENCE: u32 = 1 << 1;

msg!(
    /// One relocation whose `presumed_offset` changed: `index` is its position in the request's relocation table.
    RelocUpdate { index: u32, _pad: Pad32, presumed_offset: u64 }
);

/// Encode a complete EXECBUFFER2 request from its tables. `ext` is the already-encoded extension records (`header.ext_count` of them).
/// The counts in `header` are overwritten from the slices, so they cannot disagree with the tables.
pub fn encode_request(out: &mut [u8], mut header: ExecHeader, objects: &[ExecObject], relocs: &[Reloc], fences: &[Fence], ext: &[u8]) -> Result<usize, WireError> {
    use super::Body;
    header.buffer_count = objects.len() as u32;
    header.reloc_total = relocs.len() as u32;
    if header.flags & flags::FENCE_ARRAY != 0 {
        header.num_cliprects = fences.len() as u32;
    }
    let mut w = Writer::new(out);
    header.encode(&mut w)?;
    for o in objects {
        o.encode(&mut w)?;
    }
    for r in relocs {
        r.encode(&mut w)?;
    }
    for f in fences {
        f.encode(&mut w)?;
    }
    w.bytes(ext)?;
    Ok(w.pos())
}

/// Encode one TIMELINE_FENCES extension record (header + body) into `w`.
pub fn put_timeline_fences(w: &mut Writer<'_>, fences: &[Fence], values: &[u64]) -> Result<(), WireError> {
    use super::Body;
    if fences.len() != values.len() {
        return Err(WireError::CountMismatch { what: "timeline_fences.values", declared: fences.len() as u64, actual: values.len() as u64 });
    }
    ExtHeader { name: EXT_TIMELINE_FENCES, ..Default::default() }.encode(w)?;
    w.u64(fences.len() as u64)?;
    for f in fences {
        f.encode(w)?;
    }
    for v in values {
        w.u64(*v)?;
    }
    Ok(())
}

/// A decoded EXECBUFFER2 reply.
#[derive(Clone, Copy, Debug)]
pub struct ExecReply<'a> {
    pub status: Status,
    pub errno: i32,
    pub head: ExecRepHead,
    offsets: &'a [u8],
    updates: &'a [u8],
}

impl<'a> ExecReply<'a> {
    pub fn decode(payload: &'a [u8]) -> Result<ExecReply<'a>, WireError> {
        use super::Body;
        let mut r = Reader::new(payload);
        let (status, errno) = super::decode_reply_head(&mut r)?;
        let head = ExecRepHead::decode(&mut r)?;
        let offsets = r.take("ExecRep.offsets", (head.n_offsets as usize).checked_mul(8).ok_or(WireError::BadValue { what: "ExecRep.n_offsets", got: head.n_offsets as u64 })?)?;
        let updates = r.take("ExecRep.updates", (head.n_reloc_updates as usize).checked_mul(16).ok_or(WireError::BadValue { what: "ExecRep.n_reloc_updates", got: head.n_reloc_updates as u64 })?)?;
        r.finish()?;
        Ok(ExecReply { status, errno, head, offsets, updates })
    }
    pub fn offset(&self, i: usize) -> Option<u64> {
        let b = self.offsets.get(i.checked_mul(8)?..)?.get(..8)?;
        let mut x = [0u8; 8];
        x.copy_from_slice(b);
        Some(u64::from_le_bytes(x))
    }
    pub fn update(&self, i: usize) -> Option<RelocUpdate> {
        use super::Body;
        let b = self.updates.get(i.checked_mul(16)?..)?.get(..16)?;
        RelocUpdate::from_payload(b).ok()
    }
}
