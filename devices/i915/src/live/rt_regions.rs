// SPDX-License-Identifier: GPL-2.0-only
//! The production [`RegionSys`]: `coralos_rt::region` (REGION_MAP, copy once, REGION_UNMAP).
//!
//! THE GAP (rt has no `SIGACTION` wrapper): a mapping the kernel removes WHILE [`read`](RegionSys::read) is copying (the client died mid-call)
//! faults the device. rt's `read_region`/`write_region` are the only place a pointer into a client's memory exists; when rt gains a recoverable
//! copy, [`SysRefusal::Revoked`] is what it reports and nothing here changes. Between calls nothing is mapped, so a client that is already gone
//! is a clean `Revoked` from the kernel.
use super::session_regions::{RegionSys, SysRefusal};
use coralos_rt::region::{read_region, write_region, RegionRefusal, RegionStatus};

/// The kernel's refusal, in the device's terms.
pub fn classify(r: RegionRefusal) -> SysRefusal {
    match r {
        RegionRefusal::Named(RegionStatus::SessionEnded | RegionStatus::RevokedDuringMap | RegionStatus::RegionGone) => SysRefusal::Revoked,
        RegionRefusal::Named(RegionStatus::OutOfRegion) => SysRefusal::OutOfRegion,
        RegionRefusal::Named(RegionStatus::NoGrant) => SysRefusal::NoGrant,
        RegionRefusal::Named(RegionStatus::RightsExceedGrant) => SysRefusal::ReadOnly,
        RegionRefusal::Named(RegionStatus::NoMemory | RegionStatus::NoVirtualSpace) => SysRefusal::NoMemory,
        RegionRefusal::Named(s) => SysRefusal::Other { name: s.name(), word: s.code() },
        RegionRefusal::Unknown(w) => SysRefusal::Other { name: "REGION-UNKNOWN-WORD", word: w },
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RtRegionSys;

impl RegionSys for RtRegionSys {
    fn read(&mut self, cap: u64, offset: u64, out: &mut [u8]) -> Result<(), SysRefusal> {
        read_region(cap, offset, out).map_err(classify)
    }
    fn write(&mut self, cap: u64, offset: u64, data: &[u8]) -> Result<(), SysRefusal> {
        write_region(cap, offset, data).map_err(classify)
    }
}
