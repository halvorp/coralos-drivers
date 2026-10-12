// SPDX-License-Identifier: GPL-2.0-only
//! The vocabulary the planners speak. REGISTER FRAME: an [`Op`] names its register by [`Space`] and an offset in THAT space - BAR4
//! (`Dsp`), BAR0 (`Hda`) or PCI configuration (`PciConfig`) - and never by an absolute address. Nothing here touches a register: a planner answers with an [`Op`] and the caller (the
//! `.device`) performs it against a real BAR, PCI function or memory, then reports back with an [`Input`].

/// Which window an offset lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    /// BAR4, the DSP and its SRAM windows (`adev->dsp_ba`).
    Dsp,
    /// BAR0, the HDA controller registers (`VS_EM2`).
    Hda,
    /// PCI configuration space (`PGCTL`, `CGCTL`).
    PciConfig,
}

/// Access width. Linux mixes `readb`/`writeb` on `SD_CTL`/`SD_STS`/`SD_LVI` with 32-bit accesses on the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    B8,
    D32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reg {
    pub space: Space,
    pub off: u32,
    pub width: Width,
}

impl Reg {
    pub const fn dsp32(off: u32) -> Self {
        Self { space: Space::Dsp, off, width: Width::D32 }
    }
    pub const fn dsp8(off: u32) -> Self {
        Self { space: Space::Dsp, off, width: Width::B8 }
    }
}

/// A bounded wait: read `reg` until `(value & mask) == want`. The caller reads ONCE per question and answers
/// [`Input::Value`]; it sleeps `interval_us` between questions and answers [`Input::PollTimedOut`] when `timeout_us` of its
/// own clock ran out. Independently of that clock, the planner stops after `max_polls` answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PollSpec {
    pub reg: Reg,
    pub mask: u32,
    pub want: u32,
    pub interval_us: u32,
    pub timeout_us: u32,
    pub max_polls: u32,
}

impl PollSpec {
    /// `readl_poll_timeout(..., delay_us, timeout_us)` reads about `timeout_us / delay_us + 1` times; two more are allowed.
    pub const fn new(reg: Reg, mask: u32, want: u32, interval_us: u32, timeout_us: u32) -> Self {
        let per = if interval_us == 0 { 1 } else { interval_us };
        Self { reg, mask, want, interval_us, timeout_us, max_polls: timeout_us / per + 2 }
    }
    pub const fn satisfied(&self, value: u32) -> bool {
        value & self.mask == self.want
    }
}

/// Something the caller must wait for, signalled from its interrupt path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `adev->fw_ready` completed by the FW_READY notification (loader.c:589-598).
    FwReady,
    /// `cl->completion` completed by `hda_cldma_interrupt` (cldma.c `cldma_memcpy_work`).
    CldmaIoc,
}

/// An IPC the loader sends; the encoding lives in avs-ipc-core, not here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcCall {
    /// `avs_ipc_load_library(adev, 0, lib_id)` (loader.c:204).
    LoadLibrary { lib_id: u32 },
    /// `avs_ipc_load_modules(adev, &module_id, 1)` (loader.c:238).
    LoadModule { module_id: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `reg = (reg & !mask) | value`.
    Update { reg: Reg, mask: u32, value: u32 },
    Write { reg: Reg, value: u32 },
    /// Read a register and report it with [`Input::Value`] (the SPIB position).
    Read { reg: Reg },
    Poll(PollSpec),
    DelayUs(u32),
    Wait { event: Event, timeout_ms: u32 },
    /// Copy `len` bytes of the (stripped) firmware image, from `src_offset`, into the CLDMA data buffer at `dst_offset`.
    CopyToDma { dst_offset: u32, src_offset: u32, len: u32 },
    /// Put the IPC request on the wire and return at once: the DSP's answer is awaited by [`Op::IpcAwait`].
    IpcSend(IpcCall),
    /// Wait for the reply of the request sent by [`Op::IpcSend`]; answered with [`Input::IpcStatus`].
    IpcAwait { timeout_ms: u32 },
}

/// What the planner does when a step fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnFail {
    /// Linux ignores the result (`hda_cldma_stop(cl)` after the status poll, loader.c:183).
    Tolerate,
    /// Linux returns the error with nothing undone (loader.c:144-162).
    Abort,
    /// `avs_dsp_core_disable(adev, AVS_MAIN_CORE_MASK)` then the error (loader.c:173, :595).
    AbortDisable,
    /// `hda_cldma_stop` then `avs_dsp_core_disable` then the error (loader.c:183-188).
    AbortStopDisable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// A stable name for logs and for failure reports ("rom-init", "basefw-entered", ...).
    pub label: &'static str,
    pub op: Op,
    pub on_fail: OnFail,
}

impl Step {
    pub const NOP: Step = Step { label: "nop", op: Op::DelayUs(0), on_fail: OnFail::Tolerate };
    pub const fn new(label: &'static str, op: Op, on_fail: OnFail) -> Self {
        Self { label, op, on_fail }
    }
}

/// What the caller reports for the last [`Output::Do`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    Begin,
    /// A write, update, delay, copy or send was performed.
    Done,
    /// The value of the register an [`Op::Read`] or one question of an [`Op::Poll`] asked for.
    Value(u32),
    /// The caller's microsecond budget for the poll ran out; this was the last value it read.
    PollTimedOut(u32),
    EventFired,
    EventTimedOut,
    /// The `SD_STS` the CLDMA interrupt read (`cl->sd_status`) when it completed the transfer's completion.
    IocStatus(u8),
    /// The status of the reply to the request an [`Op::IpcSend`] sent: 0 is success, anything else is the firmware's code.
    IpcStatus(u32),
}

/// Why a step failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cause {
    PollTimeout { last: u32 },
    /// The planner's own read bound was reached before the condition held.
    PollBound { reads: u32, last: u32 },
    EventTimeout,
    /// The CLDMA reported a status without `SD_INT_COMPLETE` (cldma.c "cldma transfer error").
    CldmaTransferError { sd_status: u8 },
    /// The firmware answered the request with a non-zero status (`AVS_IPC_RET`).
    IpcRefused { status: u32 },
    /// The SPIB register held a position beyond the buffer.
    SpibBeyondBuffer { spib: u32, buffer_size: u32 },
    /// The caller answered with something the last question did not ask for.
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub step: &'static str,
    pub cause: Cause,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Output {
    Do(Step),
    Finished,
    Failed(Failure),
}
