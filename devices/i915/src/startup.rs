// SPDX-License-Identifier: GPL-2.0-only
//! The device's whole read-only bring-up as one function over two windows and a log, so a host test can run it end to end.
use crate::identity::{check, read_identity, PciIdentity, Refusal};
use crate::log::{emit, Log};
use crate::regio::{RegRead, BAR0_SLOT, READOUT_WINDOW_BYTES};
use crate::report;
use coralos_rt::mmio::MmioError;
use i915_kbl_display_core::platform::{KblDevice, SubPlatform};
use i915_kbl_display_core::readout::{read_display_state, ReadoutState};

/// A successful bring-up.
pub struct Ready {
    pub identity: PciIdentity,
    pub device: KblDevice,
    pub state: ReadoutState,
    pub active_pipes: u32,
    /// How many register reads the readout made (it makes no writes).
    pub reads: u32,
}

/// Why bring-up stopped, named.
#[derive(Debug, PartialEq, Eq)]
pub enum StartError {
    /// The function is not one this device drives.
    Refused(Refusal),
    /// BAR0 is missing or shorter than the display register block.
    Bar0(MmioError),
    /// A BAR0 read was refused mid-readout (offset first refused); the partial readout is discarded, not logged.
    ReadRefused { off: u32, err: MmioError },
}

impl StartError {
    pub fn name(&self) -> &'static str {
        match self {
            StartError::Refused(r) => r.name(),
            StartError::Bar0(e) => e.name(),
            StartError::ReadRefused { err, .. } => err.name(),
        }
    }
}

fn sub_name(s: SubPlatform) -> &'static str {
    match s {
        SubPlatform::None => "none",
        SubPlatform::Ult => "ult",
        SubPlatform::Ulx => "ulx",
    }
}

/// Run the identity check, then the readout, logging each stage as one named line.
pub fn startup<B: RegRead, C: RegRead, L: Log>(bar: &mut B, cfg: &mut C, log: &mut L) -> Result<Ready, StartError> {
    let identity = match read_identity(cfg) {
        Ok(i) => i,
        Err(r) => {
            emit(log, format_args!("[I915-KBL-DEV REFUSED step=CONFIG why={}]", r.name()));
            return Err(StartError::Refused(r));
        }
    };
    emit(log, format_args!(
        "[I915-KBL-DEV START vendor={:04x} device={:04x} rev={:02x} class={:02x}:{:02x} subsys={:04x}:{:04x}]",
        identity.vendor, identity.device, identity.revision, identity.base_class, identity.sub_class,
        identity.subsystem_vendor, identity.subsystem_device));
    let device = match check(&identity) {
        Ok(d) => d,
        Err(r) => {
            emit(log, format_args!("[I915-KBL-DEV REFUSED step=IDENTITY why={} vendor={:04x} device={:04x}]", r.name(), identity.vendor, identity.device));
            return Err(StartError::Refused(r));
        }
    };
    let caps = device.caps;
    emit(log, format_args!(
        "[I915-KBL-DEV IDENTIFIED platform=kabylake gt={} sub={} step={} display_ver={} pipes={} ports={} scalers={},{},{}]",
        device.id.gt, sub_name(device.id.sub), device.stepping.unwrap_or("?"), caps.display_ver, caps.num_pipes(),
        caps.port_mask.count_ones(), caps.scalers[0], caps.scalers[1], caps.scalers[2]));

    if bar.len() < READOUT_WINDOW_BYTES {
        let e = MmioError::TooShort { slot: BAR0_SLOT, len: bar.len(), need: READOUT_WINDOW_BYTES };
        emit(log, format_args!("[I915-KBL-DEV FAIL step=BAR0 why={} len={:#x} need={:#x}]", e.name(), bar.len(), READOUT_WINDOW_BYTES));
        return Err(StartError::Bar0(e));
    }
    emit(log, format_args!("[I915-KBL-DEV BAR0 len={:#x} access=read-only]", bar.len()));

    let mut reads = 0u32;
    let mut first_refusal: Option<(u32, MmioError)> = None;
    let state = {
        // After the first refusal the window is not touched again: the readout runs out on zeros and is discarded.
        let mut rd = |off: u32| {
            if first_refusal.is_some() {
                return 0;
            }
            match bar.read32(off) {
                Ok(v) => {
                    reads += 1;
                    v
                }
                Err(e) => {
                    first_refusal = Some((off, e));
                    0
                }
            }
        };
        read_display_state(&mut rd, &caps)
    };
    if let Some((off, err)) = first_refusal {
        emit(log, format_args!("[I915-KBL-DEV FAIL step=READOUT why={} off={:#x}]", err.name(), off));
        return Err(StartError::ReadRefused { off, err });
    }
    let active_pipes = report::all(log, &state);
    emit(log, format_args!("[I915-KBL-DEV READY pipes_active={} reads={} writes=0 refusals=0]", active_pipes, reads));
    Ok(Ready { identity, device, state, active_pipes, reads })
}
