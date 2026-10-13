// SPDX-License-Identifier: MIT OR Apache-2.0
//! The PAYLOAD REGION (ABI 1.2): how a call larger than one inline frame travels.
//!
//! An inline frame carries at most [`crate::MAX_PAYLOAD`] (64 KiB) after its header - enough for every call's FIXED tables except a
//! full EXECBUFFER2 (1024 objects x 48 B + 1024 relocations x 32 B + fences exceed it) and a long QUERY reply. Those travel in a
//! region the CLIENT granted the device: the frame carries only a [`RegionRef`] (40 bytes, `FLAG_PAYLOAD_REGION` set), and the call's
//! own payload is `len` bytes at `offset` in the region. The region is the client's memory; the device copies the request OUT of it
//! once, before it looks at a byte, so the client cannot change a table after the device validated it.
//!
//! The reply travels inline unless it does not fit ([`crate::MAX_PAYLOAD`]); then, and only then, the device places it at
//! `reply_offset` in the same region (at most `reply_cap` bytes) and answers with a header flagged `FLAG_PAYLOAD_REGION` whose payload is
//! a `RegionRef` with `offset = reply_offset` and `len` = the bytes placed. A reply that needs the area when `reply_cap` is too small is
//! refused by name, never truncated.
//!
//! BOUNDS (validation limits, enforced and tested): `len` and `reply_cap` <= [`MAX_REGION_PAYLOAD`] (4 MiB); `offset + len` and
//! `reply_offset + reply_cap` must not overflow; the layout is [`REGION_LAYOUT`] exactly; reserved is zero. WHERE the region is, and how
//! the device reaches it, is the host's business (the device's `PayloadRegions` trait): the wire says only what to read.
//!
//! ```text
//!  off  size  field
//!   0    4    region        the client's region id (opaque to the device; the host resolves it)
//!   4    4    layout        REGION_LAYOUT = 1
//!   8    8    offset        request: where the call's payload starts; reply: where the reply starts
//!  16    4    len           request: bytes of call payload; reply: bytes of reply payload placed
//!  20    4    reply_cap     bytes available at reply_offset (0 = the client has no reply area)
//!  24    8    reply_offset  where a reply that does not fit inline may be placed
//!  32    4    reply_region  ABI 1.3: the region the reply area is in; meaningful ONLY with REPLY_ELSEWHERE in `reply_flags`, else zero
//!  36    4    reply_flags   ABI 1.3: bit 0 REPLY_ELSEWHERE; every other bit zero
//! ```
//!
//! WHY A SECOND REGION (ABI 1.3): the kernel delegates a region to a device read-only or read-write, per delegation. A client delegates
//! its request storage READ-ONLY and its reply storage READ-WRITE over its session, so a reply cannot be written into the request's region.
//! With REPLY_ELSEWHERE clear (every ABI 1.2 client) the reply area is in `region`, as before; the bytes 32..40 were reserved zero, so
//! a 1.2 frame decodes unchanged. The reply's own [`RegionRef`] names the region the bytes were placed in.
use crate::codec::{Reader, Writer};
use crate::WireError;

/// The ABI minor that introduced the payload region.
pub const REGION_SINCE_MINOR: u16 = 2;
/// The only region layout there is.
pub const REGION_LAYOUT: u32 = 1;
/// Bytes of a [`RegionRef`] (the whole inline payload of a region frame).
pub const REGION_REF_BYTES: usize = 40;
/// The most a region payload or reply area may be: 4 MiB. A VALIDATION limit, not a buffer size.
pub const MAX_REGION_PAYLOAD: u32 = 4 << 20;
/// `reply_flags` bit 0: the reply area is in [`RegionRef::reply_region`], not in [`RegionRef::region`] (ABI minor 3).
pub const REPLY_ELSEWHERE: u32 = 1;
/// The ABI minor that introduced the separate reply region.
pub const REPLY_REGION_SINCE_MINOR: u16 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegionRef {
    pub region: u32,
    pub offset: u64,
    pub len: u32,
    pub reply_cap: u32,
    pub reply_offset: u64,
    /// The region the reply area is in: equal to `region` unless the frame set REPLY_ELSEWHERE.
    pub reply_region: u32,
}

impl RegionRef {
    /// Decode the whole inline payload of a region frame: exactly [`REGION_REF_BYTES`], the layout this ABI defines, every bound held.
    pub fn decode(payload: &[u8]) -> Result<RegionRef, WireError> {
        let mut r = Reader::new(payload);
        let region = r.u32("region.region")?;
        let layout = r.u32("region.layout")?;
        if layout != REGION_LAYOUT {
            return Err(WireError::BadValue { what: "region.layout", got: layout as u64 });
        }
        let offset = r.u64("region.offset")?;
        let len = r.u32("region.len")?;
        let reply_cap = r.u32("region.reply_cap")?;
        let reply_offset = r.u64("region.reply_offset")?;
        let reply_region = r.u32("region.reply_region")?;
        let reply_flags = r.u32("region.reply_flags")?;
        r.finish()?;
        if reply_flags & !REPLY_ELSEWHERE != 0 {
            return Err(WireError::FieldReservedNonZero { what: "region.reply_flags", got: reply_flags as u64 });
        }
        if reply_flags & REPLY_ELSEWHERE == 0 && reply_region != 0 {
            return Err(WireError::FieldReservedNonZero { what: "region.reply_region", got: reply_region as u64 });
        }
        let reply_region = if reply_flags & REPLY_ELSEWHERE != 0 { reply_region } else { region };
        if len == 0 {
            return Err(WireError::BadValue { what: "region.len", got: 0 });
        }
        if len > MAX_REGION_PAYLOAD {
            return Err(WireError::LimitExceeded { what: "region.len", got: len as u64, max: MAX_REGION_PAYLOAD as u64 });
        }
        if reply_cap > MAX_REGION_PAYLOAD {
            return Err(WireError::LimitExceeded { what: "region.reply_cap", got: reply_cap as u64, max: MAX_REGION_PAYLOAD as u64 });
        }
        if offset.checked_add(len as u64).is_none() {
            return Err(WireError::BadValue { what: "region.offset+len", got: offset });
        }
        if reply_offset.checked_add(reply_cap as u64).is_none() {
            return Err(WireError::BadValue { what: "region.reply_offset+reply_cap", got: reply_offset });
        }
        Ok(RegionRef { region, offset, len, reply_cap, reply_offset, reply_region })
    }

    /// Write the [`REGION_REF_BYTES`]; returns that length.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut w = Writer::new(out);
        w.u32(self.region)?;
        w.u32(REGION_LAYOUT)?;
        w.u64(self.offset)?;
        w.u32(self.len)?;
        w.u32(self.reply_cap)?;
        w.u64(self.reply_offset)?;
        if self.reply_region == self.region {
            w.u64(0)?;
        } else {
            w.u32(self.reply_region)?;
            w.u32(REPLY_ELSEWHERE)?;
        }
        Ok(w.pos())
    }
}
