// SPDX-License-Identifier: GPL-2.0-only
//! DISPLAY OWNERSHIP (D18, Astra's Kaby Lake single-owner ruling 2026-10-16): this device owns the whole GPU function - render AND
//! display - so the display the firmware lit is THIS device's to keep. It keeps it by ADOPTION: the takeover readout is judged by
//! `i915_display_modeset_core::initial::adopt`, and an adopted display is (1) never written by bring-up, (2) reserved in the GGTT
//! before any render allocation (`Gtt::new_ggtt_preserving`, through [`Adoption::keep`]), (3) offered as the ONE mode the device has,
//! with any other refused by name, and (4) described on the wire (`DISPLAY-QUERY`, i915-wire 1.5) to anyone who asks.
//!
//! | piece | what it does |
//! |---|---|
//! | [`Adoption`] | the verdict: adopted (and what), refused (and why), or no readout at all |
//! | [`answer`] | the session-less display answers both loops give (`DISPLAY-QUERY`; the authority calls refused without an authority) |
//! | [`scanout`] | the scanout pair's LIFECYCLE (FREE / RENDERING / COMMITTED / SCANNING_OUT): who may draw, present and wait, when a flip is armed, when a surface is released |
//!
//! Nothing here writes a register: the flip plan `scanout` hands over is executed by the presentation increment's service glue.
pub mod answer;
pub mod scanout;

use crate::log::{emit, Log};
use i915_display_modeset_core::initial::{adopt, AdoptRefusal, AdoptedDisplay};
use i915_kbl_display_core::readout::ReadoutState;
use i915_memory_core::Preallocated;
use i915_wire::msg::display::{refusal, state, DisplayQueryRep};

/// The GGTT reservation name of the adopted framebuffer: what a refused bind over it names.
pub const ADOPTED_SCANOUT: &str = "adopted-scanout";

/// The framebuffer region's page size for the extent: stolen system memory is `I915_GTT_PAGE_SIZE_4K` on every part without
/// `HAS_64K_PAGES` (i915_gem_stolen.c:985-986); devinfo has no Kaby Lake (or earlier) row with 64 KiB pages.
pub const FB_MIN_PAGE: u64 = i915_devinfo_core::GTT_PAGE_SIZE_4K as u64;

/// What the device made of the firmware's display.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Adoption {
    Adopted(AdoptedDisplay),
    Refused(AdoptRefusal),
    /// The readout itself failed (a register read was refused): nothing is known, so nothing is adopted.
    NoReadout,
}

impl Adoption {
    /// Judge the readout.
    pub fn of(state: &ReadoutState) -> Adoption {
        match adopt(state, FB_MIN_PAGE) {
            Ok(a) => Adoption::Adopted(a),
            Err(r) => Adoption::Refused(r),
        }
    }

    pub fn adopted(&self) -> Option<&AdoptedDisplay> {
        match self {
            Adoption::Adopted(a) => Some(a),
            _ => None,
        }
    }

    /// The GGTT range bring-up must keep exactly as found (the framebuffer the panel is showing), if any.
    pub fn keep(&self) -> Option<Preallocated> {
        self.adopted().map(|a| Preallocated { start: a.extent.start, size: a.extent.size, what: ADOPTED_SCANOUT })
    }

    /// The wire's refusal code for this verdict (`i915_wire::msg::display::refusal`).
    pub fn refusal_code(&self) -> u32 {
        match self {
            Adoption::Adopted(_) => refusal::NONE,
            Adoption::NoReadout => refusal::READOUT_FAILED,
            Adoption::Refused(r) => match r {
                AdoptRefusal::NoActivePipe => refusal::NO_ACTIVE_PIPE,
                AdoptRefusal::MultipleActivePipes { .. } => refusal::MULTIPLE_ACTIVE_PIPES,
                AdoptRefusal::PlaneUnreadable => refusal::PLANE_UNREADABLE,
                AdoptRefusal::PlaneDisabled => refusal::PLANE_DISABLED,
                AdoptRefusal::FormatNotRgb => refusal::FORMAT_NOT_RGB,
                AdoptRefusal::Compressed => refusal::COMPRESSED,
                AdoptRefusal::Rotated => refusal::ROTATED,
                AdoptRefusal::OffsetNonZero { .. } => refusal::OFFSET_NONZERO,
                AdoptRefusal::StrideUnknown => refusal::STRIDE_UNKNOWN,
                AdoptRefusal::PlaneNotPipeSource { .. } => refusal::PLANE_NOT_PIPE_SOURCE,
                AdoptRefusal::PlaneScaler { .. } => refusal::PLANE_SCALER,
                AdoptRefusal::UnscaledSizeMismatch { .. } => refusal::UNSCALED_SIZE_MISMATCH,
                AdoptRefusal::Interlaced => refusal::INTERLACED,
                AdoptRefusal::ExtentOverflow => refusal::EXTENT_OVERFLOW,
            },
        }
    }

    /// The `DISPLAY-QUERY` reply body.
    pub fn query_reply(&self) -> DisplayQueryRep {
        let Some(a) = self.adopted() else { return DisplayQueryRep { refusal: self.refusal_code(), ..Default::default() } };
        let m = a.mode();
        DisplayQueryRep {
            adopted: state::ADOPTED,
            refusal: refusal::NONE,
            pipe: a.pipe.index() as u32,
            plane: a.fb.plane_id as u32,
            width: a.fb.width,
            height: a.fb.height,
            stride: a.fb.stride,
            fourcc: a.fb.fourcc,
            modifier: a.fb.modifier,
            active_width: a.active_w,
            active_height: a.active_h,
            refresh_millihz: m.refresh_millihz,
            scaled: a.scaled() as u32,
            mode_count: 1,
            ..Default::default()
        }
    }

    /// The one `[I915-ADOPT ...]` line: what is kept (and that nothing was written), or why nothing is.
    pub fn report<L: Log>(&self, log: &mut L) {
        match self {
            Adoption::Adopted(a) => emit(log, format_args!(
                "[I915-ADOPT OK pipe={} trans={} plane={} fb={}x{} stride={} fourcc={} mod={:#x} ggtt={:#x}+{:#x} panel={}x{} scaled={} refresh_mhz={} wells={:#x} writes=0]",
                a.pipe.letter(), a.transcoder.name(), a.fb.plane_id + 1, a.fb.width, a.fb.height, a.fb.stride, Fourcc(a.fb.fourcc), a.fb.modifier,
                a.extent.start, a.extent.size, a.active_w, a.active_h, a.scaled() as u8, a.mode().refresh_millihz, a.wells_on)),
            Adoption::Refused(r) => emit(log, format_args!("[I915-ADOPT REFUSED why={} detail={:?} - the display is left exactly as the firmware set it; no mode is offered]", r.name(), r)),
            Adoption::NoReadout => emit(log, format_args!("[I915-ADOPT REFUSED why=ADOPT-READOUT-FAILED - no readout, nothing adopted, nothing written]")),
        }
    }
}

/// A fourcc as its four characters.
struct Fourcc(u32);
impl core::fmt::Display for Fourcc {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        for b in self.0.to_le_bytes() {
            let c = if b.is_ascii_graphic() { b as char } else { '?' };
            core::fmt::Write::write_char(f, c)?;
        }
        Ok(())
    }
}
