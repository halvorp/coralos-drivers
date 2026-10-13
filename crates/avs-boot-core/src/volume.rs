// SPDX-License-Identifier: GPL-2.0-only
//! The peakvol ceiling. The module's `target_volume` in the topology (`AVS_TKN_MODCFG_PEAKVOL_VOLUME_U32`, tokens.h) is the loudest the
//! path may ever be: Aura.engine owns the user volume BELOW it and this crate refuses anything above it by name.
//!
//! Volumes are the firmware's linear Q1.31 gain: `S32_MAX` (0x7fff_ffff) is unity.
use crate::msg::IpcMsg;
use alloc::vec::Vec;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::modcfg::{VolumeCfg, AVS_ALL_CHANNELS_MASK, AVS_AUDIO_CURVE_NONE, AVS_PEAKVOL_VOLUME};
use avs_ipc_core::request::{Flow, LargeSet, Payload, Request};

/// `S32_MAX`: unity gain, the largest representable volume (path.c:589).
pub const VOLUME_UNITY: u32 = 0x7fff_ffff; // path.c:589

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeError {
    /// The request is above the topology's ceiling; nothing is sent.
    AboveCeiling { requested: u32, ceiling: u32 },
    Ipc(IpcError),
}

/// A peakvol module of the running path and the ceiling its topology declared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeakvolHandle {
    pub module_id: u16,
    pub instance_id: u8,
    /// The topology's `target_volume`: never exceeded.
    pub ceiling: u32,
}

impl PeakvolHandle {
    /// `avs_ipc_peakvol_set_volume` (messages.c:702-707) for all channels, refusing a request above the ceiling. Returns one message per
    /// large-config block (a single volume is always one block).
    pub fn set_volume(&self, linear: u32) -> Result<Vec<IpcMsg>, VolumeError> {
        if linear > self.ceiling {
            return Err(VolumeError::AboveCeiling { requested: linear, ceiling: self.ceiling });
        }
        let cfg = VolumeCfg { channel_id: AVS_ALL_CHANNELS_MASK, target_volume: linear, curve_type: AVS_AUDIO_CURVE_NONE, curve_duration: 0 }.to_bytes();
        let set = LargeSet::new(self.module_id, self.instance_id, AVS_PEAKVOL_VOLUME, cfg.len()).map_err(VolumeError::Ipc)?;
        let mut out = Vec::new();
        for b in set {
            let b = b.map_err(VolumeError::Ipc)?;
            let r = Request { header: b.header, payload: Payload::Borrowed(&cfg[b.offset..b.offset + b.len]), timeout_ms: avs_ipc_core::regs::AVS_IPC_TIMEOUT_MS, flow: Flow::Msg, name: "peakvol set volume" };
            out.try_reserve(1).map_err(|_| VolumeError::Ipc(IpcError::OutOfMemory { what: "volume messages" }))?;
            out.push(IpcMsg::from_request(&r, 0).map_err(VolumeError::Ipc)?);
        }
        Ok(out)
    }
}
