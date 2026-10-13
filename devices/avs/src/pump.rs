// SPDX-License-Identifier: GPL-2.0-only
//! The interrupt pump: what runs when the cAVS interrupt line fires, and the blocking waits the plans' `Wait` ops and the IPC transaction
//! machine are made of.
//!
//! One line carries three causes: the HD-audio stream's buffer-completion interrupts (BAR0 `INTSTS`), the DSP's IPC interrupt (a reply, a
//! notification, the DSP's ack) and the code loader's completion (both in BAR4 `ADSPIS`). The pump services all three, records what
//! happened in [`Pending`], and the waits read [`Pending`] - so a reply that arrives while the host is busy with a DMA transfer (a library
//! load sends its request, moves the image, THEN awaits the reply) is not lost.
//!
//! Every wait is a blocking [`Rig::wait`] on the interrupt with an absolute deadline: no spinning, and no wait without a bound. A level
//! line that fires and has no cause is counted; [`SPURIOUS_LIMIT`] of them in a row is a named fault ([`PumpError::IrqStorm`]) rather than
//! a core burnt re-taking it.
use crate::machine::{Machine, RegFault, Want, Wake};
use crate::rig::Rig;
use alloc::vec::Vec;
use avs_fw_core::effect::{Reg, Space, Width};
use avs_fw_core::regs::{SD_INT_COMPLETE, SD_INT_DESC_ERR, SD_INT_FIFO_ERR, SD_INT_MASK};
use avs_ipc_core::error::IpcError;
use avs_ipc_core::isr::{IsrInput, IsrOp, IsrOutput, Signal, SklIsr, MAX_OPS};
use avs_ipc_core::notify::{Notify, NotifyKind};
use avs_ipc_core::regs::UPLINK_OFFSET;
use avs_ipc_core::txn::{Correlation, Event, Ipc, Outcome, Recovery};
use hda_ext_core::regs::{AZX_REG_INTSTS, AZX_REG_SD_STS};

/// Consecutive interrupts with no cause before the line is declared stuck. A bound on a retry, not a table size.
pub const SPURIOUS_LIMIT: u32 = 64;

/// What the DSP told us it crashed with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Exception {
    pub core_id: u32,
    pub stack_dump_size: u32,
    pub recovery_started: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpError {
    Reg(RegFault),
    /// The DSP reported an exception.
    DspException(Exception),
    /// The interrupt fired [`SPURIOUS_LIMIT`] times in a row with nothing to service.
    IrqStorm,
    /// The interrupt planner or the transaction machine was answered out of order (a bug in this crate).
    Protocol,
    Ipc(IpcError),
    OutOfMemory,
    /// The host stream reported a descriptor or FIFO error.
    StreamError { sts: u8 },
}

impl PumpError {
    pub const fn name(&self) -> &'static str {
        match self {
            PumpError::Reg(e) => e.name(),
            PumpError::DspException(_) => "dsp-exception",
            PumpError::IrqStorm => "irq-storm",
            PumpError::Protocol => "pump-protocol",
            PumpError::Ipc(e) => e.name(),
            PumpError::OutOfMemory => "out-of-memory",
            PumpError::StreamError { .. } => "stream-error",
        }
    }
}

impl From<RegFault> for PumpError {
    fn from(e: RegFault) -> Self {
        PumpError::Reg(e)
    }
}

/// What the interrupts have reported and nobody has consumed yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
    /// `FW_READY` arrived (stays set: the handshake happened).
    pub fw_ready: bool,
    /// The code loader completed a transfer with this `SD_STS`.
    pub cldma: Option<u8>,
    /// The DSP acked the host's request (ROM messages).
    pub tx_done: bool,
    /// Responses (replies and notifications) processed since the last transaction began: Linux's `busy_completion`.
    pub woke: u32,
    pub exception: Option<Exception>,
    /// Buffer-completion interrupts of the playback stream since taken.
    pub periods: u32,
    pub stream_error: Option<u8>,
    /// Replies that did not correlate with the request in flight / replies with none in flight (diagnostics).
    pub uncorrelated: u32,
    pub stale: u32,
}

pub struct Pump {
    pub ipc: Ipc,
    pub pend: Pending,
    /// The payload of the last reply, copied out of the uplink window by the interrupt.
    pub reply: Vec<u8>,
    /// The playback stream whose interrupt is serviced, once the host stream runs.
    pub stream: Option<u32>,
    spurious: u32,
    /// How many interrupts were serviced, for the status and the tests.
    pub serviced: u64,
}

impl Pump {
    /// A fresh pump for one DSP boot: not ready until the DSP says `FW_READY`.
    pub fn new(correlation: Correlation) -> Self {
        Self { ipc: Ipc::new(correlation), pend: Pending::default(), reply: Vec::new(), stream: None, spurious: 0, serviced: 0 }
    }

    fn hda_sts(&self, index: u32) -> Reg {
        Reg { space: Space::Hda, off: hda_ext_core::consts::SD_BASE + hda_ext_core::consts::SD_INTERVAL * index + AZX_REG_SD_STS, width: Width::B8 }
    }

    /// Service one firing of the line. Returns whether anything was handled.
    pub fn service_irq<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<bool, PumpError> {
        let mut handled = false;
        if let Some(index) = self.stream {
            let is = rig.read32(Space::Hda, AZX_REG_INTSTS)?;
            if is != u32::MAX && is & (1 << index) != 0 {
                let sts = rig.read(self.hda_sts(index))? as u8;
                if sts as u32 & SD_INT_MASK != 0 {
                    handled = true;
                    // write 1 to clear exactly the bits we saw
                    rig.write(self.hda_sts(index), sts as u32 & SD_INT_MASK)?;
                    if sts as u32 & SD_INT_COMPLETE != 0 {
                        self.pend.periods = self.pend.periods.saturating_add(1);
                    }
                    if sts as u32 & (SD_INT_DESC_ERR | SD_INT_FIFO_ERR) != 0 {
                        self.pend.stream_error = Some(sts & (SD_INT_DESC_ERR | SD_INT_FIFO_ERR) as u8);
                    }
                }
            }
        }
        if self.run_isr(rig)? {
            handled = true;
        }
        if handled {
            self.spurious = 0;
            self.serviced += 1;
        } else {
            self.spurious += 1;
            if self.spurious >= SPURIOUS_LIMIT {
                return Err(PumpError::IrqStorm);
            }
        }
        if let Some(e) = self.pend.exception {
            return Err(PumpError::DspException(e));
        }
        if let Some(sts) = self.pend.stream_error {
            return Err(PumpError::StreamError { sts });
        }
        Ok(handled)
    }

    /// `avs_skl_dsp_interrupt` as avs-ipc-core plans it, performed.
    fn run_isr<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<bool, PumpError> {
        let mut isr = SklIsr::new();
        let mut input = IsrInput::Begin;
        for _ in 0..=MAX_OPS {
            match isr.next(input) {
                IsrOutput::Finished { handled } => return Ok(handled),
                IsrOutput::Protocol => return Err(PumpError::Protocol),
                IsrOutput::Do(op) => {
                    input = match op {
                        IsrOp::Read32 { off } => IsrInput::Value(rig.read32(Space::Dsp, off)?),
                        IsrOp::Read8 { off } => IsrInput::Value(rig.read(Reg::dsp8(off))?),
                        IsrOp::Update32 { off, mask, value } => {
                            rig.update(Reg::dsp32(off), mask, value)?;
                            IsrInput::Done
                        }
                        IsrOp::Signal(s) => {
                            self.signal(rig, s)?;
                            IsrInput::Done
                        }
                    };
                }
            }
        }
        Err(PumpError::Protocol)
    }

    fn signal<M: Machine>(&mut self, rig: &mut Rig<M>, s: Signal) -> Result<(), PumpError> {
        match s {
            Signal::CldmaIoc { sd_status } => self.pend.cldma = Some(sd_status),
            Signal::TxDone => self.pend.tx_done = true,
            Signal::Response(header) => self.response(rig, header)?,
        }
        Ok(())
    }

    fn response<M: Machine>(&mut self, rig: &mut Rig<M>, header: u64) -> Result<(), PumpError> {
        let n = Notify::new(header);
        let module_size = if !avs_ipc_core::header::is_reply(header) && n.kind() == NotifyKind::ModuleEvent {
            // the third dword of the uplink payload header (ipc.c:233-236)
            rig.read32(Space::Dsp, UPLINK_OFFSET + 8)?
        } else {
            0
        };
        match self.ipc.on_response(header, module_size).map_err(PumpError::Ipc)? {
            Outcome::Reply { copy_len } => {
                self.reply.clear();
                self.reply.try_reserve_exact(copy_len).map_err(|_| PumpError::OutOfMemory)?;
                let mut off = 0;
                while off < copy_len {
                    let w = rig.read32(Space::Dsp, UPLINK_OFFSET + off as u32)?.to_le_bytes();
                    let take = (copy_len - off).min(4);
                    self.reply.extend_from_slice(&w[..take]);
                    off += 4;
                }
                self.pend.woke += 1;
            }
            Outcome::Uncorrelated { .. } => {
                self.pend.uncorrelated += 1;
                self.pend.woke += 1; // Linux wakes the sender; the transaction machine re-arms the wait
            }
            Outcome::StaleReply => self.pend.stale += 1,
            Outcome::IgnoredNotReady => {}
            Outcome::Notification { event, .. } => {
                match event {
                    Event::FwReady => self.pend.fw_ready = true,
                    Event::ExceptionCaught { recovery, core_id, stack_dump_size } => {
                        self.pend.exception = Some(Exception { core_id, stack_dump_size, recovery_started: recovery == Recovery::Started });
                    }
                    _ => {}
                }
                self.pend.woke += 1;
            }
        }
        Ok(())
    }

    /// Block until `cond` holds of what has been reported, or `deadline_ms` (absolute) passes. `Ok(true)` when it held. Only the cAVS
    /// interrupt wakes it; the client's requests and the jack stay queued for the main loop.
    pub fn wait_for<M: Machine>(&mut self, rig: &mut Rig<M>, deadline_ms: u64, cond: impl Fn(&Pending) -> bool) -> Result<bool, PumpError> {
        loop {
            if let Some(e) = self.pend.exception {
                return Err(PumpError::DspException(e));
            }
            if cond(&self.pend) {
                return Ok(true);
            }
            if rig.now() >= deadline_ms {
                return Ok(false);
            }
            if let Wake::AvsIrq = rig.wait(Some(deadline_ms), Want::IRQ_ONLY) {
                self.service_irq(rig)?;
                rig.m.rearm_avs_irq();
            }
        }
    }
}
