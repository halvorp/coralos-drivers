//! REGION_MAP (113) / REGION_UNMAP (114) / DMA_LEASE_CREATE (115) / DMA_LEASE_REVOKE (116): a `.device` reads a client's payload and gives a
//! client CPU access to the device's own DMA pages, with the kernel scoping both to the client's SESSION (2026-10-13).
//!
//! The words, the argument block and the rules are the kernel's (`coral_firmament_abi::region_map`, docs/doctrine/REGION-MAP-IS-SESSION-SCOPED.md
//! and DMA-LEASE-IS-EXACT-AND-REVOCABLE.md); this file restates ONLY what a driver needs, and the tests pin every restated number.
//!
//! * [`region_map`] maps `[offset, offset+length)` of a region capability the client DELEGATED over its session endpoint (CAP_DELEGATE: the
//!   `cap` is the slot number the delegation produced) - or of a lease handle - into the REGION_MAP window, with at most the rights the
//!   delegation carried. The kernel binds the mapping to that session incarnation and removes it, TLB shootdown first, when the session
//!   ends. [`region_unmap`] ends one early.
//! * [`read_region`] / [`write_region`] are the ONLY place a driver touches a mapping: map, copy once, unmap - nothing keeps a pointer past
//!   the call, so a session that ended between two calls is a named refusal (`SessionEnded`), never a stale pointer.
//! * [`dma_lease_create`] / [`dma_lease_revoke`]: a device gives ONE session CPU access to whole pages of its own DMA allocation. The device
//!   hands the returned handle to the client (its `region_map` cap); it revokes only after the GPU has retired the work and the GTT entries are
//!   unbound (the kernel does not interpret that), and reuses the pages only after the revoke succeeded.
//!
//! THE GAP (no signal plumbing here): a mapping the kernel removes WHILE a copy is running (the client died or closed its session mid-call)
//! makes the next access fault. The kernel delivers SIGBUS (else SIGSEGV) to a handler the task installed (`SIGACTION`, 49, with a
//! `[handler, restorer, mask, flags]` quad and a restorer that issues `RT_SIGRETURN`, 48); with none installed that task - the device - is
//! reaped. This crate has no `SIGACTION` wrapper, no restorer and no resumable copy, so [`read_region`] / [`write_region`] cannot turn that fault
//! into [`RegionRefusal::Revoked`] today. The exposure is exactly one memcpy per region call, all inside this file; the day rt gains a
//! recoverable copy (a handler that rewrites the saved RIP to the copy's failure label), it replaces `copy_in`/`copy_out` and nothing else changes.

use crate::{nr, sc};

/// REGION_MAP `rights` bits. Execute does not exist: a region is data.
pub const RIGHT_READ: u64 = 1;
pub const RIGHT_WRITE: u64 = 2;
/// Set in REGION_MAP's `cap` for a LEASE handle (the low bits are the lease id, never reused). A shmem slot number is below 64.
pub const LEASE_CAP_TAG: u64 = 1 << 62;
/// The window the kernel places REGION_MAP / lease mappings in: its own PML4 entry, 1 TiB. Addresses are never reissued.
pub const REGION_MAP_VA: u64 = 0x0000_3000_0000_0000;
pub const REGION_MAP_LEN: u64 = 0x0000_0100_0000_0000;
/// `LeaseRequest::version`.
pub const LEASE_REQUEST_V1: u32 = 1;
/// Size of the lease request block, and where the kernel writes the handle.
pub const LEASE_REQUEST_SIZE: u64 = 64;
pub const LEASE_OUT_OFFSET: usize = 56;

/// DMA_LEASE_CREATE's argument block (a0 = pointer, a1 = size). `out_lease` is written by the kernel.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LeaseRequest {
    pub version: u32,
    /// [`RIGHT_READ`] | [`RIGHT_WRITE`] for the RECIPIENT's mapping.
    pub rights: u32,
    /// The CPU memory type every alias of the pages must carry, as DMA_ALLOC_TYPED names it ([`crate::cache`]: WB 0, WC 1, UC 2).
    pub cache: u32,
    pub _reserved: u32,
    /// An address DMA_ALLOC_TYPED gave this device (its `uva`); `offset` is counted from here.
    pub alloc_va: u64,
    /// Page-aligned.
    pub offset: u64,
    /// Page-aligned, > 0.
    pub length: u64,
    /// The session's request endpoint (this device holds RECV on it) ...
    pub recipient_req_ep: u64,
    /// ... and the client's node as the connect announce carried it.
    pub recipient_node: u64,
    /// OUT: the lease handle (`LEASE_CAP_TAG | id`).
    pub out_lease: u64,
}

/// Why REGION_MAP / REGION_UNMAP granted nothing. The family is `u64::MAX-130 ..= u64::MAX-139`; nothing changed on any refusal.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum RegionStatus {
    /// `rights` is empty or carries a bit that is not READ/WRITE.
    BadRights = 1,
    /// Zero length, an overflowing range, or a range outside the capability's region.
    OutOfRegion = 2,
    /// The capability is no region delegated over a session (an empty slot, a non-session delegation, a slot that holds another capability now).
    NoGrant = 3,
    /// The session the capability arrived on has ended.
    SessionEnded = 4,
    /// `rights` asks for more than the delegation carried (WRITE on a read-only delegation).
    RightsExceedGrant = 5,
    /// The address space's REGION_MAP window is exhausted.
    NoVirtualSpace = 6,
    /// A table or page-table frame could not be allocated; nothing was mapped.
    NoMemory = 7,
    /// The backing region was retired.
    RegionGone = 8,
    /// The session ended while the mapping was being installed: it was removed again.
    RevokedDuringMap = 9,
    /// REGION_UNMAP of an address that is not a live mapping made by this address space.
    NotMapped = 10,
}

/// Why DMA_LEASE_CREATE / DMA_LEASE_REVOKE granted nothing. The family is `u64::MAX-140 ..= u64::MAX-150`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u32)]
pub enum LeaseStatus {
    /// The request block is not a user range, is the wrong size, or a version the kernel does not know.
    BadRequest = 1,
    /// The caller holds no DMA window, or `alloc_va` is not in it.
    NotOwner = 2,
    /// Offset or length is not whole pages, the length is zero, or the range overflows.
    NotPageExact = 3,
    /// The range leaves the caller's DMA window or covers a page not mapped there.
    OutOfAllocation = 4,
    /// `cache` names no type, or rights is empty / carries a non-READ/WRITE bit.
    BadCacheOrRights = 5,
    /// The requested type differs from the type some alias of these pages carries.
    CacheAliasConflict = 6,
    /// A non-WB type on a CPU whose IA32_PAT does not hold the WC layout.
    PatNotReady = 7,
    /// The recipient is not a live session of the caller.
    BadRecipient = 8,
    /// Some page of the range is already under a live lease.
    PageAlreadyLeased = 9,
    /// The lease table could not grow; nothing was created.
    NoMemory = 10,
    /// DMA_LEASE_REVOKE of a handle that is not a live lease of the caller's address space.
    NotLease = 11,
}

const REGION_BASE: u64 = u64::MAX - 130;
const LEASE_BASE: u64 = u64::MAX - 140;

impl RegionStatus {
    pub const ALL: [RegionStatus; 10] = [
        RegionStatus::BadRights,
        RegionStatus::OutOfRegion,
        RegionStatus::NoGrant,
        RegionStatus::SessionEnded,
        RegionStatus::RightsExceedGrant,
        RegionStatus::NoVirtualSpace,
        RegionStatus::NoMemory,
        RegionStatus::RegionGone,
        RegionStatus::RevokedDuringMap,
        RegionStatus::NotMapped,
    ];
    pub const fn code(self) -> u64 {
        REGION_BASE - (self as u64 - 1)
    }
    pub fn from_code(w: u64) -> Option<RegionStatus> {
        Self::ALL.into_iter().find(|s| s.code() == w)
    }
    pub const fn name(self) -> &'static str {
        match self {
            RegionStatus::BadRights => "REGION-BAD-RIGHTS",
            RegionStatus::OutOfRegion => "REGION-OUT-OF-REGION",
            RegionStatus::NoGrant => "REGION-NO-GRANT",
            RegionStatus::SessionEnded => "REGION-SESSION-ENDED",
            RegionStatus::RightsExceedGrant => "REGION-RIGHTS-EXCEED-GRANT",
            RegionStatus::NoVirtualSpace => "REGION-NO-VIRTUAL-SPACE",
            RegionStatus::NoMemory => "REGION-NO-MEMORY",
            RegionStatus::RegionGone => "REGION-GONE",
            RegionStatus::RevokedDuringMap => "REGION-REVOKED-DURING-MAP",
            RegionStatus::NotMapped => "REGION-NOT-MAPPED",
        }
    }
}

impl LeaseStatus {
    pub const ALL: [LeaseStatus; 11] = [
        LeaseStatus::BadRequest,
        LeaseStatus::NotOwner,
        LeaseStatus::NotPageExact,
        LeaseStatus::OutOfAllocation,
        LeaseStatus::BadCacheOrRights,
        LeaseStatus::CacheAliasConflict,
        LeaseStatus::PatNotReady,
        LeaseStatus::BadRecipient,
        LeaseStatus::PageAlreadyLeased,
        LeaseStatus::NoMemory,
        LeaseStatus::NotLease,
    ];
    pub const fn code(self) -> u64 {
        LEASE_BASE - (self as u64 - 1)
    }
    pub fn from_code(w: u64) -> Option<LeaseStatus> {
        Self::ALL.into_iter().find(|s| s.code() == w)
    }
    pub const fn name(self) -> &'static str {
        match self {
            LeaseStatus::BadRequest => "LEASE-BAD-REQUEST",
            LeaseStatus::NotOwner => "LEASE-NOT-OWNER",
            LeaseStatus::NotPageExact => "LEASE-NOT-PAGE-EXACT",
            LeaseStatus::OutOfAllocation => "LEASE-OUT-OF-ALLOCATION",
            LeaseStatus::BadCacheOrRights => "LEASE-BAD-CACHE-OR-RIGHTS",
            LeaseStatus::CacheAliasConflict => "LEASE-CACHE-ALIAS-CONFLICT",
            LeaseStatus::PatNotReady => "LEASE-PAT-NOT-READY",
            LeaseStatus::BadRecipient => "LEASE-BAD-RECIPIENT",
            LeaseStatus::PageAlreadyLeased => "LEASE-PAGE-ALREADY-LEASED",
            LeaseStatus::NoMemory => "LEASE-NO-MEMORY",
            LeaseStatus::NotLease => "LEASE-NOT-LEASE",
        }
    }
}

/// What a region call reports instead of a result: a named status of the family, or a word this build cannot name (reported as itself,
/// never guessed at - SYSCALL-RETURN-VALIDATED).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RegionRefusal {
    Named(RegionStatus),
    Unknown(u64),
}

impl RegionRefusal {
    pub const fn name(&self) -> &'static str {
        match self {
            RegionRefusal::Named(s) => s.name(),
            RegionRefusal::Unknown(_) => "REGION-UNKNOWN-WORD",
        }
    }
}

/// The same for the lease calls.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LeaseRefusal {
    Named(LeaseStatus),
    Unknown(u64),
}

impl LeaseRefusal {
    pub const fn name(&self) -> &'static str {
        match self {
            LeaseRefusal::Named(s) => s.name(),
            LeaseRefusal::Unknown(_) => "LEASE-UNKNOWN-WORD",
        }
    }
}

/// What REGION_MAP's word means: the VA, or the refusal. A word that is neither a VA inside the window nor a status of the family is
/// [`RegionRefusal::Unknown`] (the failure word `u64::MAX` included).
pub fn decode_map(w: u64) -> Result<u64, RegionRefusal> {
    match RegionStatus::from_code(w) {
        Some(s) => Err(RegionRefusal::Named(s)),
        None if (REGION_MAP_VA..REGION_MAP_VA + REGION_MAP_LEN + 4096).contains(&w) => Ok(w),
        None => Err(RegionRefusal::Unknown(w)),
    }
}

/// What a one-success-word region call (REGION_UNMAP) means: 0, or the refusal.
pub fn decode_region(w: u64) -> Result<(), RegionRefusal> {
    match (w, RegionStatus::from_code(w)) {
        (0, _) => Ok(()),
        (_, Some(s)) => Err(RegionRefusal::Named(s)),
        _ => Err(RegionRefusal::Unknown(w)),
    }
}

/// What a DMA_LEASE_* word means: 0, or the refusal.
pub fn decode_lease(w: u64) -> Result<(), LeaseRefusal> {
    match (w, LeaseStatus::from_code(w)) {
        (0, _) => Ok(()),
        (_, Some(s)) => Err(LeaseRefusal::Named(s)),
        _ => Err(LeaseRefusal::Unknown(w)),
    }
}

/// Map `[offset, offset+length)` of the region capability `cap` (a delegated slot, or a lease handle) with `rights`. Returns the VA of the
/// first requested byte. The caller owns the mapping until [`region_unmap`] or the end of the session; it must not touch it afterwards.
pub fn region_map(cap: u64, offset: u64, length: u64, rights: u64) -> Result<*mut u8, RegionRefusal> {
    decode_map(unsafe { sc(nr::REGION_MAP, cap, offset, length, rights) }).map(|va| va as *mut u8)
}

/// End one mapping (any address inside it).
pub fn region_unmap(uva: *mut u8) -> Result<(), RegionRefusal> {
    decode_region(unsafe { sc(nr::REGION_UNMAP, uva as u64, 0, 0, 0) })
}

/// Build the block DMA_LEASE_CREATE takes.
pub fn lease_request(alloc_va: u64, offset: u64, length: u64, rights: u32, cache: u32, recipient_req_ep: u16, recipient_node: u64) -> LeaseRequest {
    LeaseRequest { version: LEASE_REQUEST_V1, rights, cache, _reserved: 0, alloc_va, offset, length, recipient_req_ep: recipient_req_ep as u64, recipient_node, out_lease: 0 }
}

/// Mint the lease. Returns the handle to hand the recipient (its [`region_map`] `cap`).
pub fn dma_lease_create(req: &mut LeaseRequest) -> Result<u64, LeaseRefusal> {
    // the kernel writes req.out_lease; `sc` carries no `nomem`, so the compiler cannot cache the struct across it
    decode_lease(unsafe { sc(nr::DMA_LEASE_CREATE, req as *mut LeaseRequest as u64, LEASE_REQUEST_SIZE, 0, 0) })?;
    Ok(unsafe { core::ptr::read_volatile(&req.out_lease) })
}

/// Close a lease: the recipient's mappings go (the kernel completes the TLB shootdown first); the backing stays the device's. `Ok` is the
/// kernel's statement that the lease ended - the only thing that lets the pages be reused.
pub fn dma_lease_revoke(handle: u64) -> Result<(), LeaseRefusal> {
    decode_lease(unsafe { sc(nr::DMA_LEASE_REVOKE, handle, 0, 0, 0) })
}

/// Copy `src` out into `out` ONCE. Volatile: the source is memory another task may change under us, so each byte is read exactly one time and
/// nothing is re-read later (the snapshot IS this copy).
///
/// # Safety
/// `src..src+out.len()` must be readable for the duration of the call.
pub unsafe fn copy_in(src: *const u8, out: &mut [u8]) {
    let mut i = 0;
    let n = out.len();
    // bytes up to the 8-byte boundary of `src`, then aligned 64-bit volatile reads, then the tail
    while i < n && (src as usize + i) % 8 != 0 {
        out[i] = core::ptr::read_volatile(src.add(i));
        i += 1;
    }
    while i + 8 <= n {
        let w = core::ptr::read_volatile(src.add(i) as *const u64);
        out[i..i + 8].copy_from_slice(&w.to_ne_bytes());
        i += 8;
    }
    while i < n {
        out[i] = core::ptr::read_volatile(src.add(i));
        i += 1;
    }
}

/// Copy `data` to `dst`.
///
/// # Safety
/// `dst..dst+data.len()` must be writable for the duration of the call.
pub unsafe fn copy_out(dst: *mut u8, data: &[u8]) {
    core::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
}

/// Snapshot `out.len()` bytes at `offset` of the delegated region `cap` into `out`: map it READ-ONLY, copy once, unmap. Nothing is kept.
pub fn read_region(cap: u64, offset: u64, out: &mut [u8]) -> Result<(), RegionRefusal> {
    if out.is_empty() {
        return Ok(());
    }
    let p = region_map(cap, offset, out.len() as u64, RIGHT_READ)?;
    // see THE GAP above: the copy is the only unrecoverable moment
    unsafe { copy_in(p, out) };
    unmap_after(p)
}

/// Write `data` at `offset` of the delegated region `cap` (a reply area): map it READ+WRITE, copy, unmap. A read-only delegation is
/// `RightsExceedGrant`.
pub fn write_region(cap: u64, offset: u64, data: &[u8]) -> Result<(), RegionRefusal> {
    if data.is_empty() {
        return Ok(());
    }
    let p = region_map(cap, offset, data.len() as u64, RIGHT_WRITE)?;
    unsafe { copy_out(p, data) };
    unmap_after(p)
}

/// The mapping is ours to end; if the session ended meanwhile the kernel already removed it (`NotMapped`/`SessionEnded` are then the
/// expected answer, not a failure of the copy).
fn unmap_after(p: *mut u8) -> Result<(), RegionRefusal> {
    match region_unmap(p) {
        Ok(()) | Err(RegionRefusal::Named(RegionStatus::NotMapped)) | Err(RegionRefusal::Named(RegionStatus::SessionEnded)) => Ok(()),
        Err(e) => Err(e),
    }
}
