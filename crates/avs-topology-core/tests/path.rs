// SPDX-License-Identifier: GPL-2.0-only
//! Path templates: template -> paths -> pipelines -> modules (+ bindings and config ids), topology.c:1221-1578.
mod common;
use avs_topology_core::blocks::*;
use avs_topology_core::error::TplgError;
use avs_topology_core::tokens::*;
use avs_topology_core::topology::*;
use common::*;

fn load_widget(priv_: &[u8]) -> Result<Topology<'static>, TplgError> {
    let f: &'static [u8] = Box::leak(cat(&[manifest_block(&small_manifest()), block(5, 1, &widget("w", priv_, &[]))]).into_boxed_slice());
    Topology::parse(f, None)
}

#[test]
fn a_template_with_two_paths_each_with_pipelines_and_modules_keeps_its_order() {
    let p = cat(&[
        tmpl_hdr(7, "w"),
        // path 0: two pipelines
        path_hdr(0, 0, 1),
        pipeline_hdr(10, 0, 1),
        module(0, 0, 0, 0),
        module(1, 1, 1, 1),
        binding_ref(1),
        pipeline_hdr(11, 0, 0),
        module(0, 1, 0, 2),
        // path 1: one pipeline
        path_hdr(1, 1, 0),
        pipeline_hdr(12, 0, 0),
        module(0, 0, 1, 0),
    ]);
    let t = load_widget(&p).unwrap();
    assert_eq!(t.templates.len(), 1);
    let tm = t.templates.as_slice()[0];
    assert_eq!((tm.id, tm.widget_name.as_str()), (7, Some("w")));
    let paths = t.paths_of(&tm);
    assert_eq!(paths.len(), 2);
    assert_eq!((paths[0].id, paths[0].fe_fmt, paths[0].be_fmt), (0, Some(0), Some(1)));
    assert_eq!((paths[1].id, paths[1].fe_fmt, paths[1].be_fmt), (1, Some(1), Some(0)));
    let p0 = t.pipelines_of(&paths[0]);
    assert_eq!(p0.len(), 2);
    assert_eq!((p0[0].id, p0[0].cfg, p0[0].num_bindings, p0[0].nbindings), (10, Some(0), 1, 1));
    assert_eq!(p0[0].bindings[0], 1);
    let m00 = t.modules_of(&p0[0]);
    assert_eq!(m00.len(), 2);
    assert_eq!((m00[0].id, m00[0].cfg_base, m00[0].in_fmt, m00[0].cfg_ext), (0, Some(0), Some(0), Some(0)));
    assert_eq!((m00[1].id, m00[1].cfg_base, m00[1].in_fmt, m00[1].cfg_ext), (1, Some(1), Some(1), Some(1)));
    let b: Vec<_> = t.bindings_of(&p0[0]).collect();
    assert_eq!(b.len(), 1);
    assert_eq!(b[0].target_path_tmpl_id, 3);
    let m01 = t.modules_of(&p0[1]);
    assert_eq!(m01.len(), 1);
    assert_eq!(m01[0].cfg_ext, Some(2));
    assert_eq!(t.bindings_of(&p0[1]).count(), 0);
    let p1 = t.pipelines_of(&paths[1]);
    assert_eq!(p1.len(), 1);
    assert_eq!(p1[0].id, 12);
    assert_eq!(t.modules_of(&p1[0]).len(), 1);
    assert_eq!(t.modules.len(), 4);
    assert_eq!(t.pipelines.len(), 3);
    assert_eq!(t.paths.len(), 2);
    // the module's kind comes from its cfg_ext UUID
    assert_eq!(t.kind_of(&m00[0]), Some(ModuleKind::Copier));
}

#[test]
fn unknown_tokens_are_ignored_like_the_older_topologies_name_token() {
    // token 1802 is not in this tokens.h but is in the recovery set's files; Linux skips tokens no parser lists
    let p = cat(&[tmpl_hdr(0, "name"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), module(0, 0, 0, 0), ints(BYTE, &[(9999, 1)])]);
    assert!(load_widget(&p).is_ok());
}

#[test]
fn a_template_needs_a_path_and_a_pipeline_needs_a_module() {
    assert_eq!(load_widget(&tmpl_hdr(0, "w")).err(), Some(TplgError::TemplateWithoutPath));
    // path whose header is missing: the first array is already a pipeline
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), path_hdr(1, 0, 0), pipeline_hdr(0, 0, 0)]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::PipelineWithoutModule));
    // pipeline header with no module
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0)]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::PipelineWithoutModule));
    // a path that is nothing but a pipeline section (no path header array first)
    let p = cat(&[tmpl_hdr(0, "w"), words(&[(AVS_TKN_PATH_ID_U32, 0)]), pipeline_hdr(0, 0, 0), module(0, 0, 0, 0)]);
    assert!(load_widget(&p).is_ok(), "a path with only its id token is a header");
}

#[test]
fn a_path_may_have_no_pipelines() {
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0)]);
    let t = load_widget(&p).unwrap();
    let tm = t.templates.as_slice()[0];
    let paths = t.paths_of(&tm);
    assert_eq!(paths.len(), 1);
    assert_eq!(t.pipelines_of(&paths[0]).len(), 0);
    assert_eq!(t.pipelines.len(), 0);
}

#[test]
fn pipeline_bindings_must_match_their_header_and_the_manifest() {
    let ppl = |n, refs: Vec<Vec<u8>>| {
        let mut v = vec![tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, n), module(0, 0, 0, 0)];
        v.extend(refs);
        cat(&v)
    };
    // header says 1 binding, the section has none (topology.c:1346-1347)
    assert_eq!(load_widget(&ppl(1, vec![])).err(), Some(TplgError::BindingCountMismatch { states: 1, found: 0 }));
    // header says 2, the section has 1
    assert_eq!(load_widget(&ppl(2, vec![binding_ref(0)])).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_PPL_BINDING_ID_U32, entry: 1 }));
    // an id beyond the manifest's 2 bindings
    assert_eq!(load_widget(&ppl(1, vec![binding_ref(2)])).err(), Some(TplgError::IndexOutOfRange { what: "binding", index: 2, count: 2 }));
    // the last valid index
    let t = load_widget(&ppl(2, vec![binding_ref(1), binding_ref(0)])).unwrap();
    let tm = t.templates.as_slice()[0];
    let p = t.pipelines_of(&t.paths_of(&tm)[0])[0];
    assert_eq!(&p.bindings[..2], &[1, 0]);
    assert_eq!(p.nbindings, 2);
    // binding section present but the header says 0: ignored like Linux (`kcalloc(0)`)
    let t = load_widget(&ppl(0, vec![binding_ref(0)])).unwrap();
    assert_eq!(t.pipelines.as_slice()[0].nbindings, 0);
    // more than the fixed storage
    let refs: Vec<Vec<u8>> = (0..9).map(|_| binding_ref(0)).collect();
    assert_eq!(load_widget(&ppl(9, refs)).err(), Some(TplgError::TooMany { what: "pipeline bindings", count: 9, max: 8 }));
}

#[test]
fn module_init_config_ids_follow_the_module_tokens() {
    let m = cat(&[words(&[(AVS_TKN_MOD_ID_U32, 0), (AVS_TKN_MOD_MODCFG_BASE_ID_U32, 0), (AVS_TKN_MOD_IN_AFMT_ID_U32, 0), (AVS_TKN_MOD_MODCFG_EXT_ID_U32, 0)]), ints(WORD, &[(AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32, 2)]), words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 11)]), words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 22)])]);
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), m]);
    let t = load_widget(&p).unwrap();
    let md = t.modules.as_slice()[0];
    assert_eq!((md.num_config_ids, md.nconfig_ids), (2, 2));
    assert_eq!(&md.config_ids[..2], &[11, 22]);
    // config-id arrays but no count
    let m = cat(&[words(&[(AVS_TKN_MOD_ID_U32, 0)]), words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 11)])]);
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), m]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::ConfigIdsWithoutCount));
    // count larger than the storage
    let m = cat(&[words(&[(AVS_TKN_MOD_ID_U32, 0), (AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32, 9)]), words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 11)])]);
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), m]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::TooMany { what: "module config ids", count: 9, max: 8 }));
    // count larger than the ids present
    let m = cat(&[words(&[(AVS_TKN_MOD_ID_U32, 0), (AVS_TKN_MOD_INIT_CONFIG_NUM_IDS_U32, 3)]), words(&[(AVS_TKN_MOD_INIT_CONFIG_ID_U32, 11)])]);
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), m]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::DictionaryTooShort { token: AVS_TKN_MOD_INIT_CONFIG_ID_U32, entry: 1 }));
}

#[test]
fn module_index_tokens_are_range_checked() {
    let bad = |tok: u32, v: u32| {
        let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), words(&[(AVS_TKN_MOD_ID_U32, 0), (tok, v)])]);
        load_widget(&p).err()
    };
    // small_manifest: 2 base cfgs, 2 formats, 3 ext cfgs, 0 nhlt configs, 1 pplcfg
    assert_eq!(bad(AVS_TKN_MOD_MODCFG_BASE_ID_U32, 2), Some(TplgError::IndexOutOfRange { what: "module config base", index: 2, count: 2 }));
    assert_eq!(bad(AVS_TKN_MOD_IN_AFMT_ID_U32, 2), Some(TplgError::IndexOutOfRange { what: "audio format", index: 2, count: 2 }));
    assert_eq!(bad(AVS_TKN_MOD_MODCFG_EXT_ID_U32, 3), Some(TplgError::IndexOutOfRange { what: "module config ext", index: 3, count: 3 }));
    assert_eq!(bad(AVS_TKN_MOD_NHLT_CONFIG_ID_U32, 0), Some(TplgError::IndexOutOfRange { what: "nhlt config", index: 0, count: 0 }));
    // pipeline config index
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 1, 0), module(0, 0, 0, 0)]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::IndexOutOfRange { what: "pipeline config", index: 1, count: 1 }));
    // path format indices
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 2, 0)]);
    assert_eq!(load_widget(&p).err(), Some(TplgError::IndexOutOfRange { what: "audio format", index: 2, count: 2 }));
    // byte-wide module tokens refuse values that do not fit (Linux truncates)
    assert_eq!(bad(AVS_TKN_MOD_KCONTROL_ID_U32, 256), Some(TplgError::ValueTooWide { token: AVS_TKN_MOD_KCONTROL_ID_U32, value: 256, bits: 8 }));
    assert_eq!(bad(AVS_TKN_MOD_CORE_ID_U8, 256), Some(TplgError::ValueTooWide { token: AVS_TKN_MOD_CORE_ID_U8, value: 256, bits: 8 }));
}

#[test]
fn module_scalars() {
    let p = cat(&[tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0), words(&[(AVS_TKN_MOD_ID_U32, 5), (AVS_TKN_MOD_CORE_ID_U8, 1), (AVS_TKN_MOD_PROC_DOMAIN_U8, 1), (AVS_TKN_MOD_KCONTROL_ID_U32, 200)])]);
    let t = load_widget(&p).unwrap();
    let m = t.modules.as_slice()[0];
    assert_eq!((m.id, m.core_id, m.domain, m.ctl_id), (5, 1, 1, 200));
    assert_eq!((m.cfg_base, m.in_fmt, m.cfg_ext, m.nhlt_config), (None, None, None, None), "unset pointers are NULL");
}

#[test]
fn storage_limits_are_named() {
    // 65 modules in one pipeline: the module pool holds 64
    let mut parts = vec![tmpl_hdr(0, "w"), path_hdr(0, 0, 0), pipeline_hdr(0, 0, 0)];
    for i in 0..65 {
        parts.push(module(i, 0, 0, 0));
    }
    assert_eq!(load_widget(&cat(&parts)).err(), Some(TplgError::TooMany { what: "modules", count: 65, max: 64 }));
    // 17 widget templates
    let w: Vec<u8> = (0..17).map(|i| widget(&format!("w{i}"), &cat(&[tmpl_hdr(i, "x"), path_hdr(0, 0, 0)]), &[])).flatten().collect();
    let f: &'static [u8] = Box::leak(cat(&[manifest_block(&small_manifest()), block(5, 17, &w)]).into_boxed_slice());
    assert_eq!(Topology::parse(f, None).err(), Some(TplgError::TooMany { what: "path templates", count: 17, max: 16 }));
    // 65 routes
    let r: Vec<(&str, &str, &str)> = (0..65).map(|_| ("a", "", "b")).collect();
    let f: &'static [u8] = Box::leak(cat(&[manifest_block(&small_manifest()), block(4, 65, &graph(&r))]).into_boxed_slice());
    assert_eq!(Topology::parse(f, None).err(), Some(TplgError::TooMany { what: "routes", count: 65, max: 64 }));
    let _ = SND_SOC_TPLG_TYPE_DAPM_GRAPH;
}

#[test]
fn the_topology_is_a_bounded_size() {
    assert!(core::mem::size_of::<Topology<'static>>() <= Topology::SIZE_BOUND, "{} bytes", core::mem::size_of::<Topology<'static>>());
}

#[test]
fn accessors_never_index_out_of_range() {
    let t = Topology::new();
    assert!(t.fmt(Some(0)).is_none());
    assert!(t.fmt(None).is_none());
    assert!(t.modcfg_base(Some(5)).is_none());
    assert!(t.modcfg_ext(Some(5)).is_none());
    assert!(t.pplcfg(Some(5)).is_none());
    let bogus = avs_topology_core::model::PathTemplate { first_path: 200, npaths: 5, ..Default::default() };
    assert!(t.paths_of(&bogus).is_empty());
    let bp = avs_topology_core::model::Path { first_pipeline: 200, npipelines: 5, ..Default::default() };
    assert!(t.pipelines_of(&bp).is_empty());
    let bpl = avs_topology_core::model::Pipeline { first_module: 200, nmodules: 5, ..Default::default() };
    assert!(t.modules_of(&bpl).is_empty());
}

#[test]
fn module_kinds_by_uuid() {
    use avs_ipc_core::modcfg::*;
    let all: [([u8; 16], ModuleKind); 15] = [
        (AVS_COPIER_MOD_UUID, ModuleKind::Copier),
        (AVS_MIXIN_MOD_UUID, ModuleKind::Mixin),
        (AVS_MIXOUT_MOD_UUID, ModuleKind::Mixout),
        (AVS_PEAKVOL_MOD_UUID, ModuleKind::Peakvol),
        (AVS_GAIN_MOD_UUID, ModuleKind::Gain),
        (AVS_MUX_MOD_UUID, ModuleKind::Mux),
        (AVS_UPDWMIX_MOD_UUID, ModuleKind::UpdownMix),
        (AVS_SRCINTC_MOD_UUID, ModuleKind::Src),
        (AVS_ASRC_MOD_UUID, ModuleKind::Asrc),
        (AVS_MICSEL_MOD_UUID, ModuleKind::Micsel),
        (AVS_AEC_MOD_UUID, ModuleKind::Aec),
        (AVS_KPBUFF_MOD_UUID, ModuleKind::Kpbuff),
        (AVS_INTELWOV_MOD_UUID, ModuleKind::IntelWov),
        (AVS_WOVHOSTM_MOD_UUID, ModuleKind::WovHostm),
        (AVS_PROBE_MOD_UUID, ModuleKind::Probe),
    ];
    for (u, k) in all {
        assert_eq!(module_kind(&u), k);
    }
    assert_eq!(module_kind(&[0; 16]), ModuleKind::Other);
    assert_eq!(module_kind(&[0xFF; 16]), ModuleKind::Other);
}
