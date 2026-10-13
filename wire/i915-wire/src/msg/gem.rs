//! GEM object calls. Handles are session-scoped and generation-tagged by the device: the client treats them as opaque `u32`s.
use super::{msg, Pad32};
use crate::codec::{Reader, Writer};
use crate::WireError;

msg!(GemCreateReq { size: u64 });
msg!(
    /// `size` is the size actually allocated (rounded up to a page).
    GemCreateRep { handle: u32, _pad: Pad32, size: u64 }
);

/// Memory-region placements per GEM-CREATE-EXT: Linux copies `num_regions` entries (gem/i915_gem_create.c set_placements); this
/// is the validation limit that keeps the request inside one payload.
pub const MAX_CREATE_REGIONS: u32 = 64;

msg!(
    /// The fixed head of GEM-CREATE-EXT. `region_count` `(class: u16, instance: u16)` pairs follow. `set_pat_present` is 0 or 1.
    GemCreateExtHead { size: u64, flags: u32, region_count: u32, protected: u32, set_pat_present: u32, set_pat: u32, _pad: Pad32 }
);

/// A decoded GEM-CREATE-EXT request.
#[derive(Clone, Copy, Debug)]
pub struct GemCreateExtReq<'a> {
    pub head: GemCreateExtHead,
    regions: &'a [u8],
}

impl<'a> GemCreateExtReq<'a> {
    pub fn decode(payload: &'a [u8]) -> Result<GemCreateExtReq<'a>, WireError> {
        let mut r = Reader::new(payload);
        let head = <GemCreateExtHead as super::Body>::decode(&mut r)?;
        if head.region_count > MAX_CREATE_REGIONS {
            return Err(WireError::LimitExceeded { what: "GemCreateExt.region_count", got: head.region_count as u64, max: MAX_CREATE_REGIONS as u64 });
        }
        if head.protected > 1 || head.set_pat_present > 1 {
            return Err(WireError::BadValue { what: "GemCreateExt.protected/set_pat_present", got: (head.protected | head.set_pat_present) as u64 });
        }
        let regions = r.take("GemCreateExt.regions", head.region_count as usize * 4)?;
        r.finish()?;
        Ok(GemCreateExtReq { head, regions })
    }
    pub fn region(&self, i: usize) -> Option<(u16, u16)> {
        let b = self.regions.get(i.checked_mul(4)?..)?.get(..4)?;
        Some((u16::from_le_bytes([b[0], b[1]]), u16::from_le_bytes([b[2], b[3]])))
    }
    pub fn region_count(&self) -> usize {
        self.regions.len() / 4
    }
    /// Encode a request: `head.region_count` is set from `regions`.
    pub fn encode(out: &mut [u8], mut head: GemCreateExtHead, regions: &[(u16, u16)]) -> Result<usize, WireError> {
        head.region_count = regions.len() as u32;
        let mut w = Writer::new(out);
        super::Body::encode(&head, &mut w)?;
        for (c, i) in regions {
            w.u16(*c)?;
            w.u16(*i)?;
        }
        Ok(w.pos())
    }
}

msg!(GemCloseReq { handle: u32, _pad: Pad32 });

msg!(
    /// `flags` is `I915_MMAP_OFFSET_{GTT,WC,WB,UC,FIXED}`.
    GemMmapOffsetReq { handle: u32, _pad: Pad32, flags: u64 }
);
msg!(
    /// `offset` is the fake offset to present to GEM-MMAP-OPEN (it is not a pointer and not an address). `kind` is the `MmapKind`
    /// (0 GTT, 1 WC, 2 WB, 3 UC, 4 FIXED); `cache` the CPU cache attribute the mapping must carry ([`cache`]).
    GemMmapOffsetRep { offset: u64, kind: u32, cache: u32 }
);

/// CPU cache attributes of a mapping lease.
pub mod cache {
    pub const WRITE_BACK: u32 = 0;
    pub const WRITE_COMBINING: u32 = 1;
    pub const UNCACHED: u32 = 2;
}

msg!(
    /// The `mmap(2)` of a fake offset: `len` bytes from the start of the object.
    GemMmapOpenReq { offset: u64, len: u64 }
);
msg!(
    /// The legacy `GEM_MMAP`: a range of one object by handle. `flags` is `I915_MMAP_WC` (1) or 0.
    GemMmapLegacyReq { handle: u32, _pad: Pad32, offset: u64, size: u64, flags: u64 }
);
msg!(
    /// A CPU-mapping LEASE (what the shim maps; never a pointer). The device grants access to `len` bytes of the object's memory
    /// starting at `grant_addr`, a DEVICE address inside a physically contiguous run, with the cache attribute `cache`. `cache`
    /// is [`cache`] and is what the lease IS (the pages carry one cache type; the reply never promises another). The lease stays valid until
    /// GEM-MMAP-CLOSE or the session dies; closing the GEM handle does NOT revoke it.
    ///
    /// ABI 1.3: the CPU maps it with the KERNEL: `region_cap` is the kernel lease handle (REGION_MAP's `cap`), `region_offset` the byte offset in
    /// that lease where the mapping starts, `len` how many bytes. The device revokes the kernel lease when the object is freed (after the GPU
    /// retired every request that touched it and its address-space entries are unbound) or when the session ends; a mapping read after that
    /// faults (SIGBUS). One kernel lease serves every GEM-MMAP-OPEN of the same object by the same session.
    GemMmapRep { lease: u32, cache: u32, grant_addr: u64, len: u64, region_cap: u64, region_offset: u64 }
);
msg!(GemMmapCloseReq { lease: u32, _pad: Pad32 });

msg!(
    /// `read_domains` / `write_domain` are `I915_GEM_DOMAIN_*` bits.
    GemSetDomainReq { handle: u32, read_domains: u32, write_domain: u32, _pad: Pad32 }
);

msg!(
    /// `swizzle_mode` is ignored on input (Linux ignores it too).
    GemSetTilingReq { handle: u32, tiling_mode: u32, stride: u32, swizzle_mode: u32 }
);
msg!(GemGetTilingReq { handle: u32, _pad: Pad32 });
msg!(TilingRep { tiling_mode: u32, stride: u32, swizzle_mode: u32, _pad: Pad32 });

msg!(GemBusyReq { handle: u32, _pad: Pad32 });
msg!(GemBusyRep { busy: u32, _pad: Pad32 });

msg!(
    /// `timeout_ns` is RELATIVE: negative waits forever, zero is a poll. The reply carries the time left. A wait that has to
    /// block is answered LATER, by the same request id, when the request completes or the timeout runs out (`-ETIME`).
    GemWaitReq { handle: u32, flags: u32, timeout_ns: i64 }
);
msg!(GemWaitRep { timeout_ns: i64 });

msg!(GemMadviseReq { handle: u32, madv: u32 });
msg!(GemMadviseRep { retained: u32, _pad: Pad32 });

msg!(GemSetCachingReq { handle: u32, caching: u32 });
msg!(GemGetCachingReq { handle: u32, _pad: Pad32 });
msg!(GemCachingRep { caching: u32, _pad: Pad32 });
