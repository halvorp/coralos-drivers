// SPDX-License-Identifier: GPL-2.0-only
//! The two GPIO pads the audio path depends on, from the board record: the speaker amplifiers' reset (`SPKR_RST_L`, GPP_F11 on Atlas) and
//! the codec's interrupt (`HP_IRQ`, GPP_F10, level low).
//!
//! THE SPEAKERS ARE HELD IN RESET except while a speaker session runs (D17): only [`release_speakers`], called by a session start after the
//! protection path is armed, drives the line high, and every way out drives it low and proves it. The first thing the device does with a pad is drive `SPKR_RST_L` LOW (active low: low is reset) and
//! read the pad back; an amplifier in reset cannot be configured, cannot be clocked into producing sound and draws nothing, whatever else
//! goes wrong afterwards. That is the hardware half of "minimum safe state = verified amplifier shutdown". It is a WRITE, so it needs a
//! pad window the loader granted writable at exactly this pad's `PADCFG0` ([`crate::allow::PadAllow`]); without the grant the outcome is
//! [`SpeakerHold::NotGranted`] and the status says so - a named GAP the safe state depends on, never a silent skip (2026-10-13; delete-when: the Atlas
//! launch row grants the pad window writable at that register). (With no I2S data on
//! SSP0, no amplifier command on the bus and the amplifiers unconfigured, the speakers are silent regardless: the hold is the belt, those
//! are the braces.)
//!
//! Layout, from Linux `drivers/pinctrl/intel/pinctrl-intel.c`: each community has a window; `PADBAR` (0x00c, :49) holds the offset of the pad
//! configuration registers; pad `n` of the community has two dwords at `n * 8` (:195, no debounce on Sunrise Point-LP, pinctrl-sunrisepoint.c
//! `SPT_LP_COMMUNITY`), `PADCFG0` first.
use crate::machine::{Machine, PadFault, PadReg};
use crate::rig::Rig;
use avs_wire::board::Pad;
use avs_wire::SpeakerHold;

pub const PADBAR: u32 = 0x00c; // pinctrl-intel.c:49
/// Bytes between pads' configuration registers: two dwords (pinctrl-intel.c:184, :195; Sunrise Point-LP has no debounce feature).
pub const PAD_STRIDE: u32 = 8;
pub const PADCFG0_PMODE_MASK: u32 = 0xf << 10; // pinctrl-intel.c:72
pub const PADCFG0_PMODE_GPIO: u32 = 0; // pinctrl-intel.c:73
pub const PADCFG0_GPIORXDIS: u32 = 1 << 9; // pinctrl-intel.c:80
pub const PADCFG0_GPIOTXDIS: u32 = 1 << 8; // pinctrl-intel.c:81
pub const PADCFG0_GPIORXSTATE: u32 = 1 << 1; // pinctrl-intel.c:82
pub const PADCFG0_GPIOTXSTATE: u32 = 1 << 0; // pinctrl-intel.c:83

/// Why a pad could not be located.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadError {
    Fault(PadFault),
    /// `PADBAR` read as all-ones or zero: the window is not a pad controller.
    BadPadbar { value: u32 },
}

impl PadError {
    pub const fn name(&self) -> &'static str {
        match self {
            PadError::Fault(f) => f.name(),
            PadError::BadPadbar { .. } => "PAD-BAD-PADBAR",
        }
    }
}

impl From<PadFault> for PadError {
    fn from(f: PadFault) -> Self {
        PadError::Fault(f)
    }
}

/// The `PADCFG0` register of `pad`.
pub fn padcfg0<M: Machine>(rig: &mut Rig<M>, pad: Pad) -> Result<PadReg, PadError> {
    let padbar = rig.pad_read(PadReg { community: pad.community, off: PADBAR })?;
    if padbar == 0 || padbar == u32::MAX {
        return Err(PadError::BadPadbar { value: padbar });
    }
    Ok(PadReg { community: pad.community, off: padbar + pad.pad as u32 * PAD_STRIDE })
}

/// Drive the speaker amplifiers' reset pad low and read it back. Never fails the caller: the outcome is a state, and the state is shown.
pub fn hold_speakers<M: Machine>(rig: &mut Rig<M>, pad: Pad) -> SpeakerHold {
    let reg = match padcfg0(rig, pad) {
        Ok(r) => r,
        Err(PadError::Fault(PadFault::NoWindow { .. })) => return SpeakerHold::NotGranted,
        Err(_) => return SpeakerHold::Failed,
    };
    // the one register the device may ever write in a pad window
    rig.pads.spkr_rst = Some(reg);
    let Ok(cur) = rig.pad_read(reg) else { return SpeakerHold::Failed };
    // GPIO mode, transmitter ON, level LOW; the receiver stays on so the pad can be read back
    let want = (cur & !(PADCFG0_PMODE_MASK | PADCFG0_GPIORXDIS | PADCFG0_GPIOTXDIS | PADCFG0_GPIOTXSTATE)) | PADCFG0_PMODE_GPIO;
    match rig.pad_write(reg, want) {
        Ok(()) => {}
        Err(PadFault::NoWindow { .. }) => return SpeakerHold::NotGranted,
        // a window that exists but refuses the write is a grant that is read-only: the same gap
        Err(PadFault::NotAllowed { .. }) if !rig.pads.permits_write(reg) => return SpeakerHold::Failed,
        Err(_) => return SpeakerHold::NotGranted,
    }
    match rig.pad_read(reg) {
        Ok(v) if v & PADCFG0_GPIOTXDIS == 0 && v & PADCFG0_GPIOTXSTATE == 0 && v & PADCFG0_PMODE_MASK == PADCFG0_PMODE_GPIO => {
            if v & PADCFG0_GPIORXDIS == 0 && v & PADCFG0_GPIORXSTATE != 0 {
                // the register says low but the pad reads high: the line is held up by something else
                SpeakerHold::Failed
            } else {
                SpeakerHold::Held
            }
        }
        Ok(_) => SpeakerHold::Failed,
        Err(_) => SpeakerHold::Unverified,
    }
}

/// Is the codec's interrupt line asserted (LOW) right now? `None` when the pad cannot be read.
pub fn jack_line_asserted<M: Machine>(rig: &mut Rig<M>, pad: Pad) -> Option<bool> {
    let reg = padcfg0(rig, pad).ok()?;
    let v = rig.pad_read(reg).ok()?;
    if v & PADCFG0_GPIORXDIS != 0 {
        return None;
    }
    Some(v & PADCFG0_GPIORXSTATE == 0)
}

/// Is the amplifiers' reset pad driven LOW right now? A read, no write: transmitter on, level low, GPIO mode, and (when the receiver is on)
/// the line reading low. `false` when the pad cannot be read.
pub fn speakers_held<M: Machine>(rig: &mut Rig<M>, pad: Pad) -> bool {
    let Ok(reg) = padcfg0(rig, pad) else { return false };
    let Ok(v) = rig.pad_read(reg) else { return false };
    v & PADCFG0_GPIOTXDIS == 0 && v & PADCFG0_GPIOTXSTATE == 0 && v & PADCFG0_PMODE_MASK == PADCFG0_PMODE_GPIO && (v & PADCFG0_GPIORXDIS != 0 || v & PADCFG0_GPIORXSTATE == 0)
}

/// Why the amplifiers' reset could not be released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReleaseError {
    /// The pad window or the write grant is not there (the device has never held the line, or the loader's grant is read-only).
    NotGranted,
    /// The write was refused or the readback is not "driven high".
    NotHigh,
}

/// Release the amplifiers' reset: drive the pad HIGH (the reset is active low) and read it back high. Only a speaker session calls this
/// ([`crate::speakers::Speakers::begin`], after the fence is armed and the line has been low for at least 50 ms). The one pad register the
/// device may write is the one [`hold_speakers`] resolved, so a pad this never held cannot be driven.
pub fn release_speakers<M: Machine>(rig: &mut Rig<M>, pad: Pad) -> Result<(), ReleaseError> {
    let reg = padcfg0(rig, pad).map_err(|_| ReleaseError::NotGranted)?;
    if rig.pads.spkr_rst != Some(reg) {
        return Err(ReleaseError::NotGranted);
    }
    let cur = rig.pad_read(reg).map_err(|_| ReleaseError::NotHigh)?;
    let want = (cur & !(PADCFG0_PMODE_MASK | PADCFG0_GPIORXDIS | PADCFG0_GPIOTXDIS)) | PADCFG0_PMODE_GPIO | PADCFG0_GPIOTXSTATE;
    rig.pad_write(reg, want).map_err(|_| ReleaseError::NotHigh)?;
    match rig.pad_read(reg) {
        Ok(v) if v & PADCFG0_GPIOTXDIS == 0 && v & PADCFG0_GPIOTXSTATE != 0 && v & PADCFG0_PMODE_MASK == PADCFG0_PMODE_GPIO && v & PADCFG0_GPIORXDIS == 0 && v & PADCFG0_GPIORXSTATE != 0 => Ok(()),
        _ => Err(ReleaseError::NotHigh),
    }
}
