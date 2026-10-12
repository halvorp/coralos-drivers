// SPDX-License-Identifier: GPL-2.0-only
//! The request/reply state machine of `ipc.c`: `avs_dsp_do_send_msg`, `avs_ipc_wait_busy_completion`, `avs_dsp_send_tx`,
//! `avs_dsp_do_send_rom_msg`, `avs_dsp_receive_rx`, `avs_dsp_process_notification` and `avs_dsp_exception_caught`
//! (ipc.c:158-300, :308-440, :498-535), as a coroutine plus the interrupt-side entry point [`Ipc::on_response`].
//!
//! Linux serialises senders with `msg_mutex` and blocks on `wait_for_completion_*`. A planner cannot sleep, so a second sender
//! is REFUSED ([`IpcError::TransactionInFlight`]) and every wait is an [`Output`] the caller performs against its own clock and
//! answers with [`Input::Woken`] (the completion fired) or [`Input::TimedOut`].
//!
//! EVERY WAIT IS BOUNDED, and each bound is enforced here, not trusted to the caller:
//! * the first wait of a message carries the request's budget (300 ms, 5 s for code loading);
//! * after a time-out with the DSP still showing BUSY, Linux waits with `wait_for_completion_killable()` - FOREVER
//!   (ipc.c:337). This port waits once more, for the same budget, and then fails with [`IpcError::Timeout`];
//! * a wake-up that did not complete the reply (a notification's bottom half, ipc.c:340-350) re-arms the wait at most
//!   [`AVS_IPC_REPEATS`] (128) times;
//! * a ROM message waits once, for its budget.
//!
//! Beyond Linux: a reply must CORRELATE with the request in flight (same target and message type, the way the firmware echoes
//! them) or it is reported [`Outcome::Uncorrelated`] and not accepted; a reply with no request in flight is
//! [`Outcome::StaleReply`]. [`Correlation::Linux`] turns the check off (Linux accepts any reply as the answer).
use crate::error::IpcError;
use crate::header::*;
use crate::notify::{Notify, NotifyKind, Reply};
use crate::regs::*;
use crate::request::{Flow, Request};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Correlation {
    /// A reply must carry the request's target and message type.
    Strict,
    /// Any reply completes the request (what Linux does).
    Linux,
}

/// What the caller reports to [`Ipc::step`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Begin,
    /// The last [`Output::CopyPayload`] or [`Output::Write`] was performed.
    Wrote,
    /// `busy_completion` (or, for a ROM message, `done_completion`) fired.
    Woken,
    /// The caller's clock ran out on the wait.
    TimedOut,
    /// The value of the register an [`Output::ReadRsp`] asked for.
    Value(u32),
    /// Whether [`Output::UnstallMainCore`] succeeded.
    Stalled(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    /// `memcpy_toio(avs_downlink_addr(adev), payload, len)` (ipc.c:384-385).
    CopyPayload { len: usize },
    /// A 32-bit write to a BAR4 register.
    Write { off: u32, value: u32 },
    /// ROM messages are sent while the core is stalled; now release it (`avs_dsp_op(adev, stall, AVS_MAIN_CORE_MASK, false)`).
    UnstallMainCore,
    /// Wait for the reply / any response (`busy_completion`).
    WaitBusy { timeout_ms: u32 },
    /// Wait for the DSP's ACK (`done_completion`), ROM messages only.
    WaitTxDone { timeout_ms: u32 },
    /// Read `rsp_offset` to see whether the DSP is still BUSY (`avs_ipc_is_busy`, ipc.c:308-316).
    ReadRsp { off: u32 },
    Done(Result<Completed, IpcError>),
}

/// A finished transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completed {
    /// The reply header (`reply->header = ipc->rx.header`). 0 for a ROM message.
    pub header: u64,
    /// How many bytes of the reply payload the interrupt side copied ([`Outcome::Reply::copy_len`]).
    pub payload_len: usize,
}

impl Completed {
    pub const fn reply(&self) -> Reply {
        Reply::new(self.header)
    }
    /// `ret = ipc->rx.rsp.status` (ipc.c:421): 0 is success, a positive value the firmware's error.
    pub const fn status(&self) -> u32 {
        self.reply().status()
    }
}

/// `struct avs_ipc`'s recovery bookkeeping after an exception (ipc.c:158-183).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// `atomic_add_unless(&ipc->recovering, 1, 1)` took the flag: the caller starts the DSP reboot.
    Started,
    /// "dsp recovery is already in progress" (ipc.c:166-169).
    AlreadyInProgress,
}

/// What an incoming response was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The reply to the request in flight. Copy `copy_len` payload bytes from the uplink window into the reply buffer, then
    /// feed [`Input::Woken`] (Linux's `complete(&ipc->busy_completion)`).
    Reply { copy_len: usize },
    /// A reply that does not match the request in flight; it was NOT taken as the answer. Wake the waiter anyway.
    Uncorrelated { expected_type: u32, got_type: u32, expected_target: u32, got_target: u32 },
    /// A reply with nothing in flight.
    StaleReply,
    /// A notification. Read `payload_len` bytes of payload (0: none) and act on `event`; then wake the waiter.
    Notification { payload_len: usize, event: Event },
    /// "FW not ready, skip notification" (ipc.c:218-222): everything but FW_READY before the handshake.
    IgnoredNotReady,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `complete(&adev->fw_ready)` (ipc.c:262-265): the channel is now ready.
    FwReady,
    ExceptionCaught { recovery: Recovery, core_id: u32, stack_dump_size: u32 },
    LogBufferStatus { core: u32 },
    PhraseDetected,
    ResourceEvent,
    ModuleEvent,
    Unknown(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
    Idle,
    /// The payload copy was asked for.
    Copy,
    /// The extension dword write was asked for; the primary dword is next.
    ExtWritten,
    /// The primary dword write (which starts the DSP) was asked for.
    PrimaryWritten,
    /// The un-stall was asked for (ROM messages only).
    Unstall,
    AwaitBusy { second: bool },
    AwaitRsp,
    AwaitTxDone,
}

#[derive(Clone, Copy, Debug)]
pub struct Ipc {
    ready: bool,
    recovering: bool,
    recovery_requested: bool,
    correlation: Correlation,
    st: St,
    flow: Flow,
    header: u64,
    payload_len: usize,
    timeout_ms: u32,
    expected: usize,
    rx_completed: bool,
    rx_header: u64,
    rx_copy_len: usize,
    repeats_left: u32,
}

impl Ipc {
    /// `avs_ipc_init` (ipc.c:558-575): not ready until the DSP says FW_READY.
    pub const fn new(correlation: Correlation) -> Self {
        Self {
            ready: false,
            recovering: false,
            recovery_requested: false,
            correlation,
            st: St::Idle,
            flow: Flow::Msg,
            header: 0,
            payload_len: 0,
            timeout_ms: AVS_IPC_TIMEOUT_MS,
            expected: 0,
            rx_completed: false,
            rx_header: 0,
            rx_copy_len: 0,
            repeats_left: 0,
        }
    }

    pub const fn ready(&self) -> bool {
        self.ready
    }
    pub const fn in_flight(&self) -> bool {
        !matches!(self.st, St::Idle)
    }
    pub const fn recovering(&self) -> bool {
        self.recovering
    }
    /// True once after an exception started a recovery; clears on read.
    pub fn take_recovery_request(&mut self) -> bool {
        let r = self.recovery_requested;
        self.recovery_requested = false;
        r
    }
    /// `avs_ipc_block` (ipc.c:577-583): the channel is closed until the next FW_READY.
    pub fn block(&mut self) {
        self.ready = false;
        self.st = St::Idle;
    }
    /// The end of `avs_dsp_recovery`: `atomic_set(&adev->ipc->recovering, 0)` (ipc.c:145).
    pub fn recovery_finished(&mut self) {
        self.recovering = false;
    }

    /// `avs_dsp_exception_caught` (ipc.c:158-183) as state: "Account for the double-exception case" by clearing `ready`, then
    /// take the single recovery slot.
    pub fn exception_caught(&mut self) -> Recovery {
        self.ready = false;
        if self.recovering {
            return Recovery::AlreadyInProgress;
        }
        self.recovering = true;
        self.recovery_requested = true;
        Recovery::Started
    }

    /// Start sending `req`. `expected_reply` is how many reply-payload bytes the sender reserved (0 for none; the mailbox size
    /// for LARGE_CONFIG_GET). Returns the first thing the caller must do.
    pub fn begin(&mut self, req: &Request<'_>, expected_reply: usize) -> Result<Output, IpcError> {
        if self.in_flight() {
            return Err(IpcError::TransactionInFlight);
        }
        let n = req.payload.as_bytes().len();
        if n > AVS_MAILBOX_SIZE {
            return Err(IpcError::PayloadTooBig { len: n, max: AVS_MAILBOX_SIZE });
        }
        match req.flow {
            Flow::Msg => {
                if !self.ready {
                    return Err(IpcError::NotReady); // ipc.c:397
                }
            }
            Flow::Rom => {
                // ipc.c:507-512: "with hw still stalled, memory windows may not be configured properly so avoid accessing SRAM"
                if n != 0 {
                    return Err(IpcError::PayloadTooBig { len: n, max: 0 });
                }
            }
        }
        // avs_ipc_msg_init (ipc.c:359-369)
        self.flow = req.flow;
        self.header = req.wire_header();
        self.payload_len = n;
        self.timeout_ms = req.timeout_ms;
        self.expected = expected_reply;
        self.rx_completed = false;
        self.rx_header = 0;
        self.rx_copy_len = 0;
        self.repeats_left = AVS_IPC_REPEATS;
        if n > 0 {
            self.st = St::Copy;
            return Ok(Output::CopyPayload { len: n });
        }
        Ok(self.write_ext())
    }

    fn write_ext(&mut self) -> Output {
        self.st = St::ExtWritten;
        Output::Write { off: SKL_REQ_EXT_OFFSET, value: ext(self.header) } // ipc.c:386
    }

    fn done(&mut self, r: Result<Completed, IpcError>) -> Output {
        self.st = St::Idle;
        Output::Done(r)
    }

    fn timeout_error(&mut self) -> Output {
        // ipc.c:409-413: a time-out is treated as an exception, "just stack_dump=0"
        self.exception_caught();
        self.done(Err(IpcError::Timeout { exception: true }))
    }

    fn after_wake(&mut self) -> Output {
        // ipc.c:327-328: "DSP could be unresponsive at this point."
        if !self.ready {
            return self.done(Err(IpcError::NotReady));
        }
        if self.rx_completed {
            let c = Completed { header: self.rx_header, payload_len: self.rx_copy_len };
            return self.done(Ok(c));
        }
        // ipc.c:340-350: "Ongoing notification's bottom-half may cause early wakeup" - "Reply delayed due to notification"
        if self.repeats_left > 0 {
            self.repeats_left -= 1;
            self.st = St::AwaitBusy { second: false };
            return Output::WaitBusy { timeout_ms: self.timeout_ms };
        }
        self.timeout_error()
    }

    pub fn step(&mut self, input: Input) -> Output {
        match (self.st, input) {
            (St::Copy, Input::Wrote) => self.write_ext(),
            (St::ExtWritten, Input::Wrote) => {
                self.st = St::PrimaryWritten;
                // ipc.c:387: the primary dword is written LAST: it carries BUSY, which is what starts the DSP.
                Output::Write { off: SKL_REQ_OFFSET, value: primary(self.header) }
            }
            (St::PrimaryWritten, Input::Wrote) => match self.flow {
                Flow::Msg => {
                    self.st = St::AwaitBusy { second: false };
                    Output::WaitBusy { timeout_ms: self.timeout_ms }
                }
                Flow::Rom => {
                    self.st = St::Unstall;
                    Output::UnstallMainCore
                }
            },
            (St::Unstall, Input::Stalled(true)) => {
                self.st = St::AwaitTxDone;
                Output::WaitTxDone { timeout_ms: self.timeout_ms }
            }
            (St::Unstall, Input::Stalled(false)) => self.done(Err(IpcError::UnstallFailed)),
            (St::AwaitTxDone, Input::Woken) => self.done(Ok(Completed { header: 0, payload_len: 0 })),
            (St::AwaitTxDone, Input::TimedOut) => self.done(Err(IpcError::Timeout { exception: false })), // ipc.c:519
            (St::AwaitBusy { .. }, Input::Woken) => self.after_wake(),
            (St::AwaitBusy { second }, Input::TimedOut) => {
                if !self.ready {
                    return self.done(Err(IpcError::NotReady)); // ipc.c:327 precedes the `if (!ret)`
                }
                if second {
                    return self.timeout_error(); // Linux would wait here forever (ipc.c:337)
                }
                self.st = St::AwaitRsp;
                Output::ReadRsp { off: SKL_RSP_OFFSET } // ipc.c:331
            }
            (St::AwaitRsp, Input::Value(v)) => {
                if v & SKL_RSP_BUSY_MASK == 0 {
                    return self.timeout_error(); // ipc.c:331-332
                }
                // "Firmware did its job, either notification or reply has been received - now wait until it's processed."
                self.st = St::AwaitBusy { second: true };
                Output::WaitBusy { timeout_ms: self.timeout_ms }
            }
            _ => {
                if self.st == St::Idle {
                    return Output::Done(Err(IpcError::Protocol));
                }
                self.done(Err(IpcError::Protocol))
            }
        }
    }

    /// `avs_dsp_process_response` (ipc.c:287-306), the interrupt-side half. `module_event_data_size` is the third dword of the
    /// uplink payload header and is only read for MODULE_EVENT notifications (ipc.c:233-236); pass 0 otherwise.
    ///
    /// After handling the outcome the CALLER must wake the waiter ([`Input::Woken`]) - Linux completes `busy_completion` for
    /// notifications too, and the transaction loop depends on that.
    pub fn on_response(&mut self, header: u64, module_event_data_size: u32) -> Result<Outcome, IpcError> {
        if is_reply(header) {
            return Ok(self.on_reply(header));
        }
        self.on_notification(header, module_event_data_size)
    }

    fn on_reply(&mut self, header: u64) -> Outcome {
        if !matches!(self.st, St::AwaitBusy { .. } | St::AwaitRsp) {
            return Outcome::StaleReply;
        }
        let r = Reply::new(header);
        let want_type = MSG_TYPE.get(self.header);
        let want_target = MSG_TARGET.get(self.header);
        if self.correlation == Correlation::Strict && (r.msg_type() != want_type || r.target() != want_target) {
            return Outcome::Uncorrelated { expected_type: want_type, got_type: r.msg_type(), expected_target: want_target, got_target: r.target() };
        }
        let copy_len = r.payload_len(self.expected);
        self.rx_header = header;
        self.rx_copy_len = copy_len;
        self.rx_completed = true;
        Outcome::Reply { copy_len }
    }

    fn on_notification(&mut self, header: u64, module_event_data_size: u32) -> Result<Outcome, IpcError> {
        let n = Notify::new(header);
        // ipc.c:218-222
        if !self.ready && n.kind() != NotifyKind::FwReady {
            return Ok(Outcome::IgnoredNotReady);
        }
        let payload_len = n.payload_size(module_event_data_size)?;
        let event = match n.kind() {
            NotifyKind::FwReady => {
                self.ready = true; // ipc.c:262-265
                Event::FwReady
            }
            NotifyKind::ExceptionCaught => {
                let recovery = self.exception_caught();
                Event::ExceptionCaught { recovery, core_id: n.coredump_core_id(), stack_dump_size: n.coredump_stack_size() }
            }
            NotifyKind::LogBufferStatus => Event::LogBufferStatus { core: n.log_core() },
            NotifyKind::PhraseDetected => Event::PhraseDetected,
            NotifyKind::ResourceEvent => Event::ResourceEvent,
            NotifyKind::ModuleEvent => Event::ModuleEvent,
            NotifyKind::Unknown(t) => Event::Unknown(t),
        };
        Ok(Outcome::Notification { payload_len, event })
    }
}
