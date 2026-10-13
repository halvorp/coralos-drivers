// SPDX-License-Identifier: GPL-2.0-only
//! The production [`LeaseSys`]: `coralos_rt::region` (DMA_LEASE_CREATE / DMA_LEASE_REVOKE).
use super::kernel_lease::{LeaseFail, LeaseSpec, LeaseSys, Revoked};
use coralos_rt::region::{dma_lease_create, dma_lease_revoke, lease_request, LeaseRefusal, LeaseStatus, RIGHT_READ, RIGHT_WRITE};

/// The kernel's refusal, in the device's terms.
pub fn classify(r: LeaseRefusal) -> LeaseFail {
    match r {
        LeaseRefusal::Named(LeaseStatus::NoMemory) => LeaseFail::NoMemory,
        LeaseRefusal::Named(s) => LeaseFail::Refused { name: s.name(), word: s.code() },
        LeaseRefusal::Unknown(w) => LeaseFail::Refused { name: "LEASE-UNKNOWN-WORD", word: w },
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RtLeaseSys;

impl LeaseSys for RtLeaseSys {
    fn create(&mut self, s: &LeaseSpec) -> Result<u64, LeaseFail> {
        let cache = coralos_rt::cache_for(s.coherence) as u32;
        let mut req = lease_request(s.alloc_va, s.offset, s.length, (RIGHT_READ | RIGHT_WRITE) as u32, cache, s.peer.req_ep, s.peer.node);
        dma_lease_create(&mut req).map_err(classify)
    }
    fn revoke(&mut self, handle: u64) -> Result<Revoked, LeaseFail> {
        match dma_lease_revoke(handle) {
            Ok(()) => Ok(Revoked::Ended),
            Err(LeaseRefusal::Named(LeaseStatus::NotLease)) => Ok(Revoked::AlreadyGone),
            Err(e) => Err(classify(e)),
        }
    }
}
