// SPDX-License-Identifier: GPL-2.0-only
//! What a finished boot hands to the rest of the system, and the safe-stop messages for it.
use crate::msg::IpcMsg;
use crate::plan::{MsgOp, PathPlan, PipelineInfo, Role};
use crate::volume::PeakvolHandle;
use alloc::vec::Vec;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::{AVS_PPL_STATE_PAUSED, AVS_PPL_STATE_RESET};
use avs_ipc_core::payload::{FwCfg, HwCfg};
use hda_ext_core::stream::HostStream;

/// A headphone playback path that is RUNNING.
#[derive(Clone, Debug)]
pub struct Running {
    pub host_stream: HostStream,
    /// The front end's pipelines then the back end's, as created.
    pub pipelines: Vec<PipelineInfo>,
    /// The peakvol modules of the path with the ceilings their topology declared; EMPTY when the path has none (the da7219 topology has
    /// no peakvol, so the codec's own gain ceiling is the limit).
    pub peakvols: Vec<PeakvolHandle>,
    pub fw_cfg: FwCfg,
    pub hw_cfg: HwCfg,
    pub dma_id: u32,
}

impl Running {
    pub(crate) fn new(plan: &PathPlan, host: HostStream, fw: FwCfg, hw: HwCfg) -> Result<Self, IpcError> {
        let mut pipelines = Vec::new();
        pipelines.try_reserve_exact(plan.pipelines.len()).map_err(|_| IpcError::OutOfMemory { what: "running pipelines" })?;
        pipelines.extend(plan.pipelines.iter().filter(|p| p.role == Role::FrontEnd));
        pipelines.extend(plan.pipelines.iter().filter(|p| p.role == Role::BackEnd));
        let mut peakvols = Vec::new();
        peakvols.try_reserve_exact(plan.peakvols.len()).map_err(|_| IpcError::OutOfMemory { what: "running peakvols" })?;
        peakvols.extend(plan.peakvols.iter().copied());
        Ok(Self { host_stream: host, pipelines, peakvols, fw_cfg: fw, hw_cfg: hw, dma_id: plan.dma_id })
    }

    /// The messages that stop output, in the order Linux's trigger STOP runs them (pcm.c:895-908, :295-309): the front end's pipelines are
    /// PAUSED (last first), then the back end's (`avs_path_pause`, path.c:1510-1533). The `.device` stops the host stream between the two
    /// halves of the front end with hda-ext-core's `stream::stop`; this is the DSP half.
    pub fn pause_msgs(&self, role: Role) -> Result<Vec<IpcMsg>, IpcError> {
        self.state_msgs(role, AVS_PPL_STATE_PAUSED, true)
    }

    /// `avs_path_reset` (path.c:1451-1472): every pipeline of the path back to RESET, first to last.
    pub fn reset_msgs(&self, role: Role) -> Result<Vec<IpcMsg>, IpcError> {
        self.state_msgs(role, AVS_PPL_STATE_RESET, false)
    }

    fn state_msgs(&self, role: Role, state: u32, reverse: bool) -> Result<Vec<IpcMsg>, IpcError> {
        let ids = pipeline_order(&self.pipelines, role, false, reverse)?;
        let mut out = Vec::new();
        out.try_reserve_exact(ids.len()).map_err(|_| IpcError::OutOfMemory { what: "stop messages" })?;
        for ppl in ids {
            out.push(MsgOp::SetState { ppl, state }.to_msg()?);
        }
        Ok(out)
    }
}

/// The pipelines of one path in the order a state change visits them: `avs_path_pause` walks the list in REVERSE (path.c:1521),
/// `avs_path_reset` and `avs_path_run` forward (path.c:1460, :1588); `avs_path_run` skips a pipeline whose trigger is not AUTO
/// (path.c:1588-1589), which `only_auto` reproduces.
pub fn pipeline_order(pipelines: &[PipelineInfo], role: Role, only_auto: bool, reverse: bool) -> Result<Vec<u8>, IpcError> {
    let mut ids: Vec<u8> = Vec::new();
    ids.try_reserve_exact(pipelines.len()).map_err(|_| IpcError::OutOfMemory { what: "pipeline order" })?;
    ids.extend(pipelines.iter().filter(|p| p.role == role && (!only_auto || p.auto)).map(|p| p.ppl));
    if reverse {
        ids.reverse();
    }
    Ok(ids)
}
