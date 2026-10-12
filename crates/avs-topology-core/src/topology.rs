// SPDX-License-Identifier: GPL-2.0-only
//! The topology as a whole: `struct avs_tplg` plus what the ASoC core keeps for it (widgets' path templates, routes, PCMs,
//! controls), and the loader that walks a topology file the way `snd_soc_tplg_component_load` does with the AVS callbacks
//! (soc-topology.c:2067-2108, topology.c:2197-2207): the blocks in PASSES - manifest, vendor, controls, widgets, PCMs, graph,
//! DAIs, links - so the manifest's dictionaries exist before any widget names an index into them.
use crate::blocks::{self, Control, Pcm, Route};
use crate::error::TplgError;
use crate::model::*;
use crate::pool::Pool;
use crate::vendor::Name;
use avs_ipc_core::modcfg::{
    AudioFormat, AVS_AEC_MOD_UUID, AVS_ASRC_MOD_UUID, AVS_COPIER_MOD_UUID, AVS_GAIN_MOD_UUID, AVS_INTELWOV_MOD_UUID, AVS_KPBUFF_MOD_UUID, AVS_MICSEL_MOD_UUID,
    AVS_MIXIN_MOD_UUID, AVS_MIXOUT_MOD_UUID, AVS_MUX_MOD_UUID, AVS_PEAKVOL_MOD_UUID, AVS_PROBE_MOD_UUID, AVS_SRCINTC_MOD_UUID, AVS_UPDWMIX_MOD_UUID,
    AVS_WOVHOSTM_MOD_UUID,
};

/// `struct avs_tplg_init_config` with its raw data section (topology.h:165-170).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct InitConfig<'a> {
    pub id: u32,
    pub param: u8,
    pub data: &'a [u8],
}

/// `struct avs_tplg_nhlt_config` (topology.h:172-175): the capabilities blob the topology carries for a gateway.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NhltConfig<'a> {
    pub id: u32,
    pub caps: &'a [u8],
}

/// What a module type is, by the UUID of its configuration (`type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    Copier,
    Mixin,
    Mixout,
    Peakvol,
    Gain,
    Mux,
    UpdownMix,
    Src,
    Asrc,
    Micsel,
    Aec,
    Kpbuff,
    IntelWov,
    WovHostm,
    Probe,
    /// A module-type UUID this port does not know.
    Other,
}

pub fn module_kind(uuid: &[u8; 16]) -> ModuleKind {
    match *uuid {
        AVS_COPIER_MOD_UUID => ModuleKind::Copier,
        AVS_MIXIN_MOD_UUID => ModuleKind::Mixin,
        AVS_MIXOUT_MOD_UUID => ModuleKind::Mixout,
        AVS_PEAKVOL_MOD_UUID => ModuleKind::Peakvol,
        AVS_GAIN_MOD_UUID => ModuleKind::Gain,
        AVS_MUX_MOD_UUID => ModuleKind::Mux,
        AVS_UPDWMIX_MOD_UUID => ModuleKind::UpdownMix,
        AVS_SRCINTC_MOD_UUID => ModuleKind::Src,
        AVS_ASRC_MOD_UUID => ModuleKind::Asrc,
        AVS_MICSEL_MOD_UUID => ModuleKind::Micsel,
        AVS_AEC_MOD_UUID => ModuleKind::Aec,
        AVS_KPBUFF_MOD_UUID => ModuleKind::Kpbuff,
        AVS_INTELWOV_MOD_UUID => ModuleKind::IntelWov,
        AVS_WOVHOSTM_MOD_UUID => ModuleKind::WovHostm,
        AVS_PROBE_MOD_UUID => ModuleKind::Probe,
        _ => ModuleKind::Other,
    }
}

/// A parsed AVS topology. It is large (about 60 KiB, see `Topology::SIZE_BOUND`); construct it where the caller has room.
pub struct Topology<'a> {
    pub header: Header,
    pub libs: Pool<Library, MAX_LIBS>,
    pub fmts: Pool<AudioFormat, MAX_FMTS>,
    pub modcfgs_base: Pool<ModCfgBase, MAX_MODCFGS_BASE>,
    pub modcfgs_ext: Pool<ModCfgExt, MAX_MODCFGS_EXT>,
    pub pplcfgs: Pool<PplCfg, MAX_PPLCFGS>,
    pub bindings: Pool<Binding, MAX_BINDINGS>,
    pub condpath_tmpls: Pool<PathTemplate, MAX_CONDPATH_TMPLS>,
    pub init_configs: Pool<InitConfig<'a>, MAX_INIT_CONFIGS>,
    pub nhlt_configs: Pool<NhltConfig<'a>, MAX_NHLT_CONFIGS>,
    /// The path templates of the widgets, in file order (`tplg->path_tmpl_list`).
    pub templates: Pool<PathTemplate, MAX_TEMPLATES>,
    pub paths: Pool<Path, MAX_PATHS>,
    pub pipelines: Pool<Pipeline, MAX_PIPELINES>,
    pub modules: Pool<Module, MAX_MODULES>,
    pub routes: Pool<Route, MAX_ROUTES>,
    pub controls: Pool<Control, MAX_CONTROLS>,
    pub pcms: Pool<Pcm, MAX_PCMS>,
    /// DAPM widgets in the file, with or without private data.
    pub widgets_seen: u32,
    pub(crate) counts: Counts,
    pub(crate) have_manifest: bool,
}

impl<'a> Topology<'a> {
    /// A generous bound on `size_of::<Topology>()`, pinned by a test so storage cannot grow unnoticed.
    pub const SIZE_BOUND: usize = 80 * 1024;

    pub fn new() -> Self {
        Self {
            header: Header::default(),
            libs: Pool::new(),
            fmts: Pool::new(),
            modcfgs_base: Pool::new(),
            modcfgs_ext: Pool::new(),
            pplcfgs: Pool::new(),
            bindings: Pool::new(),
            condpath_tmpls: Pool::new(),
            init_configs: Pool::new(),
            nhlt_configs: Pool::new(),
            templates: Pool::new(),
            paths: Pool::new(),
            pipelines: Pool::new(),
            modules: Pool::new(),
            routes: Pool::new(),
            controls: Pool::new(),
            pcms: Pool::new(),
            widgets_seen: 0,
            counts: Counts::default(),
            have_manifest: false,
        }
    }

    /// Load a whole topology file. `ssp` is the machine's single SSP/TDM binding, if it has exactly one: `%d` in names becomes the
    /// port (`ssp0`), and a copier I2S gateway without a virtual index gets one (topology.c:398-422, :918-952).
    pub fn parse(bytes: &'a [u8], ssp: Option<SspBinding>) -> Result<Self, TplgError> {
        let mut t = Self::new();
        t.load(bytes, ssp)?;
        Ok(t)
    }

    pub fn load(&mut self, bytes: &'a [u8], ssp: Option<SspBinding>) -> Result<(), TplgError> {
        blocks::load(self, bytes, ssp)
    }

    // ---- accessors ---------------------------------------------------------------------------------------------------------
    pub fn fmt(&self, i: Idx) -> Option<&AudioFormat> {
        self.fmts.get(i? as usize)
    }
    pub fn modcfg_base(&self, i: Idx) -> Option<&ModCfgBase> {
        self.modcfgs_base.get(i? as usize)
    }
    pub fn modcfg_ext(&self, i: Idx) -> Option<&ModCfgExt> {
        self.modcfgs_ext.get(i? as usize)
    }
    pub fn pplcfg(&self, i: Idx) -> Option<&PplCfg> {
        self.pplcfgs.get(i? as usize)
    }
    pub fn nhlt_config(&self, i: Idx) -> Option<&NhltConfig<'a>> {
        self.nhlt_configs.get(i? as usize)
    }
    /// The template of the widget named `name`.
    pub fn template_by_widget(&self, name: &[u8]) -> Option<&PathTemplate> {
        self.templates.as_slice().iter().find(|t| t.widget_name.as_bytes() == name)
    }
    pub fn paths_of(&self, t: &PathTemplate) -> &[Path] {
        let a = t.first_path as usize;
        self.paths.as_slice().get(a..a + t.npaths as usize).unwrap_or(&[])
    }
    pub fn pipelines_of(&self, p: &Path) -> &[Pipeline] {
        let a = p.first_pipeline as usize;
        self.pipelines.as_slice().get(a..a + p.npipelines as usize).unwrap_or(&[])
    }
    pub fn modules_of(&self, p: &Pipeline) -> &[Module] {
        let a = p.first_module as usize;
        self.modules.as_slice().get(a..a + p.nmodules as usize).unwrap_or(&[])
    }
    /// The bindings a pipeline lists (`pipeline->bindings`), resolved.
    pub fn bindings_of<'t>(&'t self, p: &'t Pipeline) -> impl Iterator<Item = &'t Binding> + 't {
        p.bindings[..p.nbindings as usize].iter().filter_map(move |&i| self.bindings.get(i as usize))
    }
    /// What kind of module a module instance of the topology is: the UUID of its `cfg_ext`.
    pub fn kind_of(&self, m: &Module) -> Option<ModuleKind> {
        self.modcfg_ext(m.cfg_ext).map(|e| module_kind(&e.type_uuid))
    }
    pub fn name(&self) -> &Name {
        &self.header.name
    }
}

impl Default for Topology<'_> {
    fn default() -> Self {
        Self::new()
    }
}
