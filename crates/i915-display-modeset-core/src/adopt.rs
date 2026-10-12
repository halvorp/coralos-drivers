// SPDX-License-Identifier: GPL-2.0-only
//! P4 - display ADOPTION: take over the firmware's (GOP's) scanout and replace the surface it is scanning out with another one of the
//! SAME layout, changing nothing but the address. The whole write set is ONE register: [`ADOPT_WRITE_ALLOWLIST`].
//!
//! Nothing here programs a DPLL, CDCLK, DDI, PPS, AUX or watermark: the firmware's pipe, transcoder, link and DDB stay exactly as the
//! readout found them. The only write is `PLANE_SURF` of PLANE_1 on pipe A, which on a plane that is already enabled arms the update
//! for the next vblank (skl_universal_plane.c `skl_plane_update_arm`: "The control register self-arms if the plane was previously
//! disabled", and the surface register is the latch otherwise). `PLANE_SURFLIVE` then shows which address the hardware is really
//! scanning out; the old buffer may be released only after it shows the new one ([`AdoptSession::release_old_buffer`]).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/skl_universal_plane.c (`skl_plane_update_arm`'s surf write, `PLANE_SURFLIVE`
//! as read by `skl_plane_capture_error`, `skl_plane_min_alignment`, `skl_get_initial_plane_config`'s fb description).
//! Copyright (c) Intel Corporation.
use crate::error::ModesetError;
use crate::plan::Plan;
use crate::plane_plan::latch_poll;
use alloc::vec;
use i915_common_core::Reg;
use i915_display_link_core::plan::{written_regs, Effect, PollPlan, PollStep, PollTimeout};
use i915_kbl_display_core::plane::{PlaneFormat, PlaneState, Rotation, Tiling};
use i915_kbl_display_core::readout::{PipeState, ReadoutState};
use i915_kbl_display_core::regs::*;
use i915_kbl_display_core::types::Pipe;

/// THE adoption write allow-list: `PLANE_SURF(PIPE_A, PLANE_1)` and nothing else. `devices/i915` adopts this constant for its
/// `WriteGuard` when the owner-attended adoption increment is approved; this crate never widens it and the device's own list is
/// untouched by this change.
pub const ADOPT_WRITE_ALLOWLIST: [Reg; 1] = [Reg::new(PLANE_SURF_1_A)];

/// The firmware's scanout as the existing readout found it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareScanout {
    pub pipe: Pipe,
    pub plane: PlaneState,
    /// The raw `PLANE_SURF` register (low bits included) - what the rollback writes back.
    pub surf_raw: u32,
}

impl FirmwareScanout {
    /// Pick the primary plane of an ACTIVE pipe from the readout.
    pub fn from_readout(r: &ReadoutState, pipe: Pipe, surf_raw: u32) -> Result<FirmwareScanout, ModesetError> {
        let p = &r.pipes[pipe.index()];
        if !matches!(p.state, PipeState::Active(_)) {
            return Err(ModesetError::ReadoutMismatch { what: "pipe is not active" });
        }
        match p.planes[0] {
            Some(Ok(plane)) => Ok(FirmwareScanout { pipe, plane, surf_raw }),
            _ => Err(ModesetError::ReadoutMismatch { what: "primary plane unreadable" }),
        }
    }
}

/// The replacement: a new address and the layout the caller CLAIMS it has (checked against the firmware's, field by field).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplacementSurface {
    /// GGTT address of the new surface.
    pub gtt_addr: u32,
    pub format: PlaneFormat,
    pub rgb_order: bool,
    pub alpha: bool,
    pub tiling: Tiling,
    pub stride_bytes: u32,
    pub width: u32,
    pub height: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub rotation: Rotation,
}

/// `skl_plane_min_alignment()` for colour plane 0.
const fn min_alignment(t: Tiling) -> u32 {
    match t {
        Tiling::Linear | Tiling::X => 256 * 1024,
        Tiling::Y | Tiling::Yf => 1024 * 1024,
    }
}

/// The adoption plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdoptPlan {
    /// The write: `PLANE_SURF = new address`, then a posting read of it.
    pub write: Plan,
    /// The bounded wait for `PLANE_SURFLIVE` to show the new address.
    pub latch: PollPlan,
    /// Put the scanout back: `PLANE_SURF = the raw value the firmware left`. Safe to run whether or not the write took effect.
    pub rollback: Plan,
    pub old_surf: u32,
    pub new_surf: u32,
}

/// Plan the first plane write. Refuses, by name, anything that is not a pure change of address.
pub fn plan_surface_replacement(fw: &FirmwareScanout, new: &ReplacementSurface) -> Result<AdoptPlan, ModesetError> {
    let old = &fw.plane;
    if fw.pipe != Pipe::A || old.plane_id != 0 {
        return Err(ModesetError::PlaneNotInAllowList { pipe: fw.pipe, plane_id: old.plane_id });
    }
    if !old.enabled {
        return Err(ModesetError::PlaneDisabled);
    }
    // Same layout, first difference named. The order is the order of the registers a layout change would have to write.
    if old.format != Some(new.format) || old.rgb_order != new.rgb_order || old.alpha != new.alpha {
        return Err(ModesetError::LayoutDiffers { field: "format" });
    }
    if old.tiling != Some(new.tiling) {
        return Err(ModesetError::LayoutDiffers { field: "tiling" });
    }
    if old.render_decompression {
        return Err(ModesetError::LayoutDiffers { field: "render_decompression" });
    }
    if old.rotation != new.rotation || old.rotation != Rotation::R0 {
        return Err(ModesetError::LayoutDiffers { field: "rotation" });
    }
    if old.stride_bytes != Some(new.stride_bytes) {
        return Err(ModesetError::LayoutDiffers { field: "stride" });
    }
    if old.width != new.width || old.height != new.height {
        return Err(ModesetError::LayoutDiffers { field: "size" });
    }
    if old.offset_x != new.offset_x || old.offset_y != new.offset_y {
        return Err(ModesetError::LayoutDiffers { field: "offset" });
    }
    let need = min_alignment(new.tiling);
    if new.gtt_addr % need != 0 {
        return Err(ModesetError::SurfMisaligned { surf: new.gtt_addr, need });
    }
    if new.gtt_addr == old.surf {
        return Err(ModesetError::SameSurface);
    }
    let reg = Reg::new(PLANE_SURF_1_A);
    let write = vec![Effect::Write { reg, value: new.gtt_addr }, Effect::PostingRead { reg }];
    let rollback = vec![Effect::Write { reg, value: fw.surf_raw }, Effect::PostingRead { reg }];
    Ok(AdoptPlan { write, latch: latch_poll(fw.pipe, old.plane_id, new.gtt_addr), rollback, old_surf: old.surf, new_surf: new.gtt_addr })
}

impl AdoptPlan {
    /// The registers the forward plan writes (for the allow-list check).
    pub fn written(&self) -> alloc::vec::Vec<Reg> {
        written_regs(&self.write)
    }
}

/// State of the old buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatchState {
    /// The write has not been confirmed live yet.
    Pending,
    /// `PLANE_SURFLIVE` showed the new address: the hardware is scanning the new surface.
    Confirmed { polls: u32 },
    /// The bound ran out: the old buffer must stay pinned and the plan's rollback is the way back.
    TimedOut(PollTimeout),
}

/// Drives the latch confirmation and is the ONLY thing that can authorise releasing the old buffer.
#[derive(Clone, Copy, Debug)]
pub struct AdoptSession {
    poll: PollPlan,
    polls: u32,
    old_surf: u32,
    state: LatchState,
}

/// The right to release the old buffer - constructible only from a confirmed session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OldBufferRelease {
    pub old_surf: u32,
}

impl AdoptSession {
    pub fn begin(plan: &AdoptPlan) -> AdoptSession {
        AdoptSession { poll: plan.latch, polls: 0, old_surf: plan.old_surf, state: LatchState::Pending }
    }
    pub const fn state(&self) -> LatchState {
        self.state
    }
    /// Feed one read of `PLANE_SURFLIVE`. Returns `true` while more samples are wanted.
    pub fn observe(&mut self, surflive: u32) -> bool {
        if !matches!(self.state, LatchState::Pending) {
            return false;
        }
        self.polls += 1;
        match self.poll.judge(self.polls, surflive) {
            PollStep::Satisfied => {
                self.state = LatchState::Confirmed { polls: self.polls };
                false
            }
            PollStep::Again => true,
            PollStep::TimedOut(t) => {
                self.state = LatchState::TimedOut(t);
                false
            }
        }
    }
    /// Release the old buffer - refused unless the hardware showed the new address.
    pub fn release_old_buffer(&self) -> Result<OldBufferRelease, ModesetError> {
        match self.state {
            LatchState::Confirmed { .. } => Ok(OldBufferRelease { old_surf: self.old_surf }),
            LatchState::Pending => Err(ModesetError::OldBufferStillLive),
            LatchState::TimedOut(t) => Err(ModesetError::WaitTimedOut { what: t.what, polls: t.polls }),
        }
    }
}
