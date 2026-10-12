// SPDX-License-Identifier: GPL-2.0-only
//! The stages of the boot and the named failures.
use crate::plan::PlanError;
use crate::volume::VolumeError;
use avs_fw_core::effect::Failure as FwFailure;
use avs_fw_core::error::FwError;
use avs_ipc_core::error::IpcError;
use hda_ext_core::error::HdaError;

/// Where in the sequence a failure happened. The order is the order of the boot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    GatingOff,
    BaseFirmware,
    ConfigQuery,
    Libraries,
    GatingOn,
    ModuleInfo,
    PathPlan,
    PathCreate,
    PathBind,
    HostStream,
    CodecGate,
    StreamStart,
    PathPause,
    PathRun,
    /// Restoring the PCI gating after a failure.
    Unwind,
}

impl Stage {
    pub const fn name(&self) -> &'static str {
        match self {
            Stage::GatingOff => "gating-off",
            Stage::BaseFirmware => "base-firmware",
            Stage::ConfigQuery => "config-query",
            Stage::Libraries => "libraries",
            Stage::GatingOn => "gating-on",
            Stage::ModuleInfo => "module-info",
            Stage::PathPlan => "path-plan",
            Stage::PathCreate => "path-create",
            Stage::PathBind => "path-bind",
            Stage::HostStream => "host-stream",
            Stage::CodecGate => "codec-gate",
            Stage::StreamStart => "stream-start",
            Stage::PathPause => "path-pause",
            Stage::PathRun => "path-run",
            Stage::Unwind => "unwind",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// A hardware step failed (poll bound, time-out, DMA error, refused library load).
    Fw(FwFailure),
    /// A firmware-loader planner refused its input.
    FwSetup(FwError),
    /// The IPC layer refused or failed a message.
    Ipc(IpcError),
    /// The firmware answered a request with a non-zero status (`AVS_IPC_RET`).
    IpcRefused { status: u32 },
    /// The reply payload could not be used (the TLV parse is the IPC layer's; this is the orchestrator's own check).
    BadReply { why: &'static str },
    /// The firmware reports a version older than the minimum the loader accepts (core.c:817).
    FwVersionTooOld { have: (u16, u16, u16, u16), need: (u16, u16, u16, u16) },
    /// The hardware config does not list the SSP port the playback path needs.
    SspPortMissing { port: u32, ctrl_count: u32 },
    /// The firmware reports no free pipeline / library slots for what the topology asks.
    FirmwareLimit { what: &'static str, have: u32, need: u32 },
    /// The number of library images supplied does not match the topology's library table.
    LibraryImagesMismatch { topology: usize, supplied: usize },
    /// The path could not be planned (before any message was sent).
    Plan(PlanError),
    /// The host stream's programs could not be built.
    Hda(HdaError),
    /// The codec was not proven safe, so no clock was started.
    CodecNotSafe,
    /// A pipeline's state, read back after a state change, was not the one requested.
    PipelineStateMismatch { ppl: u8, want: u32, got: u32 },
    /// A peakvol request above its ceiling.
    Volume(VolumeError),
    /// The caller answered something the last action did not ask for.
    Protocol,
    OutOfMemory { what: &'static str },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootFailure {
    pub stage: Stage,
    /// The step, message or check that failed: a stable name.
    pub step: &'static str,
    pub reason: Reason,
    /// The DSP was started and may hold state: the caller must power the core down
    /// ([`avs_fw_core::programs::core_disable`]) before trying again.
    pub dsp_dirty: bool,
    /// The PCI gating that the boot disabled was put back (or had never been disabled).
    pub gating_restored: bool,
}

impl BootFailure {
    pub const fn name(&self) -> &'static str {
        match self.reason {
            Reason::Fw(_) => "fw-step-failed",
            Reason::FwSetup(_) => "fw-setup-refused",
            Reason::Ipc(_) => "ipc-failed",
            Reason::IpcRefused { .. } => "ipc-refused",
            Reason::BadReply { .. } => "bad-reply",
            Reason::FwVersionTooOld { .. } => "fw-version-too-old",
            Reason::SspPortMissing { .. } => "ssp-port-missing",
            Reason::FirmwareLimit { .. } => "firmware-limit",
            Reason::LibraryImagesMismatch { .. } => "library-images-mismatch",
            Reason::Plan(_) => "plan-refused",
            Reason::Hda(_) => "hda-refused",
            Reason::CodecNotSafe => "codec-not-safe",
            Reason::PipelineStateMismatch { .. } => "pipeline-state-mismatch",
            Reason::Volume(_) => "volume-refused",
            Reason::Protocol => "protocol",
            Reason::OutOfMemory { .. } => "out-of-memory",
        }
    }
}
