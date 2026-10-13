// SPDX-License-Identifier: GPL-2.0-only
//! The kernel's DMA lease as the device uses it (DMA_LEASE_CREATE / DMA_LEASE_REVOKE, `coralos_rt::region`): a revocable, page-exact CPU
//! mapping capability over pages of the device's OWN DMA allocation, minted for ONE client session.
//!
//! WHO DECIDES WHAT. The kernel checks the mechanism (whole pages, the caller's own window, one cache type across every alias, a live
//! recipient) and removes the recipient's mappings, TLB shootdown first, on revoke or when the recipient's session ends. The DEVICE decides
//! WHEN: it revokes only after the GPU retired every request that touched the pages and their GTT entries are unbound, and it never gives the
//! pages back to the pool before the kernel said the lease ended ([`Revoked`]). A revoke the kernel will not confirm QUARANTINES the pages.
use crate::conn::Peer;
use coralos_hal::Coherence;

/// What a lease is asked for. The recipient mapping is read+write (a GEM mmap has no narrower right to give).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeaseSpec {
    /// An address the kernel gave the device (`DMA_ALLOC`'s `uva`): `offset` is counted from here.
    pub alloc_va: u64,
    /// Page-aligned.
    pub offset: u64,
    /// Page-aligned, non-zero.
    pub length: u64,
    /// The cache type of the pages (every alias must agree).
    pub coherence: Coherence,
    /// The session the lease is for.
    pub peer: Peer,
}

/// Why the kernel granted nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseFail {
    /// This device has no way to mint leases.
    Unavailable,
    /// The kernel's lease table could not grow.
    NoMemory,
    /// A named refusal of the kernel's.
    Refused { name: &'static str, word: u64 },
}

/// What a successful revoke means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Revoked {
    /// The kernel removed the recipient's mappings and ended the lease.
    Ended,
    /// The kernel no longer knows the lease (it ended with its recipient's session): nothing is mapped.
    AlreadyGone,
}

pub trait LeaseSys {
    /// Mint a lease; returns the handle the recipient redeems with REGION_MAP.
    fn create(&mut self, spec: &LeaseSpec) -> Result<u64, LeaseFail>;
    /// End a lease. `Ok` is the kernel's statement that no CPU mapping of the pages remains.
    fn revoke(&mut self, handle: u64) -> Result<Revoked, LeaseFail>;
}

/// No leases: a host without the kernel mechanism. A GEM mmap is then refused by name (`MMAP-LEASE-UNAVAILABLE`).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoLeaseSys;

impl LeaseSys for NoLeaseSys {
    fn create(&mut self, _: &LeaseSpec) -> Result<u64, LeaseFail> {
        Err(LeaseFail::Unavailable)
    }
    fn revoke(&mut self, _: u64) -> Result<Revoked, LeaseFail> {
        Err(LeaseFail::Unavailable)
    }
}
