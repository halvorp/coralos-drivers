// SPDX-License-Identifier: GPL-2.0-only
//! The interrupt side of the Skylake HIPC: `avs_skl_dsp_interrupt` and `avs_skl_ipc_interrupt` (skl.c:18-76) as a coroutine.
//! Straight-line, so it terminates in at most 14 operations whatever the caller answers.
//!
//! The caller performs each [`IsrOp`] and answers it; [`IsrOp::Signal`] hands a completed event to the transaction layer
//! ([`crate::txn`]) or to the code-loader DMA.
use crate::regs::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Signal {
    /// `hda_cldma_interrupt` (cldma.c:251-260): the code loader's transfer completed with this `SD_STS`.
    CldmaIoc { sd_status: u8 },
    /// "DSP acked host's request": `complete(&adev->ipc->done_completion)` (skl.c:31).
    TxDone,
    /// "DSP sent new response to process": `avs_dsp_process_response(adev, msg.val)` (skl.c:45).
    Response(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsrOp {
    Read32 { off: u32 },
    Read8 { off: u32 },
    /// `reg = (reg & !mask) | value`.
    Update32 { off: u32, mask: u32, value: u32 },
    Signal(Signal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsrInput {
    Begin,
    Value(u32),
    Done,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsrOutput {
    Do(IsrOp),
    /// `IRQ_HANDLED` (true) or `IRQ_NONE` (false).
    Finished { handled: bool },
    /// The caller answered something the last op did not ask for.
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Stage {
    Start,
    GotIs,
    CldmaIrqOff,
    CldmaStatus,
    CldmaSignalled,
    IpcMasked,
    GotAck,
    GotRsp,
    TxDoneSignalled,
    AckWritten,
    ReadPrimary,
    ReadExt,
    ResponseSignalled,
    BusyWritten,
    CtlRestored,
    Done,
}

/// The most operations one interrupt can ask for, whatever the hardware says: every branch of skl.c:18-76 taken.
pub const MAX_OPS: usize = 14;

/// One interrupt.
#[derive(Clone, Copy, Debug)]
pub struct SklIsr {
    stage: Stage,
    adspis: u32,
    ack: u32,
    rsp: u32,
    primary: u32,
    handled: bool,
}

impl Default for SklIsr {
    fn default() -> Self {
        Self::new()
    }
}

const CTL_BOTH: u32 = AVS_ADSP_HIPCCTL_DONE | AVS_ADSP_HIPCCTL_BUSY;

impl SklIsr {
    pub const fn new() -> Self {
        Self { stage: Stage::Start, adspis: 0, ack: 0, rsp: 0, primary: 0, handled: false }
    }

    fn ipc_or_finish(&mut self) -> IsrOutput {
        if self.adspis & AVS_ADSP_ADSPIS_IPC != 0 {
            self.handled = true;
            self.stage = Stage::IpcMasked;
            // skl.c:23-24: mask the DONE and BUSY interrupts while they are serviced
            return IsrOutput::Do(IsrOp::Update32 { off: SKL_CTL_OFFSET, mask: CTL_BOTH, value: 0 });
        }
        self.stage = Stage::Done;
        IsrOutput::Finished { handled: self.handled }
    }

    fn after_ack(&mut self) -> IsrOutput {
        // skl.c:39: `if (hipc_rsp & spec->hipc->rsp_busy_mask)`
        if self.rsp & SKL_RSP_BUSY_MASK != 0 {
            self.stage = Stage::ReadPrimary;
            return IsrOutput::Do(IsrOp::Read32 { off: SKL_ADSP_REG_HIPCT });
        }
        self.finish_ipc()
    }

    fn finish_ipc(&mut self) -> IsrOutput {
        // skl.c:52-54: unmask DONE and BUSY again
        self.stage = Stage::CtlRestored;
        IsrOutput::Do(IsrOp::Update32 { off: SKL_CTL_OFFSET, mask: CTL_BOTH, value: CTL_BOTH })
    }

    pub fn next(&mut self, input: IsrInput) -> IsrOutput {
        match (self.stage, input) {
            (Stage::Start, IsrInput::Begin) => {
                self.stage = Stage::GotIs;
                IsrOutput::Do(IsrOp::Read32 { off: AVS_ADSP_REG_ADSPIS })
            }
            (Stage::GotIs, IsrInput::Value(is)) => {
                self.adspis = is;
                if is == u32::MAX {
                    // skl.c:62-63: `if (adspis == UINT_MAX) return ret;` (IRQ_NONE: the device is gone or asleep)
                    self.stage = Stage::Done;
                    return IsrOutput::Finished { handled: false };
                }
                if is & AVS_ADSP_ADSPIS_CLDMA != 0 {
                    self.handled = true;
                    self.stage = Stage::CldmaIrqOff;
                    // cldma.c:254: "disable CLDMA interrupt"
                    return IsrOutput::Do(IsrOp::Update32 { off: AVS_ADSP_REG_ADSPIC, mask: AVS_ADSP_ADSPIC_CLDMA, value: 0 });
                }
                self.ipc_or_finish()
            }
            (Stage::CldmaIrqOff, IsrInput::Done) => {
                self.stage = Stage::CldmaStatus;
                IsrOutput::Do(IsrOp::Read8 { off: CL_SD_STS })
            }
            (Stage::CldmaStatus, IsrInput::Value(v)) => {
                self.stage = Stage::CldmaSignalled;
                IsrOutput::Do(IsrOp::Signal(Signal::CldmaIoc { sd_status: v as u8 }))
            }
            (Stage::CldmaSignalled, IsrInput::Done) => self.ipc_or_finish(),
            (Stage::IpcMasked, IsrInput::Done) => {
                self.stage = Stage::GotAck;
                IsrOutput::Do(IsrOp::Read32 { off: SKL_ACK_OFFSET })
            }
            (Stage::GotAck, IsrInput::Value(v)) => {
                self.ack = v;
                self.stage = Stage::GotRsp;
                IsrOutput::Do(IsrOp::Read32 { off: SKL_RSP_OFFSET })
            }
            (Stage::GotRsp, IsrInput::Value(v)) => {
                self.rsp = v;
                if self.ack & SKL_ACK_DONE_MASK != 0 {
                    // skl.c:30-36: the DSP acked the host's request
                    self.stage = Stage::TxDoneSignalled;
                    return IsrOutput::Do(IsrOp::Signal(Signal::TxDone));
                }
                self.after_ack()
            }
            (Stage::TxDoneSignalled, IsrInput::Done) => {
                // skl.c:33-35 "Tell DSP it has our attention."
                self.stage = Stage::AckWritten;
                IsrOutput::Do(IsrOp::Update32 { off: SKL_ACK_OFFSET, mask: SKL_ACK_DONE_MASK, value: SKL_ACK_DONE_MASK })
            }
            (Stage::AckWritten, IsrInput::Done) => self.after_ack(),
            (Stage::ReadPrimary, IsrInput::Value(v)) => {
                self.primary = v;
                self.stage = Stage::ReadExt;
                IsrOutput::Do(IsrOp::Read32 { off: SKL_ADSP_REG_HIPCTE })
            }
            (Stage::ReadExt, IsrInput::Value(e)) => {
                self.stage = Stage::ResponseSignalled;
                IsrOutput::Do(IsrOp::Signal(Signal::Response(((e as u64) << 32) | self.primary as u64)))
            }
            (Stage::ResponseSignalled, IsrInput::Done) => {
                // skl.c:48-49 "Tell DSP we accepted its message."
                self.stage = Stage::BusyWritten;
                IsrOutput::Do(IsrOp::Update32 { off: SKL_ADSP_REG_HIPCT, mask: SKL_ADSP_HIPCT_BUSY, value: SKL_ADSP_HIPCT_BUSY })
            }
            (Stage::BusyWritten, IsrInput::Done) => self.finish_ipc(),
            (Stage::CtlRestored, IsrInput::Done) => {
                self.stage = Stage::Done;
                IsrOutput::Finished { handled: self.handled }
            }
            _ => IsrOutput::Protocol,
        }
    }
}

/// `AZX_CL_SD_BASE + AZX_REG_SD_STS`: the code loader's `SD_STS` byte (cldma.c:15, hda_register.h:83).
const CL_SD_STS: u32 = 0x80 + 0x03; // cldma.c:15, hda_register.h:83
