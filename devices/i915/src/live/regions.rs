// SPDX-License-Identifier: GPL-2.0-only
//! Payload regions (`i915_wire::region`, ABI 1.2/1.3): the host's side of "the call's bytes are in a region the client granted".
//!
//! The device never maps a client's memory itself: [`PayloadRegions`] is the host's seam, resolving a region id (the id of the SESSION
//! CAPABILITY the client's CAP_DELEGATE returned, `DELEG_CAP_TAG | id`) to bytes, for the session whose request endpoint the call ARRIVED on
//! (never a value from the request's bytes). The production binding is [`super::session_regions::SessionRegions`] over the
//! kernel's REGION_MAP (rt). [`NoRegions`] is the binding for a host that has none: every region frame is refused by name,
//! `PAYLOAD-REGION-UNAVAILABLE`, and the client keeps its calls within one fragmented frame (60 KiB).
/// Why a region access failed. Each has a stable name for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionError {
    /// The host has no way to reach client regions.
    Unavailable,
    /// The host does not know this region id (never granted, or not a region delegated over a session).
    Unknown { region: u32 },
    /// The range is not inside the region. `size` is 0 when the host cannot say (the kernel bounds the range, not the device).
    OutOfRange { region: u32, offset: u64, len: u64, size: u64 },
    /// The session the region was delegated over has ended (the client closed it or died): nothing of the region can be reached any more.
    Revoked { region: u32 },
    /// The capability did not arrive on the session this call came in on (a client naming another client's capability): the kernel refused it.
    WrongSession { region: u32 },
    /// A reply area in a region that was delegated read-only.
    ReadOnly { region: u32 },
    /// A table (or the kernel's mapping tables) could not grow; nothing was done.
    NoMemory { what: &'static str },
    /// The kernel refused with a word this device has no finer name for.
    Refused { name: &'static str, word: u64 },
}

impl RegionError {
    pub const fn name(&self) -> &'static str {
        match self {
            RegionError::Unavailable => "PAYLOAD-REGION-UNAVAILABLE",
            RegionError::Unknown { .. } => "PAYLOAD-REGION-UNKNOWN",
            RegionError::OutOfRange { .. } => "PAYLOAD-REGION-OUT-OF-RANGE",
            RegionError::Revoked { .. } => "PAYLOAD-REGION-REVOKED",
            RegionError::WrongSession { .. } => "PAYLOAD-REGION-WRONG-SESSION",
            RegionError::ReadOnly { .. } => "PAYLOAD-REGION-READ-ONLY",
            RegionError::NoMemory { .. } => "PAYLOAD-REGION-NOMEM",
            RegionError::Refused { name, .. } => name,
        }
    }
}

pub trait PayloadRegions {
    /// `session_ep` is the request endpoint the call arrived on. Copy `out.len()` bytes from `region` at `offset` into `out` (the ONE copy that makes the request the device's own: the caller validates
    /// `out`, never the client's memory).
    fn read(&mut self, session_ep: u16, region: u32, offset: u64, out: &mut [u8]) -> Result<(), RegionError>;
    /// Copy `data` into `region` at `offset` (a reply too large for a frame).
    fn write(&mut self, session_ep: u16, region: u32, offset: u64, data: &[u8]) -> Result<(), RegionError>;
}

/// No region can be reached.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRegions;

impl PayloadRegions for NoRegions {
    fn read(&mut self, _: u16, _: u32, _: u64, _: &mut [u8]) -> Result<(), RegionError> {
        Err(RegionError::Unavailable)
    }
    fn write(&mut self, _: u16, _: u32, _: u64, _: &[u8]) -> Result<(), RegionError> {
        Err(RegionError::Unavailable)
    }
}
