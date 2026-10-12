// SPDX-License-Identifier: GPL-2.0-only
//! Named refusals of the modeset planners. Every variant has a stable `name()`; wrapped errors report the wrapped crate's name so
//! one grep finds a refusal wherever it was raised.
use i915_display_clock_core::ClockError;
use i915_kbl_display_core::mode_write::ModeError;
use i915_kbl_display_core::skl_pfit::PfitError;
use i915_kbl_display_core::skl_plane::PlaneError;
use i915_kbl_display_core::skl_wm::WmError;
use i915_kbl_display_core::types::Pipe;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModesetError {
    Plane(PlaneError),
    Wm(WmError),
    Mode(ModeError),
    Pfit(PfitError),
    Clock(ClockError),
    /// Adoption: the firmware plane is not the one write allow-list entry (PLANE_SURF of PLANE_1 on pipe A).
    PlaneNotInAllowList { pipe: Pipe, plane_id: u8 },
    /// Adoption: the firmware left the plane disabled (nothing to replace).
    PlaneDisabled,
    /// Adoption: the replacement is not the same layout; `field` names the first difference (format, tiling, stride, size,
    /// offset, rotation). A layout change is `P7`'s (native modeset) business, never the adoption's.
    LayoutDiffers { field: &'static str },
    /// Adoption: the new surface address equals the one scanned out (a no-op write is refused rather than issued).
    SameSurface,
    /// Adoption: the new surface address is not aligned as `skl_plane_min_alignment` requires, or not a 32-bit GGTT address.
    SurfMisaligned { surf: u32, need: u32 },
    /// Adoption: the old buffer was asked to be released before `PLANE_SURFLIVE` showed the new address.
    OldBufferStillLive,
    /// A bounded wait ran out (adoption latch, pipe enable, DBUF power).
    WaitTimedOut { what: &'static str, polls: u32 },
    /// The stage index handed to a rollback request is outside the transaction.
    NoSuchStage { index: usize },
    /// The memory-latency mailbox exchange failed; latencies are never guessed (Linux would carry on with zeros plus 2 us).
    LatencyRead(ClockError),
    /// The firmware scanout readout and the inputs disagree (e.g. the readout pipe is not the pipe being adopted).
    ReadoutMismatch { what: &'static str },
}

impl ModesetError {
    pub const fn name(&self) -> &'static str {
        match self {
            ModesetError::Plane(e) => e.name(),
            ModesetError::Wm(e) => e.name(),
            ModesetError::Mode(e) => e.name(),
            ModesetError::Pfit(e) => e.name(),
            ModesetError::Clock(e) => e.name(),
            ModesetError::PlaneNotInAllowList { .. } => "ADOPT-PLANE-NOT-IN-ALLOW-LIST",
            ModesetError::PlaneDisabled => "ADOPT-PLANE-DISABLED",
            ModesetError::LayoutDiffers { .. } => "ADOPT-LAYOUT-DIFFERS",
            ModesetError::SameSurface => "ADOPT-SAME-SURFACE",
            ModesetError::SurfMisaligned { .. } => "ADOPT-SURF-MISALIGNED",
            ModesetError::OldBufferStillLive => "ADOPT-OLD-BUFFER-STILL-LIVE",
            ModesetError::WaitTimedOut { .. } => "MODESET-WAIT-TIMED-OUT",
            ModesetError::NoSuchStage { .. } => "MODESET-NO-SUCH-STAGE",
            ModesetError::LatencyRead(_) => "MODESET-LATENCY-READ-FAILED",
            ModesetError::ReadoutMismatch { .. } => "MODESET-READOUT-MISMATCH",
        }
    }
}

impl From<PlaneError> for ModesetError {
    fn from(e: PlaneError) -> Self {
        ModesetError::Plane(e)
    }
}
impl From<WmError> for ModesetError {
    fn from(e: WmError) -> Self {
        ModesetError::Wm(e)
    }
}
impl From<ModeError> for ModesetError {
    fn from(e: ModeError) -> Self {
        ModesetError::Mode(e)
    }
}
impl From<PfitError> for ModesetError {
    fn from(e: PfitError) -> Self {
        ModesetError::Pfit(e)
    }
}
impl From<ClockError> for ModesetError {
    fn from(e: ClockError) -> Self {
        ModesetError::Clock(e)
    }
}
