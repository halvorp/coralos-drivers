// SPDX-License-Identifier: GPL-2.0-only
//! [`SessionRegions`]: the production [`PayloadRegions`] over the kernel's session-scoped REGION_MAP (`coralos_rt::region`).
//!
//! THE CONTRACT with a client (the CoralOS shim): over its session endpoint it delegates (CAP_DELEGATE with `DELEGATE_SESSION_CAP`) its REQUEST
//! storage READ-ONLY and its REPLY storage READ-WRITE; each delegation returns a session capability `DELEG_CAP_TAG | id` (a growable kernel
//! ledger, never reused, no slot consumed). The client sends the `id` (`RegionRef::region` for the request, `reply_region` for the reply). This binding:
//!   * names the SESSION on every map: the request endpoint the call ARRIVED on (the transport's, never a value from the request bytes). A
//!     client that names another client's capability gets the kernel's `REGION-WRONG-SESSION` (`PAYLOAD-REGION-WRONG-SESSION`): the confused
//!     deputy is closed in the kernel, so this binding keeps NO ownership table of its own.
//!   * SNAPSHOTS: `read` maps the range read-only, copies it ONCE into the caller's device-private buffer and unmaps; the caller validates
//!     that buffer, never the client's memory (a read-only delegation does not stop the client changing its own pages while we read them).
//!   * keeps NO mapping and NO pointer between calls: a session that ended since the last call is a named kernel refusal
//!     (`PAYLOAD-REGION-REVOKED`), not a stale pointer. The capabilities themselves die with their session in the kernel.
//!   * writes a reply with a WRITE mapping: a read-only delegation is `PAYLOAD-REGION-READ-ONLY`.
use super::regions::{PayloadRegions, RegionError};
use coralos_rt::region::DELEG_CAP_TAG;

/// Why the kernel-side copy failed, in the device's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysRefusal {
    /// The session ended, or the region was retired: nothing is reachable.
    Revoked,
    OutOfRegion,
    /// The capability is no region delegated over a session.
    NoGrant,
    /// The capability arrived on a different session than the one named.
    WrongSession,
    /// WRITE on a read-only delegation.
    ReadOnly,
    NoMemory,
    Other { name: &'static str, word: u64 },
}

/// One map-copy-unmap against the kernel, for the capability `cap` as it arrived on session `session_ep`.
pub trait RegionSys {
    fn read(&mut self, cap: u64, session_ep: u16, offset: u64, out: &mut [u8]) -> Result<(), SysRefusal>;
    fn write(&mut self, cap: u64, session_ep: u16, offset: u64, data: &[u8]) -> Result<(), SysRefusal>;
}

pub struct SessionRegions<S: RegionSys> {
    pub sys: S,
}

impl<S: RegionSys> SessionRegions<S> {
    pub fn new(sys: S) -> Self {
        SessionRegions { sys }
    }
}

fn settle(r: Result<(), SysRefusal>, region: u32, offset: u64, len: u64) -> Result<(), RegionError> {
    match r {
        Ok(()) => Ok(()),
        Err(SysRefusal::Revoked) => Err(RegionError::Revoked { region }),
        Err(SysRefusal::OutOfRegion) => Err(RegionError::OutOfRange { region, offset, len, size: 0 }),
        Err(SysRefusal::NoGrant) => Err(RegionError::Unknown { region }),
        Err(SysRefusal::WrongSession) => Err(RegionError::WrongSession { region }),
        Err(SysRefusal::ReadOnly) => Err(RegionError::ReadOnly { region }),
        Err(SysRefusal::NoMemory) => Err(RegionError::NoMemory { what: "region-map" }),
        Err(SysRefusal::Other { name, word }) => Err(RegionError::Refused { name, word }),
    }
}

impl<S: RegionSys> PayloadRegions for SessionRegions<S> {
    fn read(&mut self, session_ep: u16, region: u32, offset: u64, out: &mut [u8]) -> Result<(), RegionError> {
        let r = self.sys.read(DELEG_CAP_TAG | region as u64, session_ep, offset, out);
        settle(r, region, offset, out.len() as u64)
    }
    fn write(&mut self, session_ep: u16, region: u32, offset: u64, data: &[u8]) -> Result<(), RegionError> {
        let r = self.sys.write(DELEG_CAP_TAG | region as u64, session_ep, offset, data);
        settle(r, region, offset, data.len() as u64)
    }
}
