// SPDX-License-Identifier: GPL-2.0-only
//! The playback path as a list of IPC operations, computed from the topology and the firmware's own tables BEFORE anything is sent, so that
//! every refusal that can be known in advance (a speaker path, a module that is not resident, a missing format, a gateway blob that does not
//! exist) happens with the DSP untouched.
//!
//! Ported from Linux `sound/soc/intel/avs/path.c`: `avs_path_find_variant` (:94-116), `avs_copier_create` / `avs_peakvol_create`
//! (:405-437, :570-611), `avs_path_module_create` (:871-905), `avs_path_binding_arm` (:908-958), `avs_path_pipeline_arm` (:982-1010),
//! `avs_path_pipeline_create` (:1037-1096), `avs_path_init` / `avs_path_arm` (:1098-1154), `avs_path_bind` (:1389-1420) and the
//! instance-id allocators of `dsp.c` (`avs_dsp_create_pipeline`, :299-317; `avs_dsp_init_module`, :225-263).
use crate::config::{PlaybackSel, StreamParams};
use crate::msg::IpcMsg;
use crate::volume::{PeakvolHandle, VOLUME_UNITY};
use alloc::vec::Vec;
use avs_ipc_core::error::IpcError;
use avs_ipc_core::modcfg::*;
use avs_ipc_core::payload::{FwCfg, HwCfg, ModulesInfo};
use avs_ipc_core::regs::AVS_MAILBOX_SIZE;
use avs_ipc_core::request;
use avs_topology_core::model::{Binding, Module, Path, Pipeline};
use avs_topology_core::nhlt::NhltError;
use avs_topology_core::topology::{ModuleKind, Topology};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanError {
    /// No widget of that name in the topology.
    TemplateNotFound { widget: &'static str },
    /// No path variant of the template matches the PCM parameters (path.c:94-116, "no matching variant found").
    NoVariant { widget: &'static str },
    /// A module refers to a format / config / pipeline config the topology does not hold.
    MissingTopologyObject { what: &'static str },
    /// The module's type UUID is not in the firmware's table (`avs_get_module_id` -ENOENT, utils.c:75-82).
    ModuleNotInFirmware { uuid: [u8; 16] },
    /// The module is in the firmware's table but its code is not loaded; loading module files is not part of this path.
    ModuleNotResident { module_id: u16 },
    /// More instances of a module than the firmware allows (`ida_alloc_max`, utils.c).
    ModuleInstancesExhausted { module_id: u16, max: u32 },
    /// More pipelines than the firmware allows (`ida_alloc_max(max_ppl_count - 1)`, dsp.c:305).
    PipelinesExhausted { max: u32 },
    /// A module on a core other than the main core: powering it (`avs_dsp_get_core`) is not planned.
    SecondaryCore { core_id: u8 },
    /// A module kind this path does not build a configuration for.
    ModuleKindNotSupported { kind: ModuleKind },
    /// A module that carries initial configs (`avs_path_module_send_init_configs`, path.c:839-864): not needed for headphones.
    InitConfigsNotSupported,
    /// A back end on one of the board's speaker SSPs. THE SPEAKER AMPLIFIERS ARE NEVER PLANNED.
    SpeakerPathRefused { ssp_instance: u32 },
    /// An I2S gateway on an SSP other than the one the playback path is for.
    SspInstanceNotAllowed { found: u32, wanted: u32 },
    /// A copier gateway type a playback path may not hold (capture, DMIC, HDA link).
    GatewayNotAllowed { dma_type: u32 },
    /// The front end's host copier must be a host-output gateway and the back end's an I2S-output gateway.
    WrongGatewayForRole { dma_type: u32, role: &'static str },
    /// A peakvol module whose topology declares no usable ceiling (zero, or above unity).
    PeakvolCeilingUnset { module_id: u16, declared: u32 },
    /// The NHLT lookup for a gateway failed (path.c:277-335).
    Nhlt(NhltError),
    Ipc(IpcError),
    /// A binding refers to a topology other than this one, or to a path/pipeline/module that was not created (path.c:908-958).
    BindingTargetMissing { what: &'static str },
    /// The back end has bindings of its own, which only the front end's `avs_path_bind` would run.
    BackEndBindings,
    OutOfMemory { what: &'static str },
}

/// One IPC operation of the script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MsgOp {
    CreatePipeline { ppl: u8, req_size: u16, priority: u8, lp: bool, attributes: u16 },
    InitInstance { ppl: u8, module_id: u16, instance: u8, core: u8, domain: u8, param: Vec<u8> },
    Bind { src_module: u16, src_instance: u8, dst_module: u16, dst_instance: u8, dst_queue: u8, src_queue: u8 },
    SetState { ppl: u8, state: u32 },
    /// `avs_ipc_get_pipeline_state` and a comparison with `state`.
    VerifyState { ppl: u8, state: u32 },
}

impl MsgOp {
    pub fn name(&self) -> &'static str {
        match self {
            MsgOp::CreatePipeline { .. } => "create pipeline",
            MsgOp::InitInstance { .. } => "init instance",
            MsgOp::Bind { .. } => "bind modules",
            MsgOp::SetState { .. } => "set pipeline state",
            MsgOp::VerifyState { .. } => "get pipeline state",
        }
    }

    pub fn to_msg(&self) -> Result<IpcMsg, IpcError> {
        match self {
            MsgOp::CreatePipeline { ppl, req_size, priority, lp, attributes } => IpcMsg::from_request(&request::create_pipeline(*req_size, *priority, *ppl, *lp, *attributes)?, 0),
            MsgOp::InitInstance { ppl, module_id, instance, core, domain, param } => IpcMsg::from_request(&request::init_instance(*module_id, *instance, *ppl, *core, *domain, param)?, 0),
            MsgOp::Bind { src_module, src_instance, dst_module, dst_instance, dst_queue, src_queue } => {
                IpcMsg::from_request(&request::bind(*src_module, *src_instance, *dst_module, *dst_instance, *dst_queue, *src_queue)?, 0)
            }
            MsgOp::SetState { ppl, state } => IpcMsg::from_request(&request::set_pipeline_state(*ppl, *state)?, 0),
            MsgOp::VerifyState { ppl, .. } => IpcMsg::from_request(&request::get_pipeline_state(*ppl)?, 0),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    FrontEnd,
    BackEnd,
}

/// A pipeline the plan creates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PipelineInfo {
    pub role: Role,
    /// The topology's pipeline id.
    pub template_id: u32,
    /// The DSP's pipeline instance id.
    pub ppl: u8,
    /// `cfg->trigger == AVS_TPLG_TRIGGER_AUTO` (the pipelines `avs_path_run(AUTO)` sets RUNNING).
    pub auto: bool,
}

/// A module the plan initialises.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleInfo {
    pub role: Role,
    pub ppl: u8,
    pub template_id: u32,
    pub module_id: u16,
    pub instance_id: u8,
    pub kind: ModuleKind,
    pub dma_type: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct PathPlan {
    /// Back end create + arm, then front end create + arm (path.c:1321-1355 per path, `avs_path_arm` :1131-1154).
    pub create: Vec<MsgOp>,
    /// `avs_path_bind` of the front end (path.c:1389-1420).
    pub bind: Vec<MsgOp>,
    pub pipelines: Vec<PipelineInfo>,
    pub modules: Vec<ModuleInfo>,
    pub peakvols: Vec<PeakvolHandle>,
    /// The front end's host DMA id (`stream_tag - 1`).
    pub dma_id: u32,
}

fn reserve<T>(v: &mut Vec<T>, what: &'static str) -> Result<(), PlanError> {
    v.try_reserve(1).map_err(|_| PlanError::OutOfMemory { what })
}

/// `avs_test_hw_params` (path.c:85-92).
fn matches(p: &StreamParams, f: &AudioFormat) -> bool {
    p.rate == f.sampling_freq && p.channels == f.num_channels as u32 && p.container_bits == f.bit_depth && p.valid_bits == f.valid_bit_depth as u32
}

/// `avs_path_find_variant` (path.c:94-116): the first path variant whose front-end AND back-end formats match the given parameters.
fn variant<'t>(topo: &'t Topology<'_>, widget: &[u8], name: &'static str, fe: &StreamParams, be: &StreamParams) -> Result<(&'t avs_topology_core::model::PathTemplate, &'t Path), PlanError> {
    let t = topo.template_by_widget(widget).ok_or(PlanError::TemplateNotFound { widget: name })?;
    for p in topo.paths_of(t) {
        if let (Some(ff), Some(bf)) = (topo.fmt(p.fe_fmt), topo.fmt(p.be_fmt)) {
            if matches(fe, ff) && matches(be, bf) {
                return Ok((t, p));
            }
        }
    }
    Err(PlanError::NoVariant { widget: name })
}

struct Alloc {
    next_ppl: u32,
    max_ppl: u32,
    /// (module id, instances handed out)
    inst: Vec<(u16, u32)>,
}

impl Alloc {
    fn ppl(&mut self) -> Result<u8, PlanError> {
        if self.next_ppl >= self.max_ppl || self.next_ppl > u8::MAX as u32 {
            return Err(PlanError::PipelinesExhausted { max: self.max_ppl });
        }
        let id = self.next_ppl as u8;
        self.next_ppl += 1;
        Ok(id)
    }
    fn instance(&mut self, module_id: u16, max: u32) -> Result<u8, PlanError> {
        let slot = match self.inst.iter().position(|e| e.0 == module_id) {
            Some(i) => i,
            None => {
                reserve(&mut self.inst, "module instance counters")?;
                self.inst.push((module_id, 0));
                self.inst.len() - 1
            }
        };
        let n = self.inst[slot].1;
        if n >= max || n > u8::MAX as u32 {
            return Err(PlanError::ModuleInstancesExhausted { module_id, max });
        }
        self.inst[slot].1 += 1;
        Ok(n as u8)
    }
}

fn buf(len: usize) -> Result<Vec<u8>, PlanError> {
    let mut b = Vec::new();
    b.try_reserve_exact(len).map_err(|_| PlanError::OutOfMemory { what: "module configuration buffer" })?;
    b.resize(len, 0);
    Ok(b)
}

fn ipc_base(t: &Topology<'_>, m: &Module, in_fmt: &AudioFormat) -> Result<ModCfgBase, PlanError> {
    let b = t.modcfg_base(m.cfg_base).ok_or(PlanError::MissingTopologyObject { what: "module base config" })?;
    Ok(ModCfgBase { cpc: b.cpc, ibs: b.ibs, obs: b.obs, is_pages: b.is_pages, audio_fmt: *in_fmt })
}

struct Ctx<'a, 'b> {
    topo: &'a Topology<'b>,
    nhlt: &'a avs_topology_core::nhlt::Nhlt<'a>,
    mods: &'a ModulesInfo<'a>,
    ssp: u32,
    speaker_ssp: &'a [u32],
    dma_id: u32,
}

/// Build one path (back end or front end) into `plan`, allocating instance ids from `al`. Returns the created pipelines' indexes.
fn build_path(c: &Ctx<'_, '_>, role: Role, path: &Path, al: &mut Alloc, plan: &mut PathPlan) -> Result<(), PlanError> {
    let pipelines = c.topo.pipelines_of(path);
    if pipelines.is_empty() {
        return Err(PlanError::MissingTopologyObject { what: "pipelines of the path" });
    }
    let first_info = plan.pipelines.len();
    // path.c:1098-1129: create every pipeline with its modules
    for p in pipelines {
        let cfg = c.topo.pplcfg(p.cfg).ok_or(PlanError::MissingTopologyObject { what: "pipeline config" })?;
        let ppl = al.ppl()?;
        reserve(&mut plan.create, "path script")?;
        plan.create.push(MsgOp::CreatePipeline { ppl, req_size: cfg.req_size, priority: cfg.priority, lp: cfg.lp, attributes: cfg.attributes });
        reserve(&mut plan.pipelines, "pipeline list")?;
        plan.pipelines.push(PipelineInfo { role, template_id: p.id, ppl, auto: cfg.trigger == 0 });
        for m in c.topo.modules_of(p) {
            module(c, role, p, ppl, m, al, plan)?;
        }
    }
    // path.c:1131-1154 avs_path_arm: the bindings are armed (no IPC), then each pipeline's modules are bound to the next module
    for (k, p) in pipelines.iter().enumerate() {
        let info = plan.pipelines[first_info + k];
        let mods: Vec<ModuleInfo> = plan.modules.iter().filter(|m| m.role == role && m.ppl == info.ppl).copied().collect();
        for w in mods.windows(2) {
            reserve(&mut plan.create, "path script")?;
            plan.create.push(MsgOp::Bind { src_module: w[0].module_id, src_instance: w[0].instance_id, dst_module: w[1].module_id, dst_instance: w[1].instance_id, dst_queue: 0, src_queue: 0 });
        }
        let _ = p;
    }
    Ok(())
}

fn module(c: &Ctx<'_, '_>, role: Role, p: &Pipeline, ppl: u8, m: &Module, al: &mut Alloc, plan: &mut PathPlan) -> Result<(), PlanError> {
    let ext = c.topo.modcfg_ext(m.cfg_ext).ok_or(PlanError::MissingTopologyObject { what: "module ext config" })?;
    let kind = avs_topology_core::topology::module_kind(&ext.type_uuid);
    let in_fmt = *c.topo.fmt(m.in_fmt).ok_or(PlanError::MissingTopologyObject { what: "module input format" })?;
    if m.core_id != 0 {
        return Err(PlanError::SecondaryCore { core_id: m.core_id });
    }
    if m.nconfig_ids != 0 {
        return Err(PlanError::InitConfigsNotSupported);
    }
    // the module must exist in the firmware and be resident (path.c:871-905 avs_get_module_id; dsp.c:245-253 would load it)
    let entry = c.mods.find_uuid(&ext.type_uuid).ok_or(PlanError::ModuleNotInFirmware { uuid: ext.type_uuid })?;
    if !entry.is_loaded() {
        return Err(PlanError::ModuleNotResident { module_id: entry.module_id });
    }
    let mut dma_type = None;
    let mut ceiling = None;
    let mut b = buf(AVS_MAILBOX_SIZE)?;
    let n = match kind {
        ModuleKind::Copier => {
            let dt = ext.copier.dma_type;
            dma_type = Some(dt);
            match (role, dt) {
                (Role::FrontEnd, AVS_DMA_HDA_HOST_OUTPUT) => {}
                (Role::BackEnd, AVS_DMA_I2S_LINK_OUTPUT) => {
                    let inst = (ext.copier.vindex >> 4) as u32;
                    if c.speaker_ssp.contains(&inst) {
                        return Err(PlanError::SpeakerPathRefused { ssp_instance: inst });
                    }
                    if inst != c.ssp {
                        return Err(PlanError::SspInstanceNotAllowed { found: inst, wanted: c.ssp });
                    }
                }
                (Role::FrontEnd, AVS_DMA_I2S_LINK_OUTPUT) | (Role::BackEnd, AVS_DMA_HDA_HOST_OUTPUT) => {
                    return Err(PlanError::WrongGatewayForRole { dma_type: dt, role: if role == Role::FrontEnd { "front end" } else { "back end" } });
                }
                (_, other) => return Err(PlanError::GatewayNotAllowed { dma_type: other }),
            }
            let out_fmt = c.topo.fmt(ext.copier.out_fmt).ok_or(PlanError::MissingTopologyObject { what: "copier output format" })?;
            let blob_fmt = c.topo.fmt(ext.copier.blob_fmt);
            let blob: &[u8] = match c.topo.nhlt_config(m.nhlt_config) {
                Some(cfg) => cfg.caps,
                None => c.nhlt.config_or_default(dt, ext.copier.vindex, &in_fmt, out_fmt, blob_fmt).map_err(PlanError::Nhlt)?,
            };
            let base = ipc_base(c.topo, m, &in_fmt)?;
            let node = node_id(dt, ext.copier.vindex, c.dma_id);
            encode_copier_cfg(&base, out_fmt, ext.copier.feature_mask, node, ext.copier.dma_buffer_size, blob, &mut b).map_err(PlanError::Ipc)?
        }
        ModuleKind::Peakvol => {
            let declared = ext.peakvol.target_volume;
            if declared == 0 || declared > VOLUME_UNITY {
                return Err(PlanError::PeakvolCeilingUnset { module_id: entry.module_id, declared });
            }
            ceiling = Some(declared);
            let base = ipc_base(c.topo, m, &in_fmt)?;
            // initialised SILENT (Linux: S32_MAX, path.c:589-593); the user volume is set below the ceiling afterwards
            let vol = VolumeCfg { channel_id: AVS_ALL_CHANNELS_MASK, target_volume: 0, curve_type: ext.peakvol.curve_type, curve_duration: ext.peakvol.curve_duration as u64 };
            encode_peakvol_cfg(&base, &[vol], &mut b).map_err(PlanError::Ipc)?
        }
        other => return Err(PlanError::ModuleKindNotSupported { kind: other }),
    };
    let instance = al.instance(entry.module_id, entry.instance_max_count as u32)?;
    if let Some(ceiling) = ceiling {
        reserve(&mut plan.peakvols, "peakvol list")?;
        plan.peakvols.push(PeakvolHandle { module_id: entry.module_id, instance_id: instance, ceiling });
    }
    finish_module(role, p, ppl, m, kind, dma_type, entry.module_id, instance, b, n, plan)
}

#[allow(clippy::too_many_arguments)]
fn finish_module(role: Role, p: &Pipeline, ppl: u8, m: &Module, kind: ModuleKind, dma_type: Option<u32>, module_id: u16, instance: u8, mut b: Vec<u8>, n: usize, plan: &mut PathPlan) -> Result<(), PlanError> {
    b.truncate(n);
    reserve(&mut plan.create, "path script")?;
    plan.create.push(MsgOp::InitInstance { ppl, module_id, instance, core: m.core_id, domain: m.domain, param: b });
    reserve(&mut plan.modules, "module list")?;
    plan.modules.push(ModuleInfo { role, ppl, template_id: m.id, module_id, instance_id: instance, kind, dma_type });
    let _ = p;
    Ok(())
}

/// `avs_path_binding_arm` + `avs_path_bind` for the front end (path.c:908-958, :1389-1420).
fn bindings(c: &Ctx<'_, '_>, fe_path: &Path, be_template_id: u32, plan: &mut PathPlan, fe_ppl_first: usize) -> Result<(), PlanError> {
    for (k, p) in c.topo.pipelines_of(fe_path).iter().enumerate() {
        let fe_ppl = plan.pipelines[fe_ppl_first + k];
        for b in c.topo.bindings_of(p) {
            bind_one(c, p, &fe_ppl, b, be_template_id, plan)?;
        }
    }
    Ok(())
}

fn bind_one(c: &Ctx<'_, '_>, _p: &Pipeline, fe_ppl: &PipelineInfo, b: &Binding, be_template_id: u32, plan: &mut PathPlan) -> Result<(), PlanError> {
    if b.target_tplg_name.as_bytes() != c.topo.name().as_bytes() {
        return Err(PlanError::BindingTargetMissing { what: "binding to another topology" });
    }
    if b.target_path_tmpl_id != be_template_id {
        return Err(PlanError::BindingTargetMissing { what: "target path template was not created" });
    }
    let this = plan.modules.iter().find(|m| m.role == Role::FrontEnd && m.ppl == fe_ppl.ppl && m.template_id == b.mod_id).copied().ok_or(PlanError::BindingTargetMissing { what: "own module" })?;
    let target_ppl = plan.pipelines.iter().find(|p| p.role == Role::BackEnd && p.template_id == b.target_ppl_id).copied().ok_or(PlanError::BindingTargetMissing { what: "target pipeline" })?;
    let target = plan.modules.iter().find(|m| m.role == Role::BackEnd && m.ppl == target_ppl.ppl && m.template_id == b.target_mod_id).copied().ok_or(PlanError::BindingTargetMissing { what: "target module" })?;
    let (source, source_pin, sink, sink_pin) = if b.is_sink != 0 { (target, b.target_mod_pin, this, b.mod_pin) } else { (this, b.mod_pin, target, b.target_mod_pin) };
    // path.c:1358-1387 avs_path_bind_prepare: only a COPIER bound from a pin other than 0 needs its sink format set first
    if source_pin != 0 && source.kind == ModuleKind::Copier {
        return Err(PlanError::BindingTargetMissing { what: "copier sink format preparation (pin != 0) is not planned" });
    }
    reserve(&mut plan.bind, "bind script")?;
    plan.bind.push(MsgOp::Bind { src_module: source.module_id, src_instance: source.instance_id, dst_module: sink.module_id, dst_instance: sink.instance_id, dst_queue: sink_pin, src_queue: source_pin });
    Ok(())
}

/// Plan the playback path. `mods` is the firmware's modules table after all libraries are loaded; `fw`/`hw` its configuration.
pub fn plan_playback(sel: &PlaybackSel<'_>, dma_id: u32, topo: &Topology<'_>, nhlt: &avs_topology_core::nhlt::Nhlt<'_>, mods: &ModulesInfo<'_>, fw: &FwCfg, hw: &HwCfg) -> Result<PathPlan, PlanError> {
    let _ = hw;
    let (be_t, be_p) = variant(topo, sel.be_widget, "back end", &sel.be, &sel.be)?;
    let (_fe_t, fe_p) = variant(topo, sel.fe_widget, "front end", &sel.fe, &sel.be)?;
    // the back end's own bindings would only be run by a back-end bind, which Linux never issues for a non-HDA BE
    for p in topo.pipelines_of(be_p) {
        if p.nbindings != 0 {
            return Err(PlanError::BackEndBindings);
        }
    }
    let mut plan = PathPlan { create: Vec::new(), bind: Vec::new(), pipelines: Vec::new(), modules: Vec::new(), peakvols: Vec::new(), dma_id };
    let mut al = Alloc { next_ppl: 0, max_ppl: fw.max_ppl_count, inst: Vec::new() };
    let c = Ctx { topo, nhlt, mods, ssp: sel.ssp_instance, speaker_ssp: sel.speaker_ssp, dma_id };
    // the back end is created first, as DPCM calls the back end's hw_params before the front end's (pcm.c:659-697)
    build_path(&c, Role::BackEnd, be_p, &mut al, &mut plan)?;
    let fe_first = plan.pipelines.len();
    build_path(&c, Role::FrontEnd, fe_p, &mut al, &mut plan)?;
    bindings(&c, fe_p, be_t.id, &mut plan, fe_first)?;
    Ok(plan)
}
