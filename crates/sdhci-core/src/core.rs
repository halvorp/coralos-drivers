// SPDX-License-Identifier: GPL-2.0-or-later

//! Pure reducer implementing the Linux SDHCI recovery / completion ordering.
//!
//! See the crate-level documentation for the pinned source, scope, and
//! extension list.

use crate::regs::*;

/// Opcode for the MMC bus test read (used by sdhci.c:3478).
const MMC_BUS_TEST_R: u8 = 14;


/// Opaque token for asynchronous register reads and deadlines.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Token(pub u64);

/// Request identifier.
pub type RequestId = u32;

/// Data transfer direction.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    Read,
    Write,
}

/// Auto-CMD mode active for this request.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum AutoCmd {
    #[default]
    None,
    Cmd12,
    Cmd23,
}

/// SDHCI quirk bitsets from `sdhci.h`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Quirks {
    pub bits: u32,
    pub bits2: u32,
}

/// Host flags relevant to recovery.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct HostFlags {
    pub use_adma: bool,
    pub req_use_dma: bool,
    pub auto_cmd12: bool,
    pub auto_cmd23: bool,
}

/// Context for an SET_BLOCK_COUNT (CMD23) command.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct SbcCtx {
    pub arg: u32,
}

/// Context for a manual stop command (typically CMD12).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct StopCtx {
    pub opcode: u8,
    pub arg: u32,
    pub flags: u16,
    pub timeout_ms: u64,
}

/// Description of the request that is already in flight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestCtx {
    pub id: RequestId,
    pub has_data: bool,
    pub direction: Direction,
    pub multiblock: bool,
    pub stop: Option<StopCtx>,
    pub sbc: Option<SbcCtx>,
    pub auto_cmd: AutoCmd,
    pub busy: bool,
    pub response_present: bool,
    pub response_136: bool,
    pub cap_cmd_during_tfr: bool,
    pub opcode: u8,
    pub is_tuning: bool,
    pub quirks: Quirks,
    pub host_flags: HostFlags,
    pub cmd_timeout_ms: u64,
    pub data_timeout_ms: u64,
}

/// Errors surfaced by recovery.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Error {
    TimedOut,
    Crc,
    Eilseq,
    Eio,
    Enomedium,
    /// CORALOS EXTENSION: a software-reset bit never cleared within 100 ms.
    /// Linux logs and returns void; CoralOS reports and continues.
    ResetStuck { mask: u8 },
}

/// Purpose of an outstanding auxiliary register read.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ReadPurpose {
    AutoCmdStatus,
    CommandReg,
    HostControl2,
}

/// IRQ continuation context. Keeps the raw status, the ACK mask actually
/// written, the mutable dispatch mask, the loop bound, and any outstanding
/// auxiliary read separate from the `Recovery` request state.
#[derive(Clone, Debug)]
pub struct IrqCont {
    pub raw: u32,
    pub ack: u32,
    pub dispatch: u32,
    pub loop_count: u8,
    pub read: Option<ReadPurpose>,
    pub read_token: Option<Token>,
    pub cmd_done: bool,
    pub data_done: bool,
}

impl IrqCont {
    pub fn idle() -> Self {
        IrqCont {
            raw: 0,
            ack: 0,
            dispatch: 0,
            loop_count: 0,
            read: None,
            read_token: None,
            cmd_done: true,
            data_done: true,
        }
    }
}

/// In-flight command slot metadata.
#[derive(Clone, Debug)]
pub struct CmdCtx {
    pub id: RequestId,
    pub opcode: u8,
    pub is_tuning: bool,
    pub has_data: bool,
    pub busy: bool,
    pub response_present: bool,
    pub response_136: bool,
    pub is_stop: bool,
    pub is_sbc: bool,
    pub error: Option<Error>,
}

/// In-flight data transfer metadata.
#[derive(Clone, Debug)]
pub struct DataCtx {
    pub id: RequestId,
    pub direction: Direction,
    pub error: Option<Error>,
    pub stop: Option<StopCtx>,
    pub sbc: Option<SbcCtx>,
    pub auto_cmd: AutoCmd,
    pub multiblock: bool,
    pub cap_cmd_during_tfr: bool,
    pub host_flags: HostFlags,
    pub quirks: Quirks,
    pub sbc_error: Option<Error>,
}

/// Stop command deferred until after an error reset completes.
#[derive(Clone, Debug)]
pub struct DeferredStop {
    pub id: RequestId,
    pub stop: StopCtx,
    pub sw_data_timeout: bool,
}

/// Next reset to issue after the current one clears.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ResetNext {
    Data,
}

/// State of an in-progress software reset wait.
#[derive(Clone, Debug)]
pub struct ResetWait {
    pub mask: u8,
    pub deadline: Token,
    pub read: Token,
    pub next: Option<ResetNext>,
}

/// Reset reason used by `sdhci_reset_for_reason` (sdhci.c:274-295).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum ResetReason {
    RequestError,
    DataOnly,
}

/// Orthogonal recovery context. Command, data command, data, early data,
/// pending reset, deferred stop, done queue, reset wait, and IRQ continuation
/// are tracked independently, matching Linux's overlapping state.
#[derive(Clone, Debug)]
pub struct Recovery {
    pub req: Option<RequestCtx>,
    pub cmd: Option<CmdCtx>,
    pub data_cmd: Option<CmdCtx>,
    pub data: Option<DataCtx>,
    pub data_early: bool,
    pub pending_reset: bool,
    pub deferred_stop: Option<DeferredStop>,
    pub done: [Option<RequestId>; SDHCI_MAX_MRQS],
    pub done_err: [Option<Error>; SDHCI_MAX_MRQS],
    pub reset_wait: Option<ResetWait>,
    pub irq: IrqCont,
    pub pending_adma: bool,
    pub pending_finish: Option<RequestId>,
    pub reset_done: bool,
    /// Saved data error before `data.take()` in `finish_data()`.
    /// Consumed by `finish_mrq()`.  (sdhci.c:3494, 3276-3279)
    saved_data_error: Option<(RequestId, Error)>,
    next_tok: u64,
}

/// Actions emitted by the reducer. The executor performs bus/time side effects
/// and feeds `ReadComplete` / `DeadlineExpired` back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Write32 { reg: u16, value: u32 },
    Write16 { reg: u16, value: u16 },
    Write8 { reg: u16, value: u8 },
    Read32 { reg: u16, token: Token },
    Read16 { reg: u16, token: Token },
    Read8 { reg: u16, token: Token },
    ArmDeadline { token: Token, ms: u64 },
    DelayUs { us: u32 },
    SendStop { id: RequestId, stop: StopCtx },
    AdmaPost,
    AdmaWorkaround,
    ClockKick,
    CompleteRequest { id: RequestId, err: Option<Error> },
    ReportError { err: Error },
}

/// Events fed to the reducer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    InterruptStatus { raw: u32 },
    ReadComplete { token: Token, value: u32 },
    DeadlineExpired { token: Token },
    CommandTimeout { id: RequestId },
    DataTimeout { id: RequestId },
}

impl Recovery {
    /// Create recovery state for an already-issued request.
    pub fn new(req: RequestCtx) -> Self {
        // UNDER-SOURCED: `data_cmd = has_data || busy` depends on the full
        // `sdhci_data_line_cmd()` and command-issuance rules.  (sdhci.c:266-287)
        let cmd = CmdCtx {
            id: req.id,
            opcode: req.opcode,
            is_tuning: req.is_tuning,
            has_data: req.has_data,
            busy: req.busy,
            response_present: req.response_present,
            response_136: req.response_136,
            is_stop: false,
            is_sbc: false,
            error: None,
        };
        let data_cmd = if req.has_data || req.busy {
            Some(cmd.clone())
        } else {
            None
        };
        let data = if req.has_data {
            Some(DataCtx {
                id: req.id,
                direction: req.direction,
                error: None,
                stop: req.stop.clone(),
                sbc: req.sbc.clone(),
                auto_cmd: req.auto_cmd,
                multiblock: req.multiblock,
                cap_cmd_during_tfr: req.cap_cmd_during_tfr,
                host_flags: req.host_flags,
                quirks: req.quirks,
                sbc_error: None,
            })
        } else {
            None
        };
        Recovery {
            req: Some(req),
            cmd: Some(cmd),
            data_cmd,
            data,
            data_early: false,
            pending_reset: false,
            deferred_stop: None,
            done: [None; SDHCI_MAX_MRQS],
            done_err: [None; SDHCI_MAX_MRQS],
            reset_wait: None,
            irq: IrqCont::idle(),
            pending_adma: false,
            pending_finish: None,
            reset_done: false,
            saved_data_error: None,
            next_tok: 0,
        }
    }

    /// Drive the recovery state machine one step.
    pub fn step(&mut self, ev: Event, out: &mut dyn FnMut(Action)) {
        match ev {
            Event::InterruptStatus { raw } => self.on_interrupt(raw, out),
            Event::ReadComplete { token, value } => self.on_read_complete(token, value, out),
            Event::DeadlineExpired { token } => self.on_deadline(token, out),
            Event::CommandTimeout { id } => self.on_command_timeout(id, out),
            Event::DataTimeout { id } => self.on_data_timeout(id, out),
        }
    }

    fn next_token(&mut self) -> Token {
        let t = Token(self.next_tok);
        self.next_tok += 1;
        t
    }

    fn on_interrupt(&mut self, raw: u32, out: &mut dyn FnMut(Action)) {
        if raw == 0 || raw == 0xffffffff {
            return; // sdhci.c:3573
        }
        // UNDER-SOURCED: Linux ignores command/data interrupts conditionally
        // inside their handlers when `pending_reset` and no matching operation
        // exists (3321, 3459); the reducer uses a global "ACK and discard
        // everything" rule, especially while an auxiliary read is pending.
        if self.reset_wait.is_some() || self.pending_reset || self.irq.read.is_some() {
            let ack = raw & (SDHCI_INT_CMD_MASK | SDHCI_INT_DATA_MASK | SDHCI_INT_BUS_POWER);
            if ack != 0 {
                out(Action::Write32 {
                    reg: SDHCI_INT_STATUS,
                    value: ack,
                });
            }
            return;
        }

        let ack = raw & (SDHCI_INT_CMD_MASK | SDHCI_INT_DATA_MASK | SDHCI_INT_BUS_POWER);
        self.irq = IrqCont {
            raw,
            ack,
            dispatch: raw,
            loop_count: 16, // sdhci.c:3562
            read: None,
            read_token: None,
            cmd_done: false,
            data_done: false,
        };
        // ACK first, then dispatch (sdhci.c:3587-3590).
        out(Action::Write32 {
            reg: SDHCI_INT_STATUS,
            value: ack,
        });
        self.continue_irq(out);
    }

    fn continue_irq(&mut self, out: &mut dyn FnMut(Action)) {
        if self.irq.read.is_some() {
            return;
        }
        loop {
            if self.irq.loop_count == 0 {
                break;
            }
            // UNDER-SOURCED: Linux rereads `SDHCI_INT_STATUS` on every loop
            // (3654-3655).  The reducer clears `dispatch`, breaks, and never
            // emits that reread, despite carrying a 16-iteration counter.
            if !self.irq.cmd_done {
                let cmd_mask = self.irq.dispatch & SDHCI_INT_CMD_MASK;
                if cmd_mask != 0 {
                    self.cmd_irq(cmd_mask, out);
                    if self.irq.read.is_some() {
                        return;
                    }
                }
                self.irq.cmd_done = true;
            }
            if !self.irq.data_done {
                let data_mask = self.irq.dispatch & SDHCI_INT_DATA_MASK;
                if data_mask != 0 {
                    self.data_irq(data_mask, out);
                    if self.irq.read.is_some() {
                        return;
                    }
                }
                self.irq.data_done = true;
            }
            self.irq.loop_count -= 1;
            self.irq.cmd_done = false;
            self.irq.data_done = false;
            self.irq.dispatch = 0;
            self.maybe_request_done(out);
            break;
        }
    }

    fn on_read_complete(&mut self, token: Token, value: u32, out: &mut dyn FnMut(Action)) {
        if let Some(rw) = &self.reset_wait {
            if token == rw.read {
                self.on_reset_read(value, out);
                return;
            }
        }
        if self.irq.read_token == Some(token) {
            let purpose = match self.irq.read {
                Some(p) => p,
                None => return,
            };
            self.irq.read = None;
            self.irq.read_token = None;
            match purpose {
                ReadPurpose::AutoCmdStatus => self.on_auto_cmd_status(value, out),
                ReadPurpose::CommandReg => self.on_command_reg(value, out),
                ReadPurpose::HostControl2 => self.on_host_control2(value, out),
            }
            self.continue_irq(out);
        }
    }

    fn on_deadline(&mut self, token: Token, out: &mut dyn FnMut(Action)) {
        // Only handle reset deadlines; unmatched tokens are ignored.
        // Software timeouts are delivered via `CommandTimeout`/`DataTimeout`.
        if let Some(rw) = &self.reset_wait {
            if token == rw.deadline {
                let rw = rw.clone();
                self.reset_wait = None;
                // CORALOS EXTENSION: Linux logs and returns void (sdhci.c:226-231).
                out(Action::ReportError {
                    err: Error::ResetStuck { mask: rw.mask },
                });
                match rw.next {
                    Some(ResetNext::Data) => self.start_reset(SDHCI_RESET_DATA, None, out),
                    None => {
                        self.after_reset(out);
                        self.maybe_request_done(out);
                    }
                }
                return;
            }
        }
        // Ignore unmatched deadline tokens.
    }

    fn on_command_timeout(&mut self, id: RequestId, out: &mut dyn FnMut(Action)) {
        if let Some(c) = self.cmd.as_mut() {
            if c.id == id {
                c.error = Some(Error::TimedOut);
                self.finish_mrq(id, out);
                return;
            }
        }
        if let Some(c) = self.data_cmd.as_mut() {
            if c.id == id {
                c.error = Some(Error::TimedOut);
                self.finish_mrq(id, out);
                return;
            }
        }
    }

    fn on_data_timeout(&mut self, id: RequestId, out: &mut dyn FnMut(Action)) {
        if let Some(d) = self.data.as_mut() {
            if d.id == id {
                d.error = Some(Error::TimedOut);
                self.finish_data(true, out);
                return;
            }
        }
    }

    fn cmd_irq(&mut self, intmask: u32, out: &mut dyn FnMut(Action)) {
        // Auto-CMD12 error (sdhci.c:3301-3312).
        if intmask & SDHCI_INT_AUTO_CMD_ERR != 0 && self.data_cmd.is_some() {
            let is_auto12 = self
                .req
                .as_ref()
                .map(|r| r.sbc.is_none() && r.host_flags.auto_cmd12)
                .unwrap_or(false);
            if is_auto12 {
                if self.irq.read.is_none() {
                    let token = self.next_token();
                    self.irq.read = Some(ReadPurpose::AutoCmdStatus);
                    self.irq.read_token = Some(token);
                    out(Action::Read16 {
                        reg: SDHCI_AUTO_CMD_STATUS,
                        token,
                    });
                }
                return;
            }
        }

        if self.cmd.is_none() {
            // sdhci.c:3315-3328
            return;
        }

        if intmask & (SDHCI_INT_TIMEOUT | SDHCI_INT_CRC | SDHCI_INT_END_BIT | SDHCI_INT_INDEX) != 0 {
            let id = self.cmd.as_ref().unwrap().id;
            let has_data = self.cmd.as_ref().unwrap().has_data;
            let crc_only = (intmask & (SDHCI_INT_CRC | SDHCI_INT_TIMEOUT)) == SDHCI_INT_CRC;
            if intmask & SDHCI_INT_TIMEOUT != 0 {
                self.cmd.as_mut().unwrap().error = Some(Error::TimedOut);
            } else {
                self.cmd.as_mut().unwrap().error = Some(Error::Eilseq);
            }
            // Treat data command CRC error the same as data CRC error
            // (sdhci.c:3340-3347).
            if has_data && crc_only {
                self.cmd = None;
                self.irq.dispatch |= SDHCI_INT_DATA_CRC;
                return;
            }
            self.finish_mrq(id, out);
            return;
        }

        // Auto-CMD23 error (sdhci.c:3353-3367).
        if intmask & SDHCI_INT_AUTO_CMD_ERR != 0 {
            let is_auto23 = self
                .req
                .as_ref()
                .map(|r| r.sbc.is_some() && r.host_flags.auto_cmd23)
                .unwrap_or(false);
            if is_auto23 {
                if self.irq.read.is_none() {
                    let token = self.next_token();
                    self.irq.read = Some(ReadPurpose::AutoCmdStatus);
                    self.irq.read_token = Some(token);
                    out(Action::Read16 {
                        reg: SDHCI_AUTO_CMD_STATUS,
                        token,
                    });
                }
                return;
            }
        }

        if intmask & SDHCI_INT_RESPONSE != 0 {
            self.finish_command(out);
        }
    }

    fn on_auto_cmd_status(&mut self, value: u32, out: &mut dyn FnMut(Action)) {
        self.irq.dispatch &= !SDHCI_INT_AUTO_CMD_ERR;
        let status = value as u16;
        let data_err_bit = if status & SDHCI_AUTO_CMD_TIMEOUT != 0 {
            SDHCI_INT_DATA_TIMEOUT
        } else {
            SDHCI_INT_DATA_CRC
        };
        let req = match &self.req {
            Some(r) => r.clone(),
            None => return,
        };
        if self.data_cmd.is_some() && req.sbc.is_none() && req.host_flags.auto_cmd12 {
            self.irq.dispatch |= data_err_bit;
            return;
        }
        if self.cmd.is_some() && req.sbc.is_some() && req.host_flags.auto_cmd23 {
            let err = if status & SDHCI_AUTO_CMD_TIMEOUT != 0 {
                Error::TimedOut
            } else {
                Error::Eilseq
            };
            if let Some(d) = &mut self.data {
                d.sbc_error = Some(err);
            }
            let id = self.cmd.as_ref().unwrap().id;
            self.finish_mrq(id, out);
        }
    }

    fn data_irq(&mut self, intmask: u32, out: &mut dyn FnMut(Action)) {
        if self.data.is_none() {
            let data_cmd = self.data_cmd.clone();
            if let Some(dc) = data_cmd {
                if dc.busy {
                    // Busy-end / busy timeout on a data-line command
                    // (sdhci.c:3431-3450).
                    if intmask & SDHCI_INT_DATA_TIMEOUT != 0 {
                        if let Some(c) = self.data_cmd.as_mut() {
                            c.error = Some(Error::TimedOut);
                        }
                        let id = dc.id;
                        self.data_cmd = None;
                        self.finish_mrq(id, out);
                        return;
                    }
                    if intmask & SDHCI_INT_DATA_END != 0 {
                        self.data_cmd = None;
                        if self.cmd.as_ref().map(|c| c.id == dc.id).unwrap_or(false) {
                            return;
                        }
                        self.finish_mrq(dc.id, out);
                        return;
                    }
                }
            }
            return;
        }

        if intmask & SDHCI_INT_DATA_TIMEOUT != 0 {
            self.data.as_mut().unwrap().error = Some(Error::TimedOut);
        } else if intmask & SDHCI_INT_DATA_END_BIT != 0 {
            if self.irq.read.is_none() {
                let token = self.next_token();
                self.irq.read = Some(ReadPurpose::CommandReg);
                self.irq.read_token = Some(token);
                out(Action::Read16 {
                    reg: SDHCI_COMMAND,
                    token,
                });
                return;
            }
        } else if (intmask & (SDHCI_INT_DATA_CRC | SDHCI_INT_TUNING_ERROR)) != 0 {
            if self.irq.read.is_none() {
                let token = self.next_token();
                self.irq.read = Some(ReadPurpose::CommandReg);
                self.irq.read_token = Some(token);
                out(Action::Read16 {
                    reg: SDHCI_COMMAND,
                    token,
                });
                return;
            }
        } else if intmask & SDHCI_INT_ADMA_ERROR != 0 {
            self.data.as_mut().unwrap().error = Some(Error::Eio);
            // UNDER-SOURCED: sdhci_adma_show_error / ops->adma_workaround
            // (sdhci.c:3489-3496).
            out(Action::AdmaWorkaround);
        }

        if self.data.as_ref().map(|d| d.error.is_some()).unwrap_or(false) {
            self.finish_data(false, out);
        } else {
            if intmask & SDHCI_INT_DATA_END != 0 {
                let data_id = self.data.as_ref().unwrap().id;
                let cmd_is_data = self.cmd.as_ref().map(|c| c.id == data_id).unwrap_or(false)
                    && self.data_cmd.as_ref().map(|d| d.id == data_id).unwrap_or(false);
                if cmd_is_data {
                    self.data_early = true; // sdhci.c:3531-3538
                } else {
                    self.finish_data(false, out);
                }
            }
        }
    }

    fn on_command_reg(&mut self, value: u32, out: &mut dyn FnMut(Action)) {
        let opcode = SDHCI_GET_CMD(value as u16) as u8;
        let intmask = self.irq.dispatch;
        if intmask & SDHCI_INT_DATA_END_BIT != 0 {
            self.data.as_mut().unwrap().error = Some(Error::Eilseq);
            self.irq.dispatch &= !SDHCI_INT_DATA_END_BIT;
        } else if (intmask & (SDHCI_INT_DATA_CRC | SDHCI_INT_TUNING_ERROR)) != 0 {
            // UNDER-SOURCED: entire CRC/tuning branch, including the
            // HOST_CONTROL2 access, is gated by `opcode != MMC_BUS_TEST_R`
            // (sdhci.c:3477-3488).
            if opcode != MMC_BUS_TEST_R {
                self.data.as_mut().unwrap().error = Some(Error::Eilseq);
            }
            self.irq.dispatch &= !(SDHCI_INT_DATA_CRC | SDHCI_INT_TUNING_ERROR);
            if intmask & SDHCI_INT_TUNING_ERROR != 0 && opcode != MMC_BUS_TEST_R {
                let token = self.next_token();
                self.irq.read = Some(ReadPurpose::HostControl2);
                self.irq.read_token = Some(token);
                out(Action::Read16 {
                    reg: SDHCI_HOST_CONTROL2,
                    token,
                });
                return;
            }
        }
        if self.data.as_ref().map(|d| d.error.is_some()).unwrap_or(false) {
            self.finish_data(false, out);
        }
    }

    fn on_host_control2(&mut self, value: u32, out: &mut dyn FnMut(Action)) {
        let ctrl2 = (value as u16) & !SDHCI_CTRL_TUNED_CLK;
        out(Action::Write16 {
            reg: SDHCI_HOST_CONTROL2,
            value: ctrl2,
        });
        self.irq.dispatch &= !SDHCI_INT_TUNING_ERROR;
        if self.data.as_ref().map(|d| d.error.is_some()).unwrap_or(false) {
            self.finish_data(false, out);
        }
    }

    fn finish_command(&mut self, out: &mut dyn FnMut(Action)) {
        // UNDER-SOURCED: `response_present`, `response_136`, and
        // `cap_cmd_during_tfr` are stored but the response-register reads and
        // `mmc_command_done()` branch from 1824-1833 are absent.
        let cmd = match self.cmd.take() {
            Some(c) => c,
            None => return,
        };
        // Busy response: keep data_cmd until DATA_END (sdhci.c:1845-1852).
        if cmd.busy
            && self.data_cmd.as_ref().map(|d| d.id == cmd.id).unwrap_or(false)
            && self
                .req
                .as_ref()
                .map(|r| r.quirks.bits & SDHCI_QUIRK_NO_BUSY_IRQ == 0)
                .unwrap_or(true)
        {
            return;
        }
        if cmd.is_sbc {
            // UNDER-SOURCED: sdhci_send_command for the actual command after
            // CMD23 (sdhci.c:1855-1860). Not part of recovery of an
            // already-issued request.
        }
        if self.data.as_ref().map(|d| d.id == cmd.id).unwrap_or(false) && self.data_early {
            self.finish_data(false, out);
        }
        if !cmd.has_data {
            self.finish_mrq(cmd.id, out);
        }
    }

    fn finish_data(&mut self, sw_data_timeout: bool, out: &mut dyn FnMut(Action)) {
        // Save data.error before taking it, so finish_mrq can retrieve it.
        // (sdhci.c:3494, 3276-3279)
        if let Some(ref d) = self.data {
            if let Some(err) = d.error {
                self.saved_data_error = Some((d.id, err));
            }
        }
        let data = match self.data.take() {
            Some(d) => d,
            None => return,
        };
        let data_cmd_id = self.data_cmd.as_ref().map(|c| c.id);
        let cmd_is_data_cmd = match (&self.cmd, data_cmd_id) {
            (Some(c), Some(id)) => c.id == id,
            _ => false,
        };
        self.data_cmd = None;

        let mut reset_started = false;
        if data.error.is_some() {
            // __sdhci_finish_data_common (sdhci.c:1574-1585).
            if !self.cmd.is_some() || cmd_is_data_cmd {
                self.reset_for(ResetReason::RequestError, out);
            } else {
                self.reset_for(ResetReason::DataOnly, out);
            }
            reset_started = true;
        }

        let need_stop = data.stop.is_some()
            && ((!data.sbc.is_some() && !sdhci_auto_cmd12(&data)) || data.error.is_some());

        if reset_started {
            // Defer ADMA post and CMD12 until the reset completes
            // (sdhci.c:1587-1641).
            if data.host_flags.req_use_dma && data.host_flags.use_adma {
                self.pending_adma = true;
            }
            if need_stop && !data.cap_cmd_during_tfr {
                self.deferred_stop = Some(DeferredStop {
                    id: data.id,
                    stop: data.stop.unwrap(),
                    sw_data_timeout,
                });
            } else {
                self.pending_finish = Some(data.id);
            }
            return;
        }

        if data.host_flags.req_use_dma && data.host_flags.use_adma {
            out(Action::AdmaPost);
        }

        if need_stop {
            if data.cap_cmd_during_tfr {
                self.finish_mrq(data.id, out);
            } else {
                self.send_stop(data.id, data.stop.unwrap(), sw_data_timeout, out);
            }
        } else {
            self.finish_mrq(data.id, out);
        }
    }

    fn send_stop(&mut self, id: RequestId, mut stop: StopCtx, sw_data_timeout: bool, out: &mut dyn FnMut(Action)) {
        // UNDER-SOURCED: Successful CMD12 issuance after reset (741-749,
        // 798-805).  Linux checks inhibit state and may defer or fail the
        // command (1627-1641, 1668-1678).  The reducer unconditionally emits
        // `SendStop`.
        let busy = stop.opcode == 12
            && self
                .req
                .as_ref()
                .map(|r| r.quirks.bits2 & SDHCI_QUIRK2_STOP_WITH_TC != 0)
                .unwrap_or(false);

        if busy {
            stop.flags |= MMC_RSP_BUSY;
        }
        let stop_cmd = CmdCtx {
            id,
            opcode: stop.opcode,
            is_tuning: false,
            has_data: false,
            busy,
            response_present: true,
            response_136: false,
            is_stop: true,
            is_sbc: false,
            error: None,
        };
        self.cmd = Some(stop_cmd.clone());
        if busy {
            self.data_cmd = Some(stop_cmd);
        }
        out(Action::SendStop { id, stop });
        if sw_data_timeout {
            // UNDER-SOURCED: sdhci_send_command result for sw_data_timeout
            // (sdhci.c:1629-1636). The executor reports failure via a later
            // timeout/error event.
        }
    }

    fn after_reset(&mut self, out: &mut dyn FnMut(Action)) {
        if self.pending_adma {
            out(Action::AdmaPost);
            self.pending_adma = false;
        }
        if let Some(ds) = self.deferred_stop.take() {
            self.send_stop(ds.id, ds.stop, ds.sw_data_timeout, out);
            return;
        }
        if let Some(id) = self.pending_finish.take() {
            self.finish_mrq(id, out);
        }
    }

    fn reset_for(&mut self, reason: ResetReason, out: &mut dyn FnMut(Action)) {
        // UNDER-SOURCED: `sdhci_needs_reset` reproduction (917-960).
        // Linux's predicate is specifically command error, SBC error, stop
        // error, or RESET_AFTER_REQUEST (1501-1507).  The reducer substitutes
        // active `data.error`/`data_cmd.error` checks and does not retain a
        // real stop error object.
        let together = self
            .req
            .as_ref()
            .map(|r| r.quirks.bits2 & SDHCI_QUIRK2_ISSUE_CMD_DAT_RESET_TOGETHER != 0)
            .unwrap_or(false);
        if together {
            self.start_reset(SDHCI_RESET_CMD | SDHCI_RESET_DATA, None, out);
            return;
        }
        match reason {
            ResetReason::RequestError => {
                self.start_reset(SDHCI_RESET_CMD, Some(ResetNext::Data), out);
            }
            ResetReason::DataOnly => self.start_reset(SDHCI_RESET_DATA, None, out),
        }
    }

    fn start_reset(&mut self, mask: u8, next: Option<ResetNext>, out: &mut dyn FnMut(Action)) {
        self.reset_done = true;
        let deadline = self.next_token();
        let read = self.next_token();
        self.reset_wait = Some(ResetWait {
            mask,
            deadline,
            read,
            next,
        });
        out(Action::Write8 {
            reg: SDHCI_SOFTWARE_RESET,
            value: mask,
        });
        out(Action::ArmDeadline {
            token: deadline,
            ms: 100, // sdhci.c:217-218
        });
        out(Action::Read8 {
            reg: SDHCI_SOFTWARE_RESET,
            token: read,
        });
    }

    fn on_reset_read(&mut self, value: u32, out: &mut dyn FnMut(Action)) {
        let rw = match &self.reset_wait {
            Some(r) => r.clone(),
            None => return,
        };
        if value & (rw.mask as u32) == 0 {
            self.reset_wait = None;
            match rw.next {
                Some(ResetNext::Data) => self.start_reset(SDHCI_RESET_DATA, None, out),
                None => {
                    self.after_reset(out);
                    self.maybe_request_done(out);
                }
            }
        } else {
            out(Action::DelayUs { us: 10 }); // sdhci.c:233
            let token = self.next_token();
            if let Some(r) = self.reset_wait.as_mut() {
                r.read = token;
            }
            out(Action::Read8 {
                reg: SDHCI_SOFTWARE_RESET,
                token,
            });
        }
    }

    fn finish_mrq(&mut self, id: RequestId, out: &mut dyn FnMut(Action)) {
        // UNDER-SOURCED: Persistent MRQ error selection (881-915).  The
        // mapping from command/data/SBC/stop errors to `CompleteRequest.err`
        // is reducer-specific and currently omits already-detached data errors
        // and actual stop-command errors.  The saved_data_error slot addresses
        // the first omission.
        let mut err = None;
        if let Some(c) = &self.cmd {
            if c.id == id && c.error.is_some() {
                err = c.error;
            }
        }
        if err.is_none() {
            if let Some(c) = &self.data_cmd {
                if c.id == id && c.error.is_some() {
                    err = c.error;
                }
            }
        }
        if err.is_none() {
            if let Some(d) = &self.data {
                if d.id == id && d.error.is_some() {
                    err = d.error;
                }
            }
        }
        if err.is_none() {
            if let Some((saved_id, saved_err)) = self.saved_data_error {
                if saved_id == id {
                    err = Some(saved_err);
                    self.saved_data_error = None;
                }
            }
        }
        if err.is_none() {
            if let Some(d) = &self.data {
                if d.id == id && d.sbc_error.is_some() {
                    err = d.sbc_error;
                }
            }
        }
        if err.is_none() {
            if let Some(s) = &self.deferred_stop {
                if s.id == id && s.sw_data_timeout {
                    err = Some(Error::Eio);
                }
            }
        }

        let mut need_reset = false;
        if let Some(c) = &self.cmd {
            if c.id == id && c.error.is_some() {
                need_reset = true;
            }
        }
        if let Some(c) = &self.data_cmd {
            if c.id == id && c.error.is_some() {
                need_reset = true;
            }
        }
        if let Some(d) = &self.data {
            if d.id == id && d.error.is_some() {
                need_reset = true;
            }
        }
        if let Some((saved_id, _)) = self.saved_data_error {
            if saved_id == id {
                need_reset = true;
            }
        }
        if let Some(d) = &self.data {
            if d.id == id && d.sbc_error.is_some() {
                need_reset = true;
            }
        }
        if let Some(s) = &self.deferred_stop {
            if s.id == id && s.sw_data_timeout {
                need_reset = true;
            }
        }
        if let Some(r) = &self.req {
            if r.id == id && (r.quirks.bits & SDHCI_QUIRK_RESET_AFTER_REQUEST) != 0 {
                need_reset = true;
            }
        }

        if self.cmd.as_ref().map(|c| c.id == id).unwrap_or(false) {
            self.cmd = None;
        }
        if self.data_cmd.as_ref().map(|c| c.id == id).unwrap_or(false) {
            self.data_cmd = None;
        }
        if self.data.as_ref().map(|d| d.id == id).unwrap_or(false) {
            self.data = None;
        }
        if self.deferred_stop.as_ref().map(|s| s.id == id).unwrap_or(false) {
            self.deferred_stop = None;
        }
        if let Some((saved_id, _)) = self.saved_data_error {
            if saved_id == id {
                self.saved_data_error = None;
            }
        }
        self.pending_finish = None;

        if need_reset && !self.reset_done {
            self.pending_reset = true; // sdhci.c:1546-1547
        }

        for i in 0..SDHCI_MAX_MRQS {
            if self.done[i] == Some(id) {
                return;
            }
        }
        for i in 0..SDHCI_MAX_MRQS {
            if self.done[i].is_none() {
                self.done[i] = Some(id);
                self.done_err[i] = err;
                break;
            }
        }
        self.maybe_request_done(out);
    }

    fn maybe_request_done(&mut self, out: &mut dyn FnMut(Action)) {
        if self.reset_wait.is_some() {
            return;
        }
        let idx = match self.done.iter().position(|x| x.is_some()) {
            Some(i) => i,
            None => return,
        };
        let id = self.done[idx].unwrap();
        if self.pending_reset && (self.cmd.is_some() || self.data_cmd.is_some()) {
            return; // sdhci.c:3179-3182
        }
        // UNDER-SOURCED: Immediate versus deferred completion (977-1006).
        // Linux also considers `always_defer_done` and mapped DMA cookies
        // (3546-3553) and performs `sdhci_request_done_dma()` before request
        // completion (3194-3214).  The reducer's immediate `CompleteRequest`
        // abstraction needs an explicit scope restriction.
        if self.pending_reset {
            // CLOCK_BEFORE_RESET kick (sdhci.c:3184-3189).
            if self
                .req
                .as_ref()
                .map(|r| r.quirks.bits & SDHCI_QUIRK_CLOCK_BEFORE_RESET != 0)
                .unwrap_or(false)
            {
                out(Action::ClockKick);
            }
            self.reset_for(ResetReason::RequestError, out);
            self.pending_reset = false;
            return;
        }
        let err = self.done_err[idx].take();
        self.done[idx] = None;
        self.reset_done = false;
        out(Action::CompleteRequest { id, err });
    }
}

/// sdhci_auto_cmd12 (sdhci.c:1402-1407).
fn sdhci_auto_cmd12(data: &DataCtx) -> bool {
    data.sbc.is_none() && data.host_flags.auto_cmd12 && !data.cap_cmd_during_tfr
}