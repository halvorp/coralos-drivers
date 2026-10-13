// SPDX-License-Identifier: GPL-2.0-only
//! The boot coroutine. See the crate documentation for the sequence; this module is the machine that drives it.
//!
//! The `.device` loop is:
//!
//! ```text
//! let mut boot = Boot::new(&cfg)?;          // plans the host stream; nothing is touched
//! let mut ev = Event::Begin;
//! loop { match boot.next(ev) {
//!     Action::Fw(step)      => ev = Event::Fw(perform(step)),       // avs-fw-core's vocabulary, incl. IpcSend / IpcAwait / Wait / CopyToDma
//!     Action::Ipc(msg)      => ev = reply_of(run_transaction(msg)), // Event::IpcReply { header, payload } or Event::IpcError(e)
//!     Action::WriteBdl(b)   => { write_bdl_page(b); ev = Event::BdlWritten }
//!     Action::AskCodecSafe  => ev = Event::CodecSafe(da7219_safe_state_verified()),
//!     Action::Finished(r)   => break,
//!     Action::Failed(f)     => { if f.dsp_dirty { power_down_core() } break }
//! } }
//! ```
use crate::config::BootConfig;
use crate::error::{BootFailure, Reason, Stage};
use crate::msg::IpcMsg;
use crate::plan::{plan_playback, MsgOp, PathPlan, PlanError, Role};
use crate::running::{pipeline_order, Running};
use alloc::vec::Vec;
use avs_fw_core::boot::{BaseFwBoot, CodeLoad};
use avs_fw_core::effect::{Input as FwInput, Output as FwOutput, Step};
use avs_fw_core::error::FwError;
use avs_fw_core::library::{Decision, LibraryTable};
use avs_fw_core::programs;
use avs_fw_core::runner::{Program, Runner};
use avs_ipc_core::error::IpcError;
use avs_ipc_core::header::{AVS_PPL_STATE_PAUSED, AVS_PPL_STATE_RUNNING};
use avs_ipc_core::notify::Reply;
use avs_ipc_core::payload::{parse_fw_config, parse_hw_config, parse_modules_info, FwCfg, HwCfg};
use avs_ipc_core::request;
use hda_ext_core::error::HdaError;
use hda_ext_core::stream::{prepare, start, stream_format, HostStream, SetupParams};

/// What the `.device` must do next.
#[derive(Debug)]
pub enum Action {
    /// Perform this hardware step and answer [`Event::Fw`]. `Op::IpcSend` / `Op::IpcAwait` / `Op::Wait` / `Op::CopyToDma` are the code
    /// loader's (avs-fw-core documents them).
    Fw(Step),
    /// Run this message through [`avs_ipc_core::txn::Ipc`] and answer [`Event::IpcReply`] (or [`Event::IpcError`]).
    Ipc(IpcMsg),
    /// Put these bytes in the buffer descriptor list page; answer [`Event::BdlWritten`].
    WriteBdl(Vec<u8>),
    /// Is the DA7219 in its verified safe state (da7219-core `safe_state_plan` read back)? Answer [`Event::CodecSafe`].
    AskCodecSafe,
    Finished(Running),
    Failed(BootFailure),
}

pub enum Event<'e> {
    Begin,
    Fw(FwInput),
    /// The reply to the last [`Action::Ipc`]: its header and the payload the interrupt side copied.
    IpcReply { header: u64, payload: &'e [u8] },
    IpcError(IpcError),
    BdlWritten,
    CodecSafe(bool),
}

enum FwRun {
    Prog(Runner),
    Base(BaseFwBoot),
    Code(CodeLoad),
}

impl FwRun {
    fn next(&mut self, i: FwInput) -> FwOutput {
        match self {
            FwRun::Prog(r) => r.next(i),
            FwRun::Base(b) => b.next(i),
            FwRun::Code(c) => c.next(i),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum IpcKind {
    HwCfg,
    FwCfg,
    ModInfo,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptId {
    Create,
    Bind,
    FePause,
    FeRun,
    BePause,
    BeRun,
}

enum Phase {
    Fw { stage: Stage, run: FwRun },
    Ipc { stage: Stage, kind: IpcKind, name: &'static str },
    Script { stage: Stage, id: ScriptId, ops: Vec<MsgOp>, pos: usize },
    Bdl,
    Codec,
    Finished,
}

struct LibState {
    table: LibraryTable,
    idx: usize,
}

pub struct Boot<'a> {
    cfg: &'a BootConfig<'a>,
    phase: Phase,
    host: HostStream,
    host_programs: Vec<Program>,
    host_prog_idx: usize,
    start_program: Program,
    hw: Option<HwCfg>,
    fw: Option<FwCfg>,
    mods: Vec<u8>,
    plan: Option<PathPlan>,
    lib: Option<LibState>,
    gating_off: bool,
    dsp_started: bool,
    unwind: Option<BootFailure>,
    next_started: bool,
}

fn host_err(e: HdaError) -> BootFailure {
    BootFailure { stage: Stage::HostStream, step: e.name(), reason: Reason::Hda(e), dsp_dirty: false, gating_restored: true }
}

impl<'a> Boot<'a> {
    /// Plan the host stream. Nothing is touched; a refusal here (a bad stream index, an unrepresentable format, an unaligned BDL) happens
    /// before the DSP is.
    pub fn new(cfg: &'a BootConfig<'a>) -> Result<Self, BootFailure> {
        let h = &cfg.host;
        let host = h.streams.playback_stream(h.playback_stream).map_err(host_err)?;
        let p = &cfg.playback.fe;
        let format = stream_format(p.channels, h.format_bits, p.rate).map_err(host_err)?;
        let params = SetupParams::new(format, &h.bdl, h.bdl_addr, h.posbuf);
        let spib = match (h.spb, h.spib) {
            (Some(spb), Some(v)) => Some((spb, v)),
            _ => None,
        };
        let host_programs = prepare(&h.streams, h.pp, &host, &params, spib).map_err(host_err)?;
        let start_program = start(&h.streams, host.index).map_err(host_err)?;
        Ok(Self {
            cfg,
            phase: Phase::Finished,
            host,
            host_programs,
            host_prog_idx: 0,
            start_program,
            hw: None,
            fw: None,
            mods: Vec::new(),
            plan: None,
            lib: None,
            gating_off: false,
            dsp_started: false,
            unwind: None,
            next_started: false,
        })
    }

    /// The program that powers the DSP's main core down, for a caller that was told `dsp_dirty`.
    pub fn shutdown_program() -> Program {
        programs::core_disable(avs_fw_core::regs::AVS_MAIN_CORE_MASK)
    }

    pub fn next(&mut self, ev: Event<'_>) -> Action {
        if !self.next_started {
            self.next_started = true;
            return match ev {
                Event::Begin => self.start_gating_off(),
                _ => self.fail(Stage::GatingOff, "begin", Reason::Protocol),
            };
        }
        match core::mem::replace(&mut self.phase, Phase::Finished) {
            Phase::Fw { stage, run } => match ev {
                Event::Fw(i) => self.pump(stage, run, i),
                _ => self.fail(stage, "fw-answer", Reason::Protocol),
            },
            Phase::Ipc { stage, kind, name } => match ev {
                Event::IpcReply { header, payload } => self.on_reply(stage, kind, name, header, payload),
                Event::IpcError(e) => self.fail(stage, name, Reason::Ipc(e)),
                _ => self.fail(stage, name, Reason::Protocol),
            },
            Phase::Script { stage, id, ops, pos } => match ev {
                Event::IpcReply { header, payload } => self.on_script_reply(stage, id, ops, pos, header, payload),
                Event::IpcError(e) => {
                    let name = ops.get(pos).map(MsgOp::name).unwrap_or("script");
                    self.fail(stage, name, Reason::Ipc(e))
                }
                _ => self.fail(stage, "script", Reason::Protocol),
            },
            Phase::Bdl => match ev {
                Event::BdlWritten => self.next_host_program(),
                _ => self.fail(Stage::HostStream, "write-bdl", Reason::Protocol),
            },
            Phase::Codec => match ev {
                Event::CodecSafe(true) => self.begin_fw(Stage::StreamStart, fw_run_prog(&self.start_program)),
                Event::CodecSafe(false) => self.fail(Stage::CodecGate, "codec-safe", Reason::CodecNotSafe),
                _ => self.fail(Stage::CodecGate, "codec-safe", Reason::Protocol),
            },
            Phase::Finished => match self.unwind.take() {
                Some(f) => Action::Failed(f),
                None => self.fail(Stage::Unwind, "finished", Reason::Protocol),
            },
        }
    }

    // ---- failure --------------------------------------------------------------------------------------------------------------
    fn fail(&mut self, stage: Stage, step: &'static str, reason: Reason) -> Action {
        let mut f = BootFailure { stage, step, reason, dsp_dirty: self.dsp_started, gating_restored: !self.gating_off };
        if self.gating_off {
            // restore what the boot disabled (loader.c:643-646 `reenable_gating`), tolerating every error, then report
            self.gating_off = false;
            f.gating_restored = true;
            match Runner::new(programs::gating(true), Program::new()) {
                Ok(r) => {
                    self.unwind = Some(f);
                    return self.pump(Stage::Unwind, FwRun::Prog(r), FwInput::Begin);
                }
                Err(_) => f.gating_restored = false,
            }
        }
        self.phase = Phase::Finished;
        Action::Failed(f)
    }

    // ---- hardware steps -------------------------------------------------------------------------------------------------------
    fn begin_fw(&mut self, stage: Stage, run: Result<FwRun, FwError>) -> Action {
        match run {
            Ok(r) => self.pump(stage, r, FwInput::Begin),
            Err(e) => self.fail(stage, "setup", Reason::FwSetup(e)),
        }
    }

    fn pump(&mut self, stage: Stage, mut run: FwRun, input: FwInput) -> Action {
        match run.next(input) {
            FwOutput::Do(step) => {
                self.phase = Phase::Fw { stage, run };
                Action::Fw(step)
            }
            FwOutput::Finished => self.after_fw(stage),
            FwOutput::Failed(f) => {
                if stage == Stage::Unwind {
                    // the unwind tolerates everything; report the original failure
                    return self.after_fw(stage);
                }
                self.fail(stage, f.step, Reason::Fw(f))
            }
        }
    }

    fn after_fw(&mut self, stage: Stage) -> Action {
        match stage {
            Stage::GatingOff => {
                self.gating_off = true;
                self.dsp_started = true;
                match BaseFwBoot::new(&self.cfg.layout, self.cfg.basefw_len) {
                    Ok(b) => self.pump(Stage::BaseFirmware, FwRun::Base(b), FwInput::Begin),
                    Err(e) => self.fail(Stage::BaseFirmware, "setup", Reason::FwSetup(e)),
                }
            }
            Stage::BaseFirmware => self.ask(Stage::ConfigQuery, IpcKind::HwCfg, request::get_hw_config()),
            Stage::Libraries => self.next_library(),
            Stage::GatingOn => {
                self.gating_off = false;
                self.ask(Stage::ModuleInfo, IpcKind::ModInfo, request::get_modules_info())
            }
            Stage::HostStream => self.next_host_program(),
            Stage::StreamStart => self.state_script(Stage::PathPause, ScriptId::FePause, Role::FrontEnd, AVS_PPL_STATE_PAUSED, true),
            Stage::Unwind => {
                self.phase = Phase::Finished;
                match self.unwind.take() {
                    Some(f) => Action::Failed(f),
                    None => Action::Failed(BootFailure { stage: Stage::Unwind, step: "unwind", reason: Reason::Protocol, dsp_dirty: self.dsp_started, gating_restored: true }),
                }
            }
            other => self.fail(other, "after-fw", Reason::Protocol),
        }
    }

    // ---- ipc ------------------------------------------------------------------------------------------------------------------
    fn ask(&mut self, stage: Stage, kind: IpcKind, r: Result<request::Request<'static>, IpcError>) -> Action {
        let name = match kind {
            IpcKind::HwCfg => "get hw config",
            IpcKind::FwCfg => "get fw config",
            IpcKind::ModInfo => "get modules info",
        };
        let msg = match r.and_then(|r| IpcMsg::from_request(&r, IpcMsg::reply_limit())) {
            Ok(m) => m,
            Err(e) => return self.fail(stage, name, Reason::Ipc(e)),
        };
        self.phase = Phase::Ipc { stage, kind, name };
        Action::Ipc(msg)
    }

    fn on_reply(&mut self, stage: Stage, kind: IpcKind, name: &'static str, header: u64, payload: &[u8]) -> Action {
        let status = Reply::new(header).status();
        if status != 0 {
            return self.fail(stage, name, Reason::IpcRefused { status });
        }
        match kind {
            IpcKind::HwCfg => match parse_hw_config(payload) {
                Ok(hw) => {
                    self.hw = Some(hw);
                    self.ask(Stage::ConfigQuery, IpcKind::FwCfg, request::get_fw_config())
                }
                Err(e) => self.fail(stage, name, Reason::Ipc(e)),
            },
            IpcKind::FwCfg => match parse_fw_config(payload) {
                Ok(fw) => self.after_config(fw),
                Err(e) => self.fail(stage, name, Reason::Ipc(e)),
            },
            IpcKind::ModInfo => {
                if self.mods.try_reserve_exact(payload.len()).is_err() {
                    return self.fail(stage, name, Reason::OutOfMemory { what: "modules info copy" });
                }
                self.mods.clear();
                self.mods.extend_from_slice(payload);
                self.plan_path()
            }
        }
    }

    /// The firmware's own report is checked before anything is built on it: its version, the SSP port the path needs, and room for the
    /// pipelines and libraries the topology asks for.
    fn after_config(&mut self, fw: FwCfg) -> Action {
        let st = Stage::ConfigQuery;
        let v = fw.fw_version;
        let have = (v.major, v.minor, v.hotfix, v.build);
        if have < self.cfg.min_fw {
            return self.fail(st, "fw-version", Reason::FwVersionTooOld { have, need: self.cfg.min_fw });
        }
        let ssp = self.cfg.playback.ssp_instance;
        let ctrl = self.hw.as_ref().map(|h| h.i2s_caps.ctrl_count).unwrap_or(0);
        if ctrl <= ssp {
            return self.fail(st, "ssp-port", Reason::SspPortMissing { port: ssp, ctrl_count: ctrl });
        }
        if fw.max_ppl_count == 0 {
            return self.fail(st, "pipeline-limit", Reason::FirmwareLimit { what: "pipelines", have: 0, need: 1 });
        }
        let topo_libs = self.cfg.topology.libs.len();
        if topo_libs != self.cfg.libs.len() {
            return self.fail(Stage::Libraries, "library-images", Reason::LibraryImagesMismatch { topology: topo_libs, supplied: self.cfg.libs.len() });
        }
        let table = match LibraryTable::new(fw.max_libs_count) {
            Ok(t) => t,
            Err(e) => return self.fail(Stage::Libraries, "library-table", Reason::FwSetup(e)),
        };
        self.fw = Some(fw);
        self.lib = Some(LibState { table, idx: 0 });
        if topo_libs > 0 {
            // loader.c:498-499: the batch must leave a slot free
            let begin = self.lib.as_ref().map(|l| l.table.begin(topo_libs as u32));
            if let Some(Err(e)) = begin {
                return self.fail(Stage::Libraries, "library-table", Reason::FwSetup(e));
            }
        }
        self.next_library()
    }

    // ---- libraries ------------------------------------------------------------------------------------------------------------
    fn next_library(&mut self) -> Action {
        loop {
            let Some(l) = self.lib.as_mut() else { return self.fail(Stage::Libraries, "libraries", Reason::Protocol) };
            if l.idx >= self.cfg.libs.len() {
                self.lib = None;
                // gating back on, then the modules table (loader.c:643-646, :652)
                return self.begin_fw(Stage::GatingOn, Runner::new(programs::gating(true), Program::new()).map(FwRun::Prog));
            }
            let img = self.cfg.libs[l.idx];
            match l.table.decide(&img.name) {
                Decision::AlreadyLoaded => {
                    l.idx += 1;
                }
                Decision::Load { lib_id } => {
                    // record only after the DSP accepted the library: do it optimistically here and undo on failure is not needed because a
                    // failure ends the boot; the table is private to this boot
                    l.table.record(lib_id, &img.name);
                    l.idx += 1;
                    let run = CodeLoad::library(&self.cfg.layout, img.image_len, lib_id).map(FwRun::Code);
                    return self.begin_fw(Stage::Libraries, run);
                }
            }
        }
    }

    // ---- path -----------------------------------------------------------------------------------------------------------------
    fn plan_path(&mut self) -> Action {
        let (Some(fw), Some(hw)) = (self.fw.as_ref(), self.hw.as_ref()) else { return self.fail(Stage::PathPlan, "plan", Reason::Protocol) };
        let mods = match parse_modules_info(&self.mods) {
            Ok(m) => m,
            Err(e) => return self.fail(Stage::ModuleInfo, "parse modules info", Reason::Ipc(e)),
        };
        let planned: Result<PathPlan, PlanError> = plan_playback(&self.cfg.playback, self.host.dma_id(), self.cfg.topology, self.cfg.nhlt, &mods, fw, hw);
        match planned {
            Ok(p) => {
                let ops = p.create.clone();
                self.plan = Some(p);
                self.start_script(Stage::PathCreate, ScriptId::Create, ops)
            }
            Err(e) => self.fail(Stage::PathPlan, "plan playback path", Reason::Plan(e)),
        }
    }

    fn start_script(&mut self, stage: Stage, id: ScriptId, ops: Vec<MsgOp>) -> Action {
        self.run_script(stage, id, ops, 0)
    }

    fn run_script(&mut self, stage: Stage, id: ScriptId, ops: Vec<MsgOp>, pos: usize) -> Action {
        if pos >= ops.len() {
            return self.script_done(id);
        }
        let name = ops[pos].name();
        match ops[pos].to_msg() {
            Ok(m) => {
                self.phase = Phase::Script { stage, id, ops, pos };
                Action::Ipc(m)
            }
            Err(e) => self.fail(stage, name, Reason::Ipc(e)),
        }
    }

    fn on_script_reply(&mut self, stage: Stage, id: ScriptId, ops: Vec<MsgOp>, pos: usize, header: u64, _payload: &[u8]) -> Action {
        let name = ops[pos].name();
        let r = Reply::new(header);
        if r.status() != 0 {
            return self.fail(stage, name, Reason::IpcRefused { status: r.status() });
        }
        if let MsgOp::VerifyState { ppl, state } = ops[pos] {
            let got = r.ppl_state();
            if got != state {
                return self.fail(stage, name, Reason::PipelineStateMismatch { ppl, want: state, got });
            }
        }
        self.run_script(stage, id, ops, pos + 1)
    }

    fn script_done(&mut self, id: ScriptId) -> Action {
        match id {
            ScriptId::Create => {
                let ops = self.plan.as_ref().map(|p| p.bind.clone()).unwrap_or_default();
                self.start_script(Stage::PathBind, ScriptId::Bind, ops)
            }
            ScriptId::Bind => self.begin_host(),
            ScriptId::FePause => self.state_script(Stage::PathRun, ScriptId::FeRun, Role::FrontEnd, AVS_PPL_STATE_RUNNING, false),
            ScriptId::FeRun => self.state_script(Stage::PathPause, ScriptId::BePause, Role::BackEnd, AVS_PPL_STATE_PAUSED, true),
            ScriptId::BePause => self.state_script(Stage::PathRun, ScriptId::BeRun, Role::BackEnd, AVS_PPL_STATE_RUNNING, false),
            ScriptId::BeRun => self.finish(),
        }
    }

    /// `avs_path_pause` (reverse order, path.c:1510-1533) and `avs_path_run` (forward, only AUTO pipelines, path.c:1579-1608), each state
    /// READ BACK.
    fn state_script(&mut self, stage: Stage, id: ScriptId, role: Role, state: u32, reverse: bool) -> Action {
        let Some(plan) = self.plan.as_ref() else { return self.fail(stage, "state script", Reason::Protocol) };
        let running = state == AVS_PPL_STATE_RUNNING;
        let ids = match pipeline_order(&plan.pipelines, role, running, reverse) {
            Ok(v) => v,
            Err(e) => return self.fail(stage, "state script", Reason::Ipc(e)),
        };
        let mut ops: Vec<MsgOp> = Vec::new();
        if ops.try_reserve_exact(ids.len() * 2).is_err() {
            return self.fail(stage, "state script", Reason::OutOfMemory { what: "state script" });
        }
        for ppl in ids {
            ops.push(MsgOp::SetState { ppl, state });
            ops.push(MsgOp::VerifyState { ppl, state });
        }
        self.start_script(stage, id, ops)
    }

    // ---- host stream ----------------------------------------------------------------------------------------------------------
    fn begin_host(&mut self) -> Action {
        let h = &self.cfg.host;
        let n = h.bdl.entries.len() * hda_ext_core::bdl::BDL_ENTRY_BYTES;
        let mut bytes: Vec<u8> = Vec::new();
        if bytes.try_reserve_exact(n).is_err() {
            return self.fail(Stage::HostStream, "write-bdl", Reason::OutOfMemory { what: "BDL bytes" });
        }
        bytes.resize(n, 0);
        if let Err(e) = h.bdl.write_to(&mut bytes) {
            return self.fail(Stage::HostStream, "write-bdl", Reason::Hda(e));
        }
        self.phase = Phase::Bdl;
        Action::WriteBdl(bytes)
    }

    fn next_host_program(&mut self) -> Action {
        if self.host_prog_idx >= self.host_programs.len() {
            // the host stream is prepared: the codec must be proven safe BEFORE any clock is started
            self.phase = Phase::Codec;
            return Action::AskCodecSafe;
        }
        let p = self.host_programs[self.host_prog_idx];
        self.host_prog_idx += 1;
        let run = Runner::new(p, Program::new()).map(FwRun::Prog);
        self.begin_fw(Stage::HostStream, run)
    }

    fn finish(&mut self) -> Action {
        let (Some(plan), Some(fw), Some(hw)) = (self.plan.as_ref(), self.fw.take(), self.hw.take()) else { return self.fail(Stage::PathRun, "finish", Reason::Protocol) };
        match Running::new(plan, self.host, fw, hw) {
            Ok(r) => {
                self.phase = Phase::Finished;
                Action::Finished(r)
            }
            Err(e) => self.fail(Stage::PathRun, "finish", Reason::Ipc(e)),
        }
    }

    fn start_gating_off(&mut self) -> Action {
        self.begin_fw(Stage::GatingOff, Runner::new(programs::gating(false), Program::new()).map(FwRun::Prog))
    }
}

fn fw_run_prog(p: &Program) -> Result<FwRun, FwError> {
    Runner::new(*p, Program::new()).map(FwRun::Prog)
}
