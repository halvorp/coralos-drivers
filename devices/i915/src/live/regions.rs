// SPDX-License-Identifier: GPL-2.0-only
//! Payload regions (`i915_wire::region`, ABI 1.2): the host's side of "the call's bytes are in a region the client granted".
//!
//! The device never maps a client's memory itself: `PayloadRegions` is the host's seam, resolving an opaque region id to bytes. Today's
//! production binding is [`NoRegions`] - the kernel has no call that maps a foreign grant into a ring-3 device (coralos-rt lists none), so
//! a region frame is refused by name, `PAYLOAD-REGION-UNAVAILABLE`, and the client keeps its calls within one inline frame (64 KiB). The
//! wire, the device's handling and the tests are complete; the day the kernel grows the call, the binding is the only change.

/// Why a region access failed. Each has a stable name for the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionError {
    /// The host has no way to reach client regions.
    Unavailable,
    /// The host does not know this region id (never granted, or revoked).
    Unknown { region: u32 },
    /// The range is not inside the region.
    OutOfRange { region: u32, offset: u64, len: u64, size: u64 },
}

impl RegionError {
    pub const fn name(&self) -> &'static str {
        match self {
            RegionError::Unavailable => "PAYLOAD-REGION-UNAVAILABLE",
            RegionError::Unknown { .. } => "PAYLOAD-REGION-UNKNOWN",
            RegionError::OutOfRange { .. } => "PAYLOAD-REGION-OUT-OF-RANGE",
        }
    }
}

pub trait PayloadRegions {
    /// Copy `out.len()` bytes from `region` at `offset` into `out` (the one copy that makes the request the device's own).
    fn read(&mut self, region: u32, offset: u64, out: &mut [u8]) -> Result<(), RegionError>;
    /// Copy `data` into `region` at `offset` (a reply too large for a frame).
    fn write(&mut self, region: u32, offset: u64, data: &[u8]) -> Result<(), RegionError>;
}

/// No region can be reached (the production binding until the kernel grows a foreign-grant map).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRegions;

impl PayloadRegions for NoRegions {
    fn read(&mut self, _: u32, _: u64, _: &mut [u8]) -> Result<(), RegionError> {
        Err(RegionError::Unavailable)
    }
    fn write(&mut self, _: u32, _: u64, _: &[u8]) -> Result<(), RegionError> {
        Err(RegionError::Unavailable)
    }
}
