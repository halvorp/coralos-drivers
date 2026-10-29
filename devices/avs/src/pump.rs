// SPDX-License-Identifier: GPL-2.0-only
//! The interrupt pump: what runs when the cAVS interrupt line fires, and the blocking waits the plans' `Wait` ops and the IPC transaction
//! machine are made of.
//!
//! One line carries three causes: the HD-audio stream's buffer-completion interrupts (BAR0 `INTSTS`), the DSP's IPC interrupt (a reply, a
//! notification, the DSP's ack) and the code loader's completion (both in BAR4 `ADSPIS`). The pump services all three, records what
//! happened in [`Pending`], and the waits read [`Pending`] - so a reply that arrives while the host is busy with a DMA transfer (a library
//! load sends its request, moves the image, THEN awaits the reply) is not lost.
//!
//! Every wait is a blocking [`Rig::wait`] on the interrupt with an absolute deadline: no spinning, and no wait without a bound.
//!
//! THE LINE IS SHARED (the cAVS function's INTx, GSI 16 on the Pixelbook Go, with every PCH function the firmware routes INTA to 16). A
//! firing is ours only if one of OUR causes is set - the stream's bit in BAR0 `INTSTS` with its `SD_STS`, or BAR4 `ADSPIS` - and only
//! those bits are written back (write-1-to-clear exactly what was read) BEFORE the line is re-armed ([`Rig::rearm_irq`]: the kernel
//! masked it on delivery, the re-arm is the unmask). A firing with none of our causes writes nothing and services nothing: it is another
//! function's. [`SPURIOUS_LIMIT`] of them in a row is another function holding the line asserted; the device then leaves the line
//! MASKED (no re-arm, no storm on this core) and polls its own causes for the rest of its life, named (`[AVS-DEV IRQ-STORM ..]`) -
//! Linux's `note_interrupt` "nobody cared" disables a line the same way, and audio keeps running.
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

/// Consecutive interrupts with no cause of ours before the shared line is left masked and the device polls. A bound on a retry, not a table size.
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
    /// The interrupt planner or the transaction machine was answered out of order (a bug in this crate).
    Protocol,
    Ipc(IpcError),
    OutOfMemory,
    /// The host stream reported a descriptor or FIFO error.
    StreamError { sts: u8 },
    /// The stream's registers read as all-ones: the function is not answering (powered down, or gone) - never a stream status to act on.
    StreamUnreadable,
}

impl PumpError {
    pub const fn name(&self) -> &'static str {
        match self {
            PumpError::Reg(e) => e.name(),
            PumpError::DspException(_) => "dsp-exception",
            PumpError::Protocol => "pump-protocol",
            PumpError::Ipc(e) => e.name(),
            PumpError::OutOfMemory => "out-of-memory",
            PumpError::StreamError { .. } => "stream-error",
            PumpError::StreamUnreadable => "stream-registers-unreadable",
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
    /// How many looks without a firing found a cause (poll mode; the last look of a wait).
    pub looked: u64,
}

impl Pump {
    /// A fresh pump for one DSP boot: not ready until the DSP says `FW_READY`.
    pub fn new(correlation: Correlation) -> Self {
        Self { ipc: Ipc::new(correlation), pend: Pending::default(), reply: Vec::new(), stream: None, spurious: 0, serviced: 0, looked: 0 }
    }

    fn hda_sts(&self, index: u32) -> Reg {
        Reg { space: Space::Hda, off: hda_ext_core::consts::SD_BASE + hda_ext_core::consts::SD_INTERVAL * index + AZX_REG_SD_STS, width: Width::B8 }
    }

    /// Service one firing of the line. Returns whether anything was handled.
    pub fn service_irq<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<bool, PumpError> {
        self.service(rig, true)
    }

    /// Look at the causes without the line having fired (poll mode, and the last look of a timed-out wait): the same service, but a look
    /// that finds nothing is not a spurious interrupt and is not counted towards [`SPURIOUS_LIMIT`].
    pub fn look<M: Machine>(&mut self, rig: &mut Rig<M>) -> Result<bool, PumpError> {
        let found = self.service(rig, false)?;
        if found {
            self.looked = self.looked.saturating_add(1);
        }
        Ok(found)
    }

    fn service<M: Machine>(&mut self, rig: &mut Rig<M>, fired: bool) -> Result<bool, PumpError> {
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
        } else if fired {
            self.spurious += 1;
            if self.spurious >= SPURIOUS_LIMIT && !rig.irq_poll {
                // not ours, again and again: another function holds the shared line asserted. Leave it masked and poll.
                rig.irq_poll = true;
                crate::log::emit(
                    &mut rig.m,
                    format_args!("[AVS-DEV IRQ-STORM spurious={} the shared line fired with no cause of ours (another function holds it asserted) -> masked, mode=poll]", self.spurious),
                );
                self.spurious = 0;
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
    ///
    /// POLL MODE ([`crate::irqmode`], `rig.irq_poll`): the interrupt cannot reach the task, so the causes are looked at every
    /// [`crate::irqmode::WAIT_POLL_MS`] instead - a bounded timed wait between looks, never a spin. In line mode a wait that reaches its
    /// deadline looks ONCE more: a cause found pending then is an interrupt that never arrived, which is named
    /// (`[AVS-DEV IRQ-LOST ..]`), counted (`rig.lost_irqs`), and switches the device to poll mode for the rest of its life.
    pub fn wait_for<M: Machine>(&mut self, rig: &mut Rig<M>, deadline_ms: u64, cond: impl Fn(&Pending) -> bool) -> Result<bool, PumpError> {
        loop {
            if let Some(e) = self.pend.exception {
                return Err(PumpError::DspException(e));
            }
            if cond(&self.pend) {
                return Ok(true);
            }
            let now = rig.now();
            if now >= deadline_ms {
                if rig.irq_poll {
                    return Ok(false);
                }
                let found = self.look(rig)?;
                if found && cond(&self.pend) {
                    rig.lost_irqs = rig.lost_irqs.saturating_add(1);
                    rig.irq_poll = true;
                    crate::log::emit(&mut rig.m, format_args!("[AVS-DEV IRQ-LOST cause pending at the deadline, no interrupt woke the wait lost={} -> mode=poll]", rig.lost_irqs));
                    return Ok(true);
                }
                return Ok(false);
            }
            if rig.irq_poll {
                rig.sleep_ms(crate::irqmode::WAIT_POLL_MS.min(deadline_ms - now));
                self.look(rig)?;
            } else if let Wake::AvsIrq = rig.wait(Some(deadline_ms), Want::IRQ_ONLY) {
                self.service_irq(rig)?;
                rig.rearm_irq();
            }
        }
    }
}
