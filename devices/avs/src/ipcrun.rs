// SPDX-License-Identifier: GPL-2.0-only
//! The IPC transaction executor: avs-ipc-core's [`Ipc`] machine performed over BAR4 and the interrupt pump.
//!
//! A request is: copy the payload into the downlink mailbox (the uplink copy of the REPLY is the pump's, done in the interrupt), write
//! the extension dword, write the primary dword with BUSY (that starts the DSP), then BLOCK on the interrupt until a response wakes the
//! sender or the budget passes. The machine bounds the waits (its own repeat count, one extra wait after a time-out with the DSP still
//! busy); this executor only supplies the deadline and the wake.
//!
//! The code loader needs the request SENT and the reply AWAITED as two steps (it moves the image in between): [`send`] drives to the
//! first wait and parks there, [`await_reply`] resumes.
use crate::machine::Machine;
use crate::pump::{Pump, PumpError};
use crate::rig::Rig;
use avs_fw_core::effect::{IpcCall, Reg, Space};
use avs_fw_core::programs;
use avs_fw_core::regs::AVS_MAIN_CORE_MASK;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::regs::{AVS_MAILBOX_SIZE, DOWNLINK_OFFSET};
use avs_ipc_core::request::{self, Request};
use avs_ipc_core::txn::{Completed, Input, Output};

/// Where a transaction stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drive {
    Done(Completed),
    /// Sent, and parked at its first wait.
    Parked(Output),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcFail {
    /// The transaction machine's own failure (a time-out, a channel that is not ready, ...).
    Ipc(IpcError),
    /// The interrupt pump failed under it (an exception, a storm, a refused register).
    Pump(PumpError),
}

impl IpcFail {
    pub const fn name(&self) -> &'static str {
        match self {
            IpcFail::Ipc(e) => e.name(),
            IpcFail::Pump(e) => e.name(),
        }
    }
}

impl From<PumpError> for IpcFail {
    fn from(e: PumpError) -> Self {
        IpcFail::Pump(e)
    }
}

impl From<crate::machine::RegFault> for IpcFail {
    fn from(e: crate::machine::RegFault) -> Self {
        IpcFail::Pump(PumpError::Reg(e))
    }
}

/// The downlink mailbox is written with 32-bit stores (`memcpy_toio` in 4-byte units); a last partial dword is zero padded.
fn copy_payload<M: Machine>(rig: &mut Rig<M>, bytes: &[u8]) -> Result<(), IpcFail> {
    if bytes.len() > AVS_MAILBOX_SIZE {
        return Err(IpcFail::Ipc(IpcError::PayloadTooBig { len: bytes.len(), max: AVS_MAILBOX_SIZE }));
    }
    for (i, c) in bytes.chunks(4).enumerate() {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        rig.write(Reg::dsp32(DOWNLINK_OFFSET + 4 * i as u32), u32::from_le_bytes(w))?;
    }
    Ok(())
}

/// Begin a transaction and drive it until it is done, or - with `park` - until it first waits for the reply.
pub fn begin<M: Machine>(rig: &mut Rig<M>, pump: &mut Pump, req: &Request<'_>, expected_reply: usize, park: bool) -> Result<Drive, IpcFail> {
    pump.pend.woke = 0; // `reinit_completion(&ipc->busy_completion)`
    pump.reply.clear();
    let first = pump.ipc.begin(req, expected_reply).map_err(IpcFail::Ipc)?;
    drive(rig, pump, req.payload.as_bytes(), first, park)
}

/// Resume a parked transaction and drive it to its end.
pub fn await_reply<M: Machine>(rig: &mut Rig<M>, pump: &mut Pump, parked: Output) -> Result<Completed, IpcFail> {
    match drive(rig, pump, &[], parked, false)? {
        Drive::Done(c) => Ok(c),
        Drive::Parked(_) => Err(IpcFail::Ipc(IpcError::Protocol)),
    }
}

fn drive<M: Machine>(rig: &mut Rig<M>, pump: &mut Pump, payload: &[u8], first: Output, park: bool) -> Result<Drive, IpcFail> {
    let mut out = first;
    // Bound on the machine's own steps: a transaction is a handful of writes plus at most 128 re-armed waits, two reads each way.
    for _ in 0..(8 + 4 * avs_ipc_core::regs::AVS_IPC_REPEATS as usize) {
        match out {
            Output::CopyPayload { len } => {
                copy_payload(rig, &payload[..len.min(payload.len())])?;
                out = pump.ipc.step(Input::Wrote);
            }
            Output::Write { off, value } => {
                rig.write(Reg::dsp32(off), value)?;
                out = pump.ipc.step(Input::Wrote);
            }
            Output::UnstallMainCore => {
                // ROM messages only (the host-DMA boot of other cAVS generations): release the main core's stall
                let ok = crate::fwexec::run_program(rig, &programs::core_stall(AVS_MAIN_CORE_MASK, false, avs_fw_core::effect::OnFail::Abort)).is_ok();
                out = pump.ipc.step(Input::Stalled(ok));
            }
            Output::WaitBusy { timeout_ms } => {
                if park && !pump.pend.exception.is_some() {
                    return Ok(Drive::Parked(out));
                }
                let deadline = rig.now().saturating_add(timeout_ms as u64);
                let woke = pump.wait_for(rig, deadline, |p| p.woke > 0)?;
                out = if woke {
                    pump.pend.woke -= 1;
                    pump.ipc.step(Input::Woken)
                } else {
                    pump.ipc.step(Input::TimedOut)
                };
            }
            Output::WaitTxDone { timeout_ms } => {
                let deadline = rig.now().saturating_add(timeout_ms as u64);
                let done = pump.wait_for(rig, deadline, |p| p.tx_done)?;
                pump.pend.tx_done = false;
                out = pump.ipc.step(if done { Input::Woken } else { Input::TimedOut });
            }
            Output::ReadRsp { off } => {
                let v = rig.read32(Space::Dsp, off)?;
                out = pump.ipc.step(Input::Value(v));
            }
            Output::Done(r) => return r.map(Drive::Done).map_err(IpcFail::Ipc),
        }
    }
    Err(IpcFail::Ipc(IpcError::Protocol))
}

/// The request a code loader's [`IpcCall`] stands for (`avs_ipc_load_library` with DMA id 0, loader.c:204; one module, loader.c:238).
pub fn call_request(call: IpcCall) -> Result<Request<'static>, IpcError> {
    match call {
        IpcCall::LoadLibrary { lib_id } => request::load_library(0, lib_id),
        IpcCall::LoadModule { module_id } => request::load_module(module_id),
    }
}
