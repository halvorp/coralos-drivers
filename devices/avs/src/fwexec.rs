// SPDX-License-Identifier: GPL-2.0-only
//! The executor of avs-fw-core's plans: every [`Step`] a planner asks for, performed against the registers, the DMA memory, the interrupt
//! pump and the IPC machine, and answered with the [`Input`] the planner judges. It is the only code that turns the vocabulary of
//! `Op` into effects, and it adds no policy: a failure is the planner's to name.
//!
//! * `Update` / `Write` / `Read` go through the [`Rig`] (the write allow-list).
//! * `Poll` is one read per question. Between questions the task SLEEPS (a blocking timed wait), and the executor answers
//!   `PollTimedOut` when the step's own microsecond budget has run out on the monotonic clock; the planner independently stops at its read
//!   bound. Delays are served at millisecond granularity, rounded up.
//! * `Wait` blocks on the interrupt (`FwReady`, the code loader's completion) with the op's deadline.
//! * `CopyToDma` copies from the image the boot is moving into the code loader's buffer.
//! * `IpcSend` / `IpcAwait` are the split transaction ([`crate::ipcrun`]).
use crate::ipcrun::{self, Drive, IpcFail};
use crate::machine::{Machine, RegFault};
use crate::pump::{Pump, PumpError};
use crate::rig::Rig;
use alloc::vec::Vec;
use avs_boot_core::boot::Image;
use avs_fw_core::boot::{BaseFwBoot, CodeLoad};
use avs_fw_core::effect::{Event, Failure, Input, Op, Output, Reg, Step};
use avs_fw_core::runner::{Program, Runner};
use avs_ipc_core::error::IpcError;
use avs_ipc_core::txn::{Completed, Output as IpcOutput};
use coralos_hal::DmaRegion;

/// The firmware images of one boot, stripped of their extended manifests (the bytes the code loader's DMA carries).
pub struct Images<'a> {
    pub base: &'a [u8],
    pub libs: Vec<&'a [u8]>,
}

impl<'a> Images<'a> {
    pub fn get(&self, which: Image) -> Option<&'a [u8]> {
        match which {
            Image::BaseFirmware => Some(self.base),
            Image::Library(i) => self.libs.get(i).copied(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecFail {
    Reg(RegFault),
    Pump(PumpError),
    Ipc(IpcFail),
    /// A code-loader copy with no image selected, or from an image that does not exist.
    NoImage,
    /// A planned copy that does not fit the image or the buffer (a planner bug, refused before any byte moves).
    CopyRange { src: u32, dst: u32, len: u32 },
    /// A step the context cannot perform (a copy with no code-loader buffer, an IPC with no pump).
    Unsupported { label: &'static str },
    /// The send of a request failed to start (the machine refused it).
    IpcStart(IpcError),
    /// A plan could not be built (the core named why).
    PlanBuild(&'static str),
}

impl ExecFail {
    pub const fn name(&self) -> &'static str {
        match self {
            ExecFail::Reg(e) => e.name(),
            ExecFail::Pump(e) => e.name(),
            ExecFail::Ipc(e) => e.name(),
            ExecFail::NoImage => "no-image",
            ExecFail::CopyRange { .. } => "copy-out-of-range",
            ExecFail::Unsupported { .. } => "unsupported-step",
            ExecFail::IpcStart(e) => e.name(),
            ExecFail::PlanBuild(_) => "plan-build",
        }
    }
}

impl From<RegFault> for ExecFail {
    fn from(e: RegFault) -> Self {
        ExecFail::Reg(e)
    }
}
impl From<PumpError> for ExecFail {
    fn from(e: PumpError) -> Self {
        ExecFail::Pump(e)
    }
}
impl From<IpcFail> for ExecFail {
    fn from(e: IpcFail) -> Self {
        ExecFail::Ipc(e)
    }
}

/// How a planner run ended badly: the plan failed by name, or the executor could not do what the plan asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunFail {
    Planner(Failure),
    Exec(ExecFail),
}

impl RunFail {
    pub const fn step(&self) -> &'static str {
        match self {
            RunFail::Planner(f) => f.step,
            RunFail::Exec(e) => e.name(),
        }
    }
}

/// Anything that answers `Input` with `Output` the way the avs-fw-core planners do.
pub trait Planner {
    fn advance(&mut self, input: Input) -> Output;
}
impl Planner for Runner {
    fn advance(&mut self, input: Input) -> Output {
        self.next(input)
    }
}
impl Planner for BaseFwBoot {
    fn advance(&mut self, input: Input) -> Output {
        self.next(input)
    }
}
impl Planner for CodeLoad {
    fn advance(&mut self, input: Input) -> Output {
        self.next(input)
    }
}

#[derive(Clone, Copy)]
struct PollTrack {
    label: &'static str,
    reg: Reg,
    start_ms: u64,
}

/// What the executor remembers between steps.
pub struct FwCtx<'a> {
    pub images: Option<&'a Images<'a>>,
    pub current: Option<Image>,
    /// The code loader's data buffer.
    pub cldma: Option<DmaRegion>,
    poll: Option<PollTrack>,
    parked: Option<IpcOutput>,
    finished: Option<Completed>,
}

impl<'a> FwCtx<'a> {
    /// A context for programs that only touch registers.
    pub const fn bare() -> Self {
        Self { images: None, current: None, cldma: None, poll: None, parked: None, finished: None }
    }
    pub const fn loader(images: &'a Images<'a>, cldma: DmaRegion) -> Self {
        Self { images: Some(images), current: None, cldma: Some(cldma), poll: None, parked: None, finished: None }
    }
}

/// Perform one step.
pub fn perform<M: Machine>(rig: &mut Rig<M>, pump: &mut Pump, ctx: &mut FwCtx<'_>, step: &Step) -> Result<Input, ExecFail> {
    if !matches!(step.op, Op::Poll(_)) {
        ctx.poll = None;
    }
    Ok(match step.op {
        Op::Update { reg, mask, value } => {
            rig.update(reg, mask, value)?;
            Input::Done
        }
        Op::Write { reg, value } => {
            rig.write(reg, value)?;
            Input::Done
        }
        Op::Read { reg } => Input::Value(rig.read(reg)?),
        Op::Poll(p) => {
            let now = rig.now();
            let track = match ctx.poll {
                Some(t) if t.label == step.label && t.reg == p.reg => t,
                _ => PollTrack { label: step.label, reg: p.reg, start_ms: now },
            };
            ctx.poll = Some(track);
            let v = rig.read(p.reg)?;
            if p.satisfied(v) {
                ctx.poll = None;
                Input::Value(v)
            } else if now.saturating_sub(track.start_ms).saturating_mul(1000) >= p.timeout_us as u64 {
                ctx.poll = None;
                Input::PollTimedOut(v)
            } else {
                // a bounded hardware handshake with no wakeup source (the plan's own clock budget and read bound end it): a blocking timed
                // wait between reads, the PollPlan exception of rig::Rig::sleep_ms - never a spin
                rig.sleep_us(p.interval_us);
                Input::Value(v)
            }
        }
        Op::DelayUs(us) => {
            rig.sleep_us(us);
            Input::Done
        }
        Op::Wait { event: Event::FwReady, timeout_ms } => {
            let deadline = rig.now().saturating_add(timeout_ms as u64);
            if pump.wait_for(rig, deadline, |p| p.fw_ready)? {
                Input::EventFired
            } else {
                Input::EventTimedOut
            }
        }
        Op::Wait { event: Event::CldmaIoc, timeout_ms } => {
            let deadline = rig.now().saturating_add(timeout_ms as u64);
            if pump.wait_for(rig, deadline, |p| p.cldma.is_some())? {
                Input::IocStatus(pump.pend.cldma.take().unwrap_or(0))
            } else {
                Input::EventTimedOut
            }
        }
        Op::CopyToDma { dst_offset, src_offset, len } => {
            let (Some(images), Some(buf)) = (ctx.images, ctx.cldma) else { return Err(ExecFail::Unsupported { label: step.label }) };
            let image = ctx.current.and_then(|c| images.get(c)).ok_or(ExecFail::NoImage)?;
            let (s, d, l) = (src_offset as usize, dst_offset as usize, len as usize);
            if s.checked_add(l).is_none_or(|e| e > image.len()) || d.checked_add(l).is_none_or(|e| e > buf.len) {
                return Err(ExecFail::CopyRange { src: src_offset, dst: dst_offset, len });
            }
            rig.m.dma_write(&buf, d, &image[s..s + l]);
            Input::Done
        }
        Op::IpcSend(call) => {
            let req = ipcrun::call_request(call).map_err(ExecFail::IpcStart)?;
            match ipcrun::begin(rig, pump, &req, 0, true)? {
                Drive::Parked(o) => ctx.parked = Some(o),
                Drive::Done(c) => ctx.finished = Some(c),
            }
            Input::Done
        }
        Op::IpcAwait { .. } => {
            let done = if let Some(p) = ctx.parked.take() {
                match ipcrun::await_reply(rig, pump, p) {
                    Ok(c) => c,
                    Err(IpcFail::Ipc(IpcError::Timeout { .. })) => return Ok(Input::EventTimedOut),
                    Err(e) => return Err(e.into()),
                }
            } else {
                ctx.finished.take().ok_or(ExecFail::Unsupported { label: step.label })?
            };
            Input::IpcStatus(done.status())
        }
    })
}

/// Run a planner to its end.
pub fn run_planner<M: Machine, P: Planner>(rig: &mut Rig<M>, pump: &mut Pump, ctx: &mut FwCtx<'_>, planner: &mut P) -> Result<(), RunFail> {
    let mut input = Input::Begin;
    // The planners bound themselves (poll reads, transfer chunks); this is a backstop against a planner that never ends.
    for _ in 0..1_000_000u32 {
        match planner.advance(input) {
            Output::Finished => return Ok(()),
            Output::Failed(f) => return Err(RunFail::Planner(f)),
            Output::Do(step) => input = perform(rig, pump, ctx, &step).map_err(RunFail::Exec)?,
        }
    }
    Err(RunFail::Exec(ExecFail::Unsupported { label: "planner-never-ended" }))
}

/// Run a register-only program (gating, controller, stream, core power) on a throwaway pump context.
pub fn run_program<M: Machine>(rig: &mut Rig<M>, program: &Program) -> Result<(), RunFail> {
    let mut pump = Pump::new(avs_ipc_core::txn::Correlation::Strict);
    run_program_with(rig, &mut pump, program)
}

/// Run a register-only program against an existing pump (so its interrupt state survives).
pub fn run_program_with<M: Machine>(rig: &mut Rig<M>, pump: &mut Pump, program: &Program) -> Result<(), RunFail> {
    let mut runner = Runner::new(*program, Program::new()).map_err(|_| RunFail::Exec(ExecFail::Unsupported { label: "program-overflow" }))?;
    let mut ctx = FwCtx::bare();
    run_planner(rig, pump, &mut ctx, &mut runner)
}
