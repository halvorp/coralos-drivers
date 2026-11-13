// SPDX-License-Identifier: GPL-2.0-only
//! Cherryview's GT power wells (DECISIONS.md D8): the GT wake gate and the render / media well status, before any engine register is
//! touched. On Gen9 the forcewake MEDIA domain is the whole story; on Cherryview the GUnit also gates GT waking
//! (`VLV_GTLC_WAKE_CTRL.ALLOWWAKEREQ`, acknowledged in `VLV_GTLC_PW_STATUS.ALLOWWAKEACK`) and reports each well's power
//! (`VLV_GTLC_PW_RENDER_STATUS_MASK`, `VLV_GTLC_PW_MEDIA_STATUS_MASK`). The video engine's MMIO (VCS0, 0x12000: MEDIA in
//! `__chv_fw_ranges`) is only reached with the media well up.
//!
//! Ported from drivers/gpu/drm/i915/vlv_suspend.c (v7.0-rc6): `vlv_wait_for_pw_status` (:281-303), `vlv_allow_gt_wake`
//! (:329-348) and `vlv_wait_for_gt_wells` (:350-370). The CoralOS silicon reference for the same facts is coralos
//! `userland/coral-i915-gem/src/gt_power.rs` (`media_well`: the media well is woken through FORCEWAKE_MEDIA_VLV before a media
//! register is written); only its facts are used, never its code. Copyright (c) Intel Corporation and the i915 authors.
//!
//! Every access goes through the caller's `Mmio` (the proof of life's write guard): the only write here is `VLV_GTLC_WAKE_CTRL`, and only
//! when the firmware left GT waking disallowed.
use coralos_hal::Mmio;
use i915_common_core::I915Error;
use i915_gt_regs_core as r;

/// "Transitioning between RC6 states should be at most 2ms ... so use a 3ms timeout" (vlv_suspend.c:292-293).
pub const PW_STATUS_TIMEOUT_MS: u64 = 3; // vlv_suspend.c:295-296

/// Iteration cap for a host whose `Mmio` has no clock (the deadline is the real bound on silicon).
const MAX_POLLS: u32 = 100_000;

/// `VLV_GTLC_PW_STATUS` decoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PwStatus(pub u32);

impl PwStatus {
    pub const fn allow_wake_ack(self) -> bool {
        self.0 & r::VLV_GTLC_ALLOWWAKEACK != 0
    }
    /// "GT register access while GT waking disabled" (`vlv_check_no_gt_access`, vlv_suspend.c:372-380).
    pub const fn allow_wake_err(self) -> bool {
        self.0 & r::VLV_GTLC_ALLOWWAKEERR != 0
    }
    pub const fn render_on(self) -> bool {
        self.0 & r::VLV_GTLC_PW_RENDER_STATUS_MASK != 0
    }
    pub const fn media_on(self) -> bool {
        self.0 & r::VLV_GTLC_PW_MEDIA_STATUS_MASK != 0
    }
}

/// One read of `VLV_GTLC_PW_STATUS`; all-ones (a BAR that answers nothing) is refused by name.
fn read_pw_status<M: Mmio>(m: &mut M) -> Result<PwStatus, I915Error> {
    match m.r32(r::VLV_GTLC_PW_STATUS.offset()) {
        u32::MAX => Err(I915Error::MmioUnreliable { domain: 0, reg: r::VLV_GTLC_PW_STATUS }),
        v => Ok(PwStatus(v)),
    }
}

/// What [`allow_gt_wake`] found and did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GtWake {
    /// The firmware left GT waking allowed (`ALLOWWAKEACK` set): nothing was written.
    AlreadyAllowed,
    /// `ALLOWWAKEREQ` was set and acknowledged: one write.
    Allowed,
}

/// Poll `VLV_GTLC_PW_STATUS` until `(status & mask) == val` (`vlv_wait_for_pw_status`, vlv_suspend.c:281-303), at most
/// [`PW_STATUS_TIMEOUT_MS`]. A timeout is `AckTimeout` naming the register, the mask, what was wanted and the last value; an all-ones
/// read is a dead BAR, never a status (it would satisfy every mask), and is `MmioUnreliable`.
pub fn wait_for_pw_status<M: Mmio>(m: &mut M, mask: u32, val: u32) -> Result<PwStatus, I915Error> {
    m.arm_deadline(PW_STATUS_TIMEOUT_MS);
    let mut polls = 0u32;
    loop {
        let got = read_pw_status(m)?.0;
        polls += 1;
        if got & mask == val {
            return Ok(PwStatus(got));
        }
        if m.deadline_expired() || polls >= MAX_POLLS {
            return Err(I915Error::AckTimeout { domain: 0, reg: r::VLV_GTLC_PW_STATUS, mask, want: val, got, polls });
        }
        m.relax();
    }
}

/// `vlv_allow_gt_wake(i915, true)` (vlv_suspend.c:329-348): if GT waking is not acknowledged, set `ALLOWWAKEREQ` in
/// `VLV_GTLC_WAKE_CTRL` (read-modify-write: the other bits are the firmware's), post it, and wait for `ALLOWWAKEACK`. Linux runs it
/// on resume (`vlv_resume_prepare`); a boot after firmware that left the gate shut is the same state, so the device runs it at bring-up,
/// and writes nothing when the gate is already open.
pub fn allow_gt_wake<M: Mmio>(m: &mut M) -> Result<GtWake, I915Error> {
    if read_pw_status(m)?.allow_wake_ack() {
        return Ok(GtWake::AlreadyAllowed);
    }
    let ctl = m.r32(r::VLV_GTLC_WAKE_CTRL.offset());
    m.w32(r::VLV_GTLC_WAKE_CTRL.offset(), ctl | r::VLV_GTLC_ALLOWWAKEREQ); // intel_uncore_rmw, :336-337
    let _ = m.r32(r::VLV_GTLC_WAKE_CTRL.offset()); // intel_uncore_posting_read, :338
    wait_for_pw_status(m, r::VLV_GTLC_ALLOWWAKEACK, r::VLV_GTLC_ALLOWWAKEACK)?; // :340-343
    Ok(GtWake::Allowed)
}

/// `vlv_wait_for_gt_wells(i915, true)` (vlv_suspend.c:350-370): with the render and media forcewake domains held, both wells report
/// power. Bspec calls these flags debug-only and Linux only logs a timeout (:366-369); the device does the same: the returned status
/// is reported, and a timeout is returned for the caller to NAME, not to stop on.
pub fn wait_for_gt_wells_on<M: Mmio>(m: &mut M) -> Result<PwStatus, I915Error> {
    let mask = r::VLV_GTLC_PW_MEDIA_STATUS_MASK | r::VLV_GTLC_PW_RENDER_STATUS_MASK; // :356
    wait_for_pw_status(m, mask, mask) // val = wait_for_on ? mask : 0, :357
}
