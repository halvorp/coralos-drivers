// SPDX-License-Identifier: GPL-2.0-only
//! [`SessionRegions`]: the production [`PayloadRegions`] over the kernel's session-scoped REGION_MAP (`coralos_rt::region`).
//!
//! THE CONTRACT with a client (the CoralOS shim): over its session endpoint it delegates (CAP_DELEGATE) its REQUEST storage READ-ONLY and its
//! REPLY storage READ-WRITE, and names each by the SLOT the delegation returned (`RegionRef::region` for the request, `reply_region` for the
//! reply). This binding:
//!   * SNAPSHOTS: `read` maps the range read-only, copies it ONCE into the caller's device-private buffer and unmaps; the caller validates
//!     that buffer, never the client's memory (a read-only delegation does not stop the client changing its own pages while we read them).
//!   * keeps NO mapping and NO pointer between calls: a session that ended since the last call is a named refusal from the kernel
//!     (`PAYLOAD-REGION-REVOKED`), not a stale pointer. Nothing survives "session end" because nothing is kept; [`conn_ended`] only drops the
//!     bookkeeping below.
//!   * writes a reply with a WRITE mapping: a read-only delegation is `PAYLOAD-REGION-READ-ONLY`.
//!   * refuses a slot that another CONNECTION is using (`PAYLOAD-REGION-NOT-YOURS`): the kernel's REGION_MAP takes only the slot, so
//!     without this a client could name another client's slot and have the device read or overwrite it. KNOWN LIMIT: the first connection to use
//!     a slot owns it, and a client that guesses another's slot BEFORE its first call can take it; the exact fix is the kernel binding REGION_MAP to
//!     the caller-named session (the grant already records the session's request endpoint) - see the report for this change.
//! The table of claims is a growable `Vec` (RUNTIME-TABLES-HAVE-NO-CAP): a claim is reserved BEFORE the copy; no room is `PAYLOAD-REGION-NOMEM`.
use super::regions::{PayloadRegions, RegionError};
use crate::conn::ConnId;
use alloc::vec::Vec;

/// Why the kernel-side copy failed, in the device's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysRefusal {
    /// The session ended, or the region was retired: nothing is reachable.
    Revoked,
    OutOfRegion,
    /// The slot holds no region delegated over a session.
    NoGrant,
    /// WRITE on a read-only delegation.
    ReadOnly,
    NoMemory,
    Other { name: &'static str, word: u64 },
}

/// One map-copy-unmap against the kernel.
pub trait RegionSys {
    fn read(&mut self, cap: u64, offset: u64, out: &mut [u8]) -> Result<(), SysRefusal>;
    fn write(&mut self, cap: u64, offset: u64, data: &[u8]) -> Result<(), SysRefusal>;
}

pub struct SessionRegions<S: RegionSys> {
    pub sys: S,
    /// `(slot, connection)`: the connection that first used the slot.
    claims: Vec<(u32, ConnId)>,
}

impl<S: RegionSys> SessionRegions<S> {
    pub fn new(sys: S) -> Self {
        SessionRegions { sys, claims: Vec::new() }
    }

    /// Slots currently claimed (the tests' view).
    pub fn claimed(&self) -> usize {
        self.claims.len()
    }

    /// May `conn` use `slot`? `Ok(true)` = a new claim (room was reserved), `Ok(false)` = already its own.
    fn check(&mut self, conn: ConnId, slot: u32) -> Result<bool, RegionError> {
        match self.claims.iter().find(|c| c.0 == slot) {
            Some(c) if c.1 == conn => Ok(false),
            Some(_) => Err(RegionError::NotYours { region: slot }),
            None => {
                self.claims.try_reserve(1).map_err(|_| RegionError::NoMemory { what: "region-claims" })?;
                Ok(true)
            }
        }
    }

    fn settle(&mut self, conn: ConnId, slot: u32, fresh: bool, r: Result<(), SysRefusal>, offset: u64, len: u64) -> Result<(), RegionError> {
        match r {
            Ok(()) => {
                if fresh {
                    self.claims.push((slot, conn)); // room reserved by `check`
                }
                Ok(())
            }
            Err(SysRefusal::Revoked) => Err(RegionError::Revoked { region: slot }),
            Err(SysRefusal::OutOfRegion) => Err(RegionError::OutOfRange { region: slot, offset, len, size: 0 }),
            Err(SysRefusal::NoGrant) => Err(RegionError::Unknown { region: slot }),
            Err(SysRefusal::ReadOnly) => Err(RegionError::ReadOnly { region: slot }),
            Err(SysRefusal::NoMemory) => Err(RegionError::NoMemory { what: "region-map" }),
            Err(SysRefusal::Other { name, word }) => Err(RegionError::Refused { name, word }),
        }
    }
}

impl<S: RegionSys> PayloadRegions for SessionRegions<S> {
    fn read(&mut self, conn: ConnId, region: u32, offset: u64, out: &mut [u8]) -> Result<(), RegionError> {
        let fresh = self.check(conn, region)?;
        let r = self.sys.read(region as u64, offset, out);
        self.settle(conn, region, fresh, r, offset, out.len() as u64)
    }
    fn write(&mut self, conn: ConnId, region: u32, offset: u64, data: &[u8]) -> Result<(), RegionError> {
        let fresh = self.check(conn, region)?;
        let r = self.sys.write(region as u64, offset, data);
        self.settle(conn, region, fresh, r, offset, data.len() as u64)
    }
    fn conn_ended(&mut self, conn: ConnId) {
        self.claims.retain(|c| c.1 != conn);
    }
}
