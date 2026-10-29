// SPDX-License-Identifier: GPL-2.0-only
//! THE LOGICAL SCANOUT SIZE, device side: the host's [`LogicalSize`] policy resolved ONCE against the adopted panel, the fastset planned
//! by `i915_display_modeset_core::resize` (or refused by name, keeping the firmware's size), and the markers. Nothing here writes a
//! register: the plan rides on the scanout pair's first flip off the firmware's framebuffer (`scanout`), executed by the presentation
//! increment's glue inside a vblank-evasion window.
//!
//! Markers: `[I915-SCANOUT-SIZE planned logical=WxH panel=WxH scaler=psN ratio=R how=fastset wm_levels=N]` at assembly;
//! `[I915-SCANOUT-SIZE logical=WxH panel=WxH scaler=psN ratio=R how=fastset ok]` when the first flip is proven latched;
//! `[I915-SCANOUT-SIZE REFUSED why=NAME ...]` for every way the adopted size is kept (`why=ddb-too-small need=N have=M` when the wider
//! plane's level-0 watermark does not fit its DDB); `[I915-SCANOUT-HEALTH ...]` a few frames after the latch, and
//! `[I915-SCANOUT-SIZE ROLLBACK why=underrun]` when the pipe underran at the logical size and the firmware's geometry goes back.
use super::Adoption;
use crate::log::{emit, Log};
use i915_display_modeset_core::resize::{plan_scanout_resize, FirmwareGeometry, LogicalSize, ResizeRefusal, ScanoutResize};
use i915_display_modeset_core::scanout_health::ScanoutHealth;
use i915_kbl_display_core::platform::DisplayCaps;

/// A ratio in thousandths as `1.500`.
pub struct Ratio(pub u32);
impl core::fmt::Display for Ratio {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}.{:03}", self.0 / 1000, self.0 % 1000)
    }
}

/// Resolve `policy` for the adopted display and plan the fastset. `None` keeps the firmware's size (the reason is logged by name).
pub fn choose<L: Log>(log: &mut L, caps: &DisplayCaps, adoption: &Adoption, geometry: Option<&FirmwareGeometry>, policy: LogicalSize) -> Option<ScanoutResize> {
    let a = adoption.adopted()?;
    let (w, h) = policy.resolve((a.active_w, a.active_h), (a.fb.width, a.fb.height));
    let Some(fw) = geometry else {
        emit(log, format_args!("[I915-SCANOUT-SIZE REFUSED why=RESIZE-NO-GEOMETRY-READOUT logical={}x{} panel={}x{}] - the adopted {}x{} is kept", w, h, a.active_w, a.active_h, a.fb.width, a.fb.height));
        return None;
    };
    match plan_scanout_resize(caps, a, fw, w, h) {
        Ok(Some(r)) => {
            emit(log, format_args!(
                "[I915-SCANOUT-SIZE planned logical={}x{} panel={}x{} scaler=ps{} ratio={} how=fastset wm_levels={} ddb={} ddb_how={} wm0_need={} stride={} policy={:?}] - applied with the first present",
                r.logical_w, r.logical_h, r.panel_w, r.panel_h, r.scaler_id as u32 + 1, Ratio(r.ratio_milli()), r.wm_levels, r.ddb_blocks, r.ddb_how, r.wm0_need, r.stride, policy));
            Some(r)
        }
        Ok(None) => {
            emit(log, format_args!("[I915-SCANOUT-SIZE logical={}x{} panel={}x{} how=none] - the firmware's size, nothing to change", w, h, a.active_w, a.active_h));
            None
        }
        Err(ResizeRefusal::DdbTooSmall { need, have }) => {
            emit(log, format_args!("[I915-SCANOUT-SIZE REFUSED why=ddb-too-small need={} have={} logical={}x{} panel={}x{}] - the adopted {}x{} is kept", need, have, w, h, a.active_w, a.active_h, a.fb.width, a.fb.height));
            None
        }
        Err(e) => {
            emit(log, format_args!("[I915-SCANOUT-SIZE REFUSED why={} logical={}x{} panel={}x{} detail={:?}] - the adopted {}x{} is kept", e.name(), w, h, a.active_w, a.active_h, e, a.fb.width, a.fb.height));
            None
        }
    }
}

/// The line that says the fastset is on the glass.
pub fn report_ok<L: Log>(log: &mut L, r: &ScanoutResize) {
    emit(log, format_args!("[I915-SCANOUT-SIZE logical={}x{} panel={}x{} scaler=ps{} ratio={} how=fastset ok]", r.logical_w, r.logical_h, r.panel_w, r.panel_h, r.scaler_id as u32 + 1, Ratio(r.ratio_milli())));
}

/// The post-resize health line.
pub fn report_health<L: Log>(log: &mut L, h: &ScanoutHealth) {
    emit(log, format_args!("{}", h));
}

/// The pipe underran at the logical size: the firmware's geometry is being written back.
pub fn report_rollback<L: Log>(log: &mut L, r: &ScanoutResize) {
    emit(log, format_args!("[I915-SCANOUT-SIZE ROLLBACK why=underrun] - logical={}x{} off, the firmware's geometry and framebuffer go back", r.logical_w, r.logical_h));
}
