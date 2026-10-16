// SPDX-License-Identifier: GPL-2.0-only
//! THE SCANOUT PAIR'S LIFECYCLE (D18; i915-wire 1.5 SCANOUT-*; CoralOS SCANOUT-BUFFER-LIFECYCLE): two images of the adopted layout the
//! display authority draws into, and the flips between them. This module is the STATE MACHINE and nothing else: it holds no memory,
//! writes no register and reads none - the caller (the presentation increment's service glue) creates and binds the images, executes
//! the one-register flip plan it is handed, and feeds back the `PLANE_SURFLIVE` reads. Every rule of the wire is decided HERE, so it
//! is tested here, against the real planner (`i915_display_modeset_core::adopt`).
//!
//! THE RULES. An image is FREE, RENDERING (the authority may draw), COMMITTED (a flip to it is planned; it waits for its GPU work and
//! then for the latch) or SCANNING_OUT. Only the owning session may begin, present or wait (`SCANOUT-NOT-YOURS`). Drawing into a
//! COMMITTED or SCANNING_OUT image is `SCANOUT-REUSE-VIOLATION` - through `SCANOUT-BEGIN` and through a GPU write
//! ([`ScanoutPair::check_render_write`]). One flip is in flight at a time. The flip is armed ONLY after the GPU work writing the image
//! is complete, and confirmed ONLY when `PLANE_SURFLIVE` shows the new surface (a bounded wait, the planner's `LATCH_TIMEOUT_US`). An
//! image - or the firmware's own framebuffer, kept until the FIRST replacement is proven latched - becomes FREE again only when a NEWER
//! surface is proven latched. A latch that never shows is counted (`latch_timeouts`) and RELEASES NOTHING: the old surface stays
//! SCANNING_OUT, the new one stays COMMITTED, further presents are refused until a later read shows the latch after all.
//!
//! THE AUTHORITY'S END. When the display authority's session ends with one of its images on the glass, the pair is ORPHANED
//! ([`ScanoutPair::orphan`]) and the plane is put back on the FIRMWARE's framebuffer - the surface the readout found, whose GGTT extent
//! stays reserved for the life of the device - by the same one-register flip, confirmed by the same bounded latch read
//! ([`ScanoutPair::ready_to_restore`]). The CPU compositor that replaces a dead GPU compositor draws into that framebuffer (the boot
//! framebuffer the kernel maps), so without the restore its frames would never reach the glass. The images stay allocated and are
//! FREE once the restore is latched; a new authority takes them over as before.
use i915_display_modeset_core::adopt::{plan_surface_replacement, AdoptPlan, AdoptSession, FirmwareScanout, LatchState, ReplacementSurface};
use i915_display_modeset_core::initial::AdoptedDisplay;
use i915_display_modeset_core::ModesetError;
use i915_kbl_display_core::plane::Rotation;
use i915_wire::errno;

/// Where an image is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageState {
    Free,
    Rendering,
    Committed { seq: u64 },
    ScanningOut { seq: u64 },
}

impl ImageState {
    pub const fn name(&self) -> &'static str {
        match self {
            ImageState::Free => "FREE",
            ImageState::Rendering => "RENDERING",
            ImageState::Committed { .. } => "COMMITTED",
            ImageState::ScanningOut { .. } => "SCANNING_OUT",
        }
    }
    /// Does the display engine hold it (its pages may be neither drawn into nor reused)?
    pub const fn display_holds(&self) -> bool {
        matches!(self, ImageState::Committed { .. } | ImageState::ScanningOut { .. })
    }
}

/// One scanout image: the authority's GEM handle, the caller's key for the object (to match GPU writes), its GGTT address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Image {
    pub handle: u32,
    pub key: u64,
    pub ggtt: u32,
    pub state: ImageState,
}

/// A refused scanout call, named, with the errno the wire carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScanoutError {
    /// The calling session is not the pair's owner.
    NotYours { inc: u64, owner: u64 },
    /// No image of the pair has this handle.
    UnknownImage { handle: u32 },
    /// Drawing into an image the display holds.
    ReuseViolation { handle: u32, state: ImageState },
    /// Presenting an image that is not RENDERING (never begun, or already committed / on the glass).
    NotRendering { handle: u32, state: ImageState },
    /// A flip is still in flight (armed, or its latch unconfirmed): one at a time.
    FlipInFlight { seq: u64 },
    /// The surface-replacement planner refused the image (it is not the adopted layout).
    Plan(ModesetError),
}

impl ScanoutError {
    pub const fn name(&self) -> &'static str {
        match self {
            ScanoutError::NotYours { .. } => "SCANOUT-NOT-YOURS",
            ScanoutError::UnknownImage { .. } => "SCANOUT-UNKNOWN-IMAGE",
            ScanoutError::ReuseViolation { .. } => "SCANOUT-REUSE-VIOLATION",
            ScanoutError::NotRendering { .. } => "SCANOUT-NOT-RENDERING",
            ScanoutError::FlipInFlight { .. } => "SCANOUT-FLIP-IN-FLIGHT",
            ScanoutError::Plan(e) => e.name(),
        }
    }
    pub const fn errno(&self) -> i32 {
        match self {
            ScanoutError::NotYours { .. } => errno::EACCES,
            ScanoutError::UnknownImage { .. } => errno::ENOENT,
            ScanoutError::ReuseViolation { .. } | ScanoutError::FlipInFlight { .. } => errno::EBUSY,
            ScanoutError::NotRendering { .. } | ScanoutError::Plan(_) => errno::EINVAL,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    /// Waiting for the GPU work writing the image to complete; nothing is written yet.
    AwaitGpu,
    /// `PLANE_SURF` is written; the latch confirmation is running.
    AwaitLatch(AdoptSession),
    /// The bounded latch confirmation ran out; reads go on (each a fresh bounded window), and nothing is released.
    Unconfirmed(AdoptSession),
}

/// The flip's target index that names the firmware's framebuffer (the restore), never an image.
const FIRMWARE_IDX: usize = usize::MAX;

#[derive(Clone, Debug)]
struct Flip {
    /// The image flipped to, or [`FIRMWARE_IDX`] for the restore.
    idx: usize,
    seq: u64,
    plan: AdoptPlan,
    phase: Phase,
}

/// What was released by a latch: the firmware's framebuffer (the first latch) or the image that was on the glass before.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Released {
    Firmware,
    Image { handle: u32 },
}

/// The outcome of one `PLANE_SURFLIVE` read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatchStep {
    /// No flip is waiting for a latch.
    Idle,
    StillWaiting,
    /// Proven on the glass; the previous surface is released.
    Latched { seq: u64, released: Released },
    /// The bounded confirmation ran out (counted once per flip): nothing is released.
    TimedOut { seq: u64, got: u32 },
    /// The restore is proven on the glass: the firmware's framebuffer is scanned again and every image is FREE.
    Restored,
}

/// What a `SCANOUT-WAIT` on `seq` can answer now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeqState {
    Latched,
    InFlight,
    /// Its latch confirmation timed out (`-EIO` on the wire); a later read may still confirm it.
    LatchUnconfirmed,
    /// Never presented on this pair.
    Unknown,
}

/// The `SCANOUT-WAIT` reply's facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub latched_seq: u64,
    /// The handle on the glass; 0 = the firmware's framebuffer.
    pub scanning: u32,
    pub free_mask: u32,
    pub latch_timeouts: u32,
}

/// The pair of one display authority.
#[derive(Clone, Debug)]
pub struct ScanoutPair {
    owner: u64,
    images: [Image; 2],
    /// What the plane scans NOW, as the planner sees it: the firmware's surface until the first latch, each latched image after.
    live: FirmwareScanout,
    /// The firmware's own scanout, as the readout found it: what the restore puts back.
    firmware: FirmwareScanout,
    template: ReplacementSurface,
    firmware_live: bool,
    /// The authority ended with an image on the glass: put the firmware's framebuffer back when no flip is in flight.
    restore_wanted: bool,
    next_seq: u64,
    latched_seq: u64,
    latch_timeouts: u32,
    flip: Option<Flip>,
}

impl ScanoutPair {
    /// A pair for session `owner` over the adopted display: `live` is the firmware's scanout as the readout found it, `images` the two
    /// images' `(handle, key, ggtt)` - created at the adopted layout by the caller. Both start FREE.
    pub fn new(owner: u64, live: FirmwareScanout, adopted: &AdoptedDisplay, images: [(u32, u64, u32); 2]) -> ScanoutPair {
        let fb = &adopted.fb;
        let template = ReplacementSurface {
            gtt_addr: 0,
            format: fb.format,
            rgb_order: fb.rgb_order,
            alpha: fb.alpha,
            tiling: fb.tiling,
            stride_bytes: fb.stride,
            width: fb.width,
            height: fb.height,
            offset_x: 0,
            offset_y: 0,
            rotation: Rotation::R0,
        };
        let img = |(handle, key, ggtt): (u32, u64, u32)| Image { handle, key, ggtt, state: ImageState::Free };
        ScanoutPair { owner, images: [img(images[0]), img(images[1])], live, firmware: live, template, firmware_live: true, restore_wanted: false, next_seq: 1, latched_seq: 0, latch_timeouts: 0, flip: None }
    }

    pub const fn owner(&self) -> u64 {
        self.owner
    }

    /// A NEW display authority takes the pair over (the previous one's session ended): its own handles for the same two images. Nothing
    /// on the glass moves - a flip in flight goes on, the image scanned out stays SCANNING_OUT - and an image the dead authority was
    /// drawing into is FREE again (its half-drawn content is never presented). `owner` 0 orphans the pair: nobody may call it.
    pub fn rebind_owner(&mut self, owner: u64, handles: [u32; 2]) {
        self.owner = owner;
        for (img, h) in self.images.iter_mut().zip(handles) {
            img.handle = h;
            if img.state == ImageState::Rendering {
                img.state = ImageState::Free;
            }
        }
    }

    /// The authority's session ended: no session may call the pair (`owner` 0), an image it was drawing into is FREE, a flip it had not
    /// yet armed is dropped (nothing reached the glass), and - when an image is on the glass, or a flip to one is in flight - the plane
    /// goes back to the firmware's framebuffer ([`ScanoutPair::ready_to_restore`]).
    pub fn orphan(&mut self) {
        self.rebind_owner(0, [0, 0]);
        if let Some(Flip { idx, phase: Phase::AwaitGpu, .. }) = &self.flip {
            let i = *idx;
            self.flip = None;
            self.images[i].state = ImageState::Free;
        }
        self.restore_wanted = !self.firmware_live || self.flip.is_some();
    }

    /// Is a restore of the firmware's framebuffer owed or in flight?
    pub fn restoring(&self) -> bool {
        self.restore_wanted || matches!(&self.flip, Some(Flip { idx: FIRMWARE_IDX, .. }))
    }

    /// The restore's one-register flip, to execute NOW - once the pair is orphaned and no flip is in flight (an armed one finishes
    /// first, so the planner works from what the plane really scans). `None`: nothing to restore yet. A plan the planner refuses
    /// drops the restore (the error is returned for the log; nothing was written).
    pub fn ready_to_restore(&mut self) -> Option<Result<AdoptPlan, ScanoutError>> {
        if !self.restore_wanted || self.owner != 0 || self.flip.is_some() {
            return None;
        }
        self.restore_wanted = false;
        if self.firmware_live {
            return None; // the in-flight flip that wanted it never latched over the firmware: nothing to put back
        }
        let fw = ReplacementSurface { gtt_addr: self.firmware.plane.surf, ..self.template };
        match plan_surface_replacement(&self.live, &fw) {
            Ok(plan) => {
                self.flip = Some(Flip { idx: FIRMWARE_IDX, seq: 0, plan: plan.clone(), phase: Phase::AwaitLatch(AdoptSession::begin(&plan)) });
                Some(Ok(plan))
            }
            Err(e) => Some(Err(ScanoutError::Plan(e))),
        }
    }

    /// The firmware framebuffer's GGTT address (what the restore writes).
    pub const fn firmware_surf(&self) -> u32 {
        self.firmware.plane.surf
    }

    /// The restore's write could not be executed: nothing changed on the glass, and it is not retried.
    pub fn restore_failed(&mut self) {
        if matches!(&self.flip, Some(Flip { idx: FIRMWARE_IDX, .. })) {
            self.flip = None;
        }
    }

    /// The register whose reads [`ScanoutPair::observe`] wants while a flip is armed (`PLANE_SURFLIVE` of the adopted plane).
    pub fn latch_reg(&self) -> Option<i915_common_core::Reg> {
        self.flip.as_ref().map(|f| f.plan.latch.reg)
    }

    /// The handle of the image the armed flip waits for the GPU on, with its key.
    pub fn awaiting_gpu_image(&self) -> Option<Image> {
        match &self.flip {
            Some(Flip { idx, phase: Phase::AwaitGpu, .. }) => Some(self.images[*idx]),
            _ => None,
        }
    }
    pub fn images(&self) -> &[Image; 2] {
        &self.images
    }
    pub const fn firmware_live(&self) -> bool {
        self.firmware_live
    }

    fn mine(&self, inc: u64, handle: u32) -> Result<usize, ScanoutError> {
        if inc != self.owner {
            return Err(ScanoutError::NotYours { inc, owner: self.owner });
        }
        self.images.iter().position(|i| i.handle == handle).ok_or(ScanoutError::UnknownImage { handle })
    }

    /// `SCANOUT-BEGIN`: FREE -> RENDERING (RENDERING stays RENDERING); an image the display holds is a reuse violation.
    pub fn begin(&mut self, inc: u64, handle: u32) -> Result<(), ScanoutError> {
        let i = self.mine(inc, handle)?;
        let st = self.images[i].state;
        match st {
            ImageState::Free | ImageState::Rendering => {
                self.images[i].state = ImageState::Rendering;
                Ok(())
            }
            _ => Err(ScanoutError::ReuseViolation { handle, state: st }),
        }
    }

    /// May the GPU WRITE the object `key`? Any object that is not one of the pair's images, yes; an image only while RENDERING.
    pub fn check_render_write(&self, key: u64) -> Result<(), ScanoutError> {
        match self.images.iter().find(|i| i.key == key) {
            Some(i) if i.state != ImageState::Rendering => Err(ScanoutError::ReuseViolation { handle: i.handle, state: i.state }),
            _ => Ok(()),
        }
    }

    /// Does the display hold the object `key` (COMMITTED or SCANNING_OUT)? Its pages may not be freed or reused while it does.
    pub fn display_holds(&self, key: u64) -> bool {
        self.images.iter().any(|i| i.key == key && i.state.display_holds())
    }

    /// `SCANOUT-PRESENT`: RENDERING -> COMMITTED, the flip planned (one `PLANE_SURF` write, checked by the planner against the live
    /// layout) but NOT armed: [`ScanoutPair::ready_to_arm`] hands it over once the GPU work is done. Returns the present's sequence.
    pub fn present(&mut self, inc: u64, handle: u32) -> Result<u64, ScanoutError> {
        let i = self.mine(inc, handle)?;
        if let Some(f) = &self.flip {
            return Err(ScanoutError::FlipInFlight { seq: f.seq });
        }
        let st = self.images[i].state;
        if st != ImageState::Rendering {
            return Err(ScanoutError::NotRendering { handle, state: st });
        }
        let new = ReplacementSurface { gtt_addr: self.images[i].ggtt, ..self.template };
        let plan = plan_surface_replacement(&self.live, &new).map_err(ScanoutError::Plan)?;
        let seq = self.next_seq;
        self.next_seq += 1;
        self.images[i].state = ImageState::Committed { seq };
        self.flip = Some(Flip { idx: i, seq, plan, phase: Phase::AwaitGpu });
        Ok(seq)
    }

    /// The key of the image whose GPU work the armed flip waits for, if a flip waits for its GPU work.
    pub fn awaiting_gpu(&self) -> Option<u64> {
        match &self.flip {
            Some(Flip { idx, phase: Phase::AwaitGpu, .. }) => Some(self.images[*idx].key),
            _ => None,
        }
    }

    /// The GPU work writing the committed image is complete: arm the flip. Returns the plan the caller executes NOW (its write set
    /// is `ADOPT_WRITE_ALLOWLIST`); `None` when no flip waits for its GPU work. From here the latch confirmation runs.
    pub fn ready_to_arm(&mut self) -> Option<AdoptPlan> {
        let f = self.flip.as_mut()?;
        if !matches!(f.phase, Phase::AwaitGpu) {
            return None;
        }
        f.phase = Phase::AwaitLatch(AdoptSession::begin(&f.plan));
        Some(f.plan.clone())
    }

    /// The caller could not execute the armed write (its guard refused, the window faulted): the image goes back to RENDERING, the
    /// flip is dropped, nothing on the glass changed. Returns the dropped present's sequence.
    pub fn arm_failed(&mut self) -> Option<u64> {
        if matches!(&self.flip, Some(Flip { idx: FIRMWARE_IDX, .. })) {
            return None; // the restore is not an armed present: restore_failed drops it
        }
        let f = self.flip.take()?;
        self.images[f.idx].state = ImageState::Rendering;
        Some(f.seq)
    }

    /// Feed one `PLANE_SURFLIVE` read.
    pub fn observe(&mut self, surflive: u32) -> LatchStep {
        let Some(f) = self.flip.as_mut() else { return LatchStep::Idle };
        let (mut session, unconfirmed) = match f.phase {
            Phase::AwaitGpu => return LatchStep::Idle,
            Phase::AwaitLatch(s) => (s, false),
            Phase::Unconfirmed(s) => (s, true),
        };
        session.observe(surflive);
        match session.state() {
            LatchState::Pending => {
                f.phase = if unconfirmed { Phase::Unconfirmed(session) } else { Phase::AwaitLatch(session) };
                LatchStep::StillWaiting
            }
            LatchState::TimedOut(t) => {
                let seq = f.seq;
                f.phase = Phase::Unconfirmed(AdoptSession::begin(&f.plan)); // a fresh bounded window for later reads
                if unconfirmed {
                    return LatchStep::StillWaiting;
                }
                self.latch_timeouts += 1;
                LatchStep::TimedOut { seq, got: t.got }
            }
            LatchState::Confirmed { .. } => self.latched(),
        }
    }

    /// The flip is proven on the glass: the new image SCANNING_OUT, the surface before it released.
    fn latched(&mut self) -> LatchStep {
        let Some(f) = self.flip.take() else { return LatchStep::Idle };
        if f.idx == FIRMWARE_IDX {
            for img in self.images.iter_mut() {
                if matches!(img.state, ImageState::ScanningOut { .. }) {
                    img.state = ImageState::Free;
                }
            }
            self.live = self.firmware;
            self.firmware_live = true;
            return LatchStep::Restored;
        }
        let released = match self.images.iter().position(|i| matches!(i.state, ImageState::ScanningOut { .. })) {
            Some(prev) => {
                self.images[prev].state = ImageState::Free;
                Released::Image { handle: self.images[prev].handle }
            }
            None => {
                self.firmware_live = false;
                Released::Firmware
            }
        };
        let img = &mut self.images[f.idx];
        img.state = ImageState::ScanningOut { seq: f.seq };
        self.live.plane.surf = img.ggtt;
        self.live.surf_raw = img.ggtt;
        self.latched_seq = f.seq;
        LatchStep::Latched { seq: f.seq, released }
    }

    /// Is a flip in flight whose latch the caller should read for (armed, or unconfirmed)?
    pub fn wants_latch_reads(&self) -> bool {
        matches!(&self.flip, Some(Flip { phase: Phase::AwaitLatch(_) | Phase::Unconfirmed(_), .. }))
    }

    pub fn seq_state(&self, seq: u64) -> SeqState {
        if seq != 0 && seq <= self.latched_seq {
            return SeqState::Latched;
        }
        match &self.flip {
            Some(f) if f.seq == seq => match f.phase {
                Phase::Unconfirmed(_) => SeqState::LatchUnconfirmed,
                _ => SeqState::InFlight,
            },
            _ => SeqState::Unknown,
        }
    }

    pub fn status(&self) -> Status {
        let scanning = self.images.iter().find(|i| matches!(i.state, ImageState::ScanningOut { .. })).map_or(0, |i| i.handle);
        let free_mask = self.images.iter().enumerate().filter(|(_, i)| i.state == ImageState::Free).fold(0, |m, (k, _)| m | 1 << k);
        Status { latched_seq: self.latched_seq, scanning, free_mask, latch_timeouts: self.latch_timeouts }
    }
}
