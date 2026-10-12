// SPDX-License-Identifier: GPL-2.0-only
//! The Pixelbook Go's REAL AVS topologies (ChromeOS recovery image, FIRMWARE-SHA256.tsv). Intel's/Google's blobs are not
//! committed; this reads `$CORAL_AVS_FW_DIR` (default: the owner's recovery tree) and says SKIPPED by name when absent.
//! Expected values were read out of the files with a throw-away Python dump (tuple arrays, widgets, graph) on 2026-10-12 and
//! are written out here by hand.
use avs_ipc_core::modcfg::*;
use avs_topology_core::blocks::ControlKind;
use avs_topology_core::model::*;
use avs_topology_core::topology::*;

const DEFAULT_DIR: &str = "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/lib/firmware/intel/avs";

fn read(name: &str) -> Option<Vec<u8>> {
    let d = std::env::var("CORAL_AVS_FW_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_string());
    let p = std::path::Path::new(&d).join(name);
    if !p.exists() {
        eprintln!("{name}: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return None;
    }
    Some(std::fs::read(p).unwrap())
}

#[test]
fn da7219_tplg_the_headset_codec_on_ssp1() {
    let Some(b) = read("da7219-tplg.bin") else { return };
    assert_eq!(b.len(), 4616);
    let t = Topology::parse(&b, None).unwrap();
    assert_eq!((t.header.name.as_str(), t.header.version), (Some("avs_kbl-da7219"), 0));
    assert_eq!(t.libs.len(), 0);

    // two audio formats: 48 kHz stereo, 16-bit and 32-bit-container/24-valid
    assert_eq!(t.fmts.len(), 2);
    let f = t.fmts.as_slice();
    assert_eq!((f[0].sampling_freq, f[0].bit_depth, f[0].channel_map, f[0].channel_config, f[0].interleaving, f[0].num_channels, f[0].valid_bit_depth, f[0].sample_type), (48000, 16, 4294967056, 1, 0, 2, 16, 0));
    assert_eq!((f[1].sampling_freq, f[1].bit_depth, f[1].num_channels, f[1].valid_bit_depth), (48000, 32, 2, 24));
    assert_eq!(f[0].channel_map, 0xFFFF_FF10, "left = 0, right = 1, the rest invalid");

    assert_eq!(t.modcfgs_base.len(), 3);
    let b3 = t.modcfgs_base.as_slice();
    assert_eq!((b3[0].cpc, b3[0].ibs, b3[0].obs, b3[0].is_pages), (12000, 192, 384, 1));
    assert_eq!((b3[1].cpc, b3[1].ibs, b3[1].obs, b3[1].is_pages), (12000, 384, 384, 1));
    assert_eq!((b3[2].cpc, b3[2].ibs, b3[2].obs, b3[2].is_pages), (12000, 384, 192, 1));

    // four copiers: host output, I2S out (SSP1), host input, I2S in (SSP1)
    assert_eq!(t.modcfgs_ext.len(), 4);
    let e = t.modcfgs_ext.as_slice();
    for x in e {
        assert_eq!(module_kind(&x.type_uuid), ModuleKind::Copier);
        assert_eq!(x.copier.dma_buffer_size, 768);
        assert_eq!(x.copier.feature_mask, 0);
        assert_eq!(x.npins, 0);
    }
    assert_eq!((e[0].copier.out_fmt, e[0].copier.dma_type, e[0].copier.vindex), (Some(0), AVS_DMA_HDA_HOST_INPUT, 0));
    assert_eq!((e[1].copier.out_fmt, e[1].copier.dma_type, e[1].copier.vindex), (Some(1), AVS_DMA_I2S_LINK_INPUT, 16));
    assert_eq!((e[2].copier.out_fmt, e[2].copier.dma_type, e[2].copier.vindex), (Some(1), AVS_DMA_HDA_HOST_OUTPUT, 0));
    assert_eq!((e[3].copier.out_fmt, e[3].copier.dma_type, e[3].copier.vindex), (Some(1), AVS_DMA_I2S_LINK_OUTPUT, 16));
    // vindex 16 = instance 1 (SSP1), time slot 0
    assert_eq!(e[1].copier.vindex >> 4, 1);
    assert_eq!(e[3].copier.vindex >> 4, 1);

    assert_eq!(t.pplcfgs.len(), 1);
    let p = t.pplcfgs.as_slice()[0];
    assert_eq!((p.req_size, p.priority, p.lp, p.attributes, p.trigger), (2, 0, false, 0, 0));

    // two bindings, both to this topology's own templates 1 (ssp1c_be) and 3 (ssp1p_be)
    assert_eq!(t.bindings.len(), 2);
    let bd = t.bindings.as_slice();
    assert_eq!((bd[0].target_tplg_name.as_str(), bd[0].target_path_tmpl_id, bd[0].target_ppl_id, bd[0].target_mod_id, bd[0].mod_id, bd[0].target_mod_pin, bd[0].mod_pin, bd[0].is_sink), (Some("avs_kbl-da7219"), 1, 0, 0, 0, 0, 0, 1));
    assert_eq!((bd[1].target_path_tmpl_id, bd[1].is_sink), (3, 0));

    assert_eq!(t.condpath_tmpls.len(), 0);
    assert_eq!((t.init_configs.len(), t.nhlt_configs.len()), (0, 0));

    // four widgets, one template each, one path/pipeline/module each
    assert_eq!(t.widgets_seen, 4);
    assert_eq!(t.templates.len(), 4);
    let names: Vec<_> = t.templates.as_slice().iter().map(|x| x.widget_name.as_str().unwrap()).collect();
    assert_eq!(names, ["ssp1c_fe", "ssp1c_be", "ssp1p_fe", "ssp1p_be"]);
    let want = [
        // (template id, pipeline num_bindings, module base cfg, module in fmt, module ext cfg, bindings)
        (0u32, 1u32, 2u16, 1u16, 0u16),
        (1, 0, 1, 1, 1),
        (2, 1, 0, 0, 2),
        (3, 0, 1, 1, 3),
    ];
    for (tm, w) in t.templates.as_slice().iter().zip(want) {
        assert_eq!(tm.id, w.0);
        let paths = t.paths_of(tm);
        assert_eq!(paths.len(), 1);
        assert_eq!((paths[0].id, paths[0].fe_fmt, paths[0].be_fmt), (0, Some(0), Some(0)));
        let ppls = t.pipelines_of(&paths[0]);
        assert_eq!(ppls.len(), 1);
        assert_eq!((ppls[0].id, ppls[0].cfg, ppls[0].num_bindings), (0, Some(0), w.1));
        let mods = t.modules_of(&ppls[0]);
        assert_eq!(mods.len(), 1);
        assert_eq!((mods[0].id, mods[0].cfg_base, mods[0].in_fmt, mods[0].cfg_ext), (0, Some(w.2), Some(w.3), Some(w.4)));
        assert_eq!((mods[0].core_id, mods[0].domain, mods[0].ctl_id, mods[0].nhlt_config, mods[0].nconfig_ids), (0, 0, 0, None, 0));
    }
    // front ends bind to back ends: ssp1c_fe -> binding 0 (template 1), ssp1p_fe -> binding 1 (template 3)
    let fe_c = t.template_by_widget(b"ssp1c_fe").unwrap();
    let ppl = t.pipelines_of(&t.paths_of(fe_c)[0])[0];
    assert_eq!(t.binding_ids_of(&ppl), &[0]);
    let fe_p = t.template_by_widget(b"ssp1p_fe").unwrap();
    let ppl = t.pipelines_of(&t.paths_of(fe_p)[0])[0];
    assert_eq!(t.binding_ids_of(&ppl), &[1]);
    // the headphone back end copier writes to SSP1 through the I2S output gateway at 32-bit container
    let be_p = t.template_by_widget(b"ssp1p_be").unwrap();
    let m = t.modules_of(&t.pipelines_of(&t.paths_of(be_p)[0])[0])[0];
    let x = t.modcfg_ext(m.cfg_ext).unwrap();
    assert_eq!(x.copier.dma_type, AVS_DMA_I2S_LINK_OUTPUT);
    assert_eq!(t.fmt(m.in_fmt).unwrap().bit_depth, 32);
    assert_eq!(t.fmt(x.copier.out_fmt).unwrap().valid_bit_depth, 24);

    assert_eq!(t.pcms.len(), 1);
    let pcm = t.pcms.as_slice()[0];
    assert_eq!((pcm.pcm_name.as_str(), pcm.dai_name.as_str(), pcm.pcm_id, pcm.dai_id, pcm.playback, pcm.capture, pcm.compress), (Some("Headset"), Some("Headset-dai"), 0, 0, true, true, false));

    let routes: Vec<_> = t.routes.as_slice().iter().map(|r| (r.sink.as_str().unwrap(), r.control.as_str().unwrap(), r.source.as_str().unwrap())).collect();
    assert_eq!(
        routes,
        [
            ("Headset-capture", "", "ssp1c_fe"),
            ("ssp1c_fe", "", "ssp1c_be"),
            ("ssp1c_be", "", "ssp1 Rx"),
            ("ssp1 Tx", "", "ssp1p_be"),
            ("ssp1p_be", "", "ssp1p_fe"),
            ("ssp1p_fe", "", "Headset-playback"),
        ]
    );
    assert_eq!(t.controls.len(), 0);
}

#[test]
fn dmic_tplg() {
    let Some(b) = read("dmic-tplg.bin") else { return };
    let t = Topology::parse(&b, None).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_dmic"));
    assert_eq!((t.fmts.len(), t.modcfgs_base.len(), t.modcfgs_ext.len(), t.pplcfgs.len(), t.bindings.len()), (2, 2, 8, 1, 1));
    // 3 modules per capture pipeline: copier (DMIC), peakvol-like and copier (host)
    assert_eq!(t.templates.len(), 2);
    assert_eq!(t.modules.len(), 8);
    let kinds: Vec<_> = t.modcfgs_ext.as_slice().iter().map(|e| module_kind(&e.type_uuid)).collect();
    assert_eq!(kinds.iter().filter(|k| **k == ModuleKind::Copier).count(), 6);
    assert_eq!(kinds.iter().filter(|k| **k == ModuleKind::Peakvol).count(), 2);
    assert_eq!(t.modcfgs_ext.as_slice()[0].copier.dma_type, AVS_DMA_DMIC_LINK_INPUT);
    assert_eq!(t.routes.len(), 3);
    assert_eq!(t.pcms.as_slice()[0].pcm_name.as_str(), Some("Digital Microphone"));
}

#[test]
fn hdmi_tplg() {
    let Some(b) = read("hda-8086280b-tplg.bin") else { return };
    let t = Topology::parse(&b, None).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_hdmi"));
    assert_eq!((t.fmts.len(), t.modcfgs_base.len(), t.modcfgs_ext.len(), t.pplcfgs.len(), t.bindings.len()), (1, 1, 2, 1, 3));
    assert_eq!((t.templates.len(), t.paths.len(), t.pipelines.len(), t.modules.len()), (6, 6, 6, 6));
    assert_eq!((t.pcms.len(), t.routes.len()), (3, 9));
    assert_eq!(t.modcfgs_ext.as_slice()[1].copier.dma_type, AVS_DMA_HDA_LINK_OUTPUT);
}

#[test]
fn max98373_tplg_parses_but_its_speaker_protection_is_data_for_later() {
    let Some(b) = read("max98373-tplg.bin") else { return };
    assert_eq!(b.len(), 7632);
    let t = Topology::parse(&b, None).unwrap();
    assert_eq!(t.header.name.as_str(), Some("avs_kbl-max98373"));
    // the two libraries the DSP must load for speaker protection
    assert_eq!(t.libs.len(), 2);
    assert_eq!(t.libs.as_slice()[0].name.as_str(), Some("dsp_lib_dsm_core_spt_release.bin"));
    assert_eq!(t.libs.as_slice()[1].name.as_str(), Some("dsp_lib_waves5_9_spt_release.bin"));
    assert_eq!((t.fmts.len(), t.modcfgs_base.len(), t.modcfgs_ext.len(), t.pplcfgs.len(), t.bindings.len()), (7, 7, 9, 2, 1));
    // two BYTES controls ("DSM data", "WAVES data"): mainline AVS refuses these, the recovery driver evidently does not
    assert_eq!(t.controls.len(), 2);
    let c = t.controls.as_slice();
    assert!(c.iter().all(|x| x.kind == ControlKind::Bytes));
    assert_eq!(c[0].name.as_str(), Some("DSM data"));
    assert_eq!(c[1].name.as_str(), Some("WAVES data"));
    assert_eq!((c[0].id, c[1].id), (1, 2));
    assert_eq!(c[0].max, 0x0010_0000);
    // the speaker front end has a pipeline of 5 modules including a DSM-library module and a mixer-like module
    let fe = t.template_by_widget(b"ssp0p_fe").unwrap();
    let ppls = t.pipelines_of(&t.paths_of(fe)[0]);
    assert_eq!(ppls.len(), 2);
    assert_eq!(t.modules_of(&ppls[0]).len(), 5);
    assert_eq!(t.modules_of(&ppls[1]).len(), 2);
    // modules refer to the kcontrols by id: ctl_id 2 and 1 (DSM/WAVES)
    let ctl_ids: Vec<u32> = t.modules_of(&ppls[0]).iter().map(|m| m.ctl_id).collect();
    assert_eq!(ctl_ids, [0, 2, 0, 1, 0]);
    // module core/domain overrides on the library modules
    let m = t.modules_of(&ppls[0]);
    assert_eq!((m[1].core_id, m[1].domain), (1, 1));
    assert_eq!((m[3].core_id, m[3].domain), (1, 1));
    // the DSM and Waves module configs have input and output pins
    let dsm = t.kind_of(&m[1]);
    assert_eq!(dsm, Some(ModuleKind::Other));
    let e = t.modcfg_ext(m[1].cfg_ext).unwrap();
    assert_eq!((e.num_input_pins, e.num_output_pins, e.npins), (1, 1, 2));
    // the last module of the speaker pipeline is the copier on the I2S output gateway (SSP0: the back-end templates carry no pipeline)
    let last = m[4];
    let x = t.modcfg_ext(last.cfg_ext).unwrap();
    assert_eq!(module_kind(&x.type_uuid), ModuleKind::Copier);
    assert_eq!(x.copier.dma_type, AVS_DMA_I2S_LINK_OUTPUT);
    let be = t.template_by_widget(b"ssp0p_be").unwrap();
    assert!(t.pipelines_of(&t.paths_of(be)[0]).is_empty());
    // the manifest's binding names the speaker FE pipeline 0 / module 3 as target and this template's module 1 as source
    let bd = t.bindings.as_slice()[0];
    assert_eq!((bd.target_tplg_name.as_str(), bd.target_path_tmpl_id, bd.target_ppl_id, bd.target_mod_id, bd.mod_id, bd.is_sink), (Some("avs_kbl-max98373"), 0, 0, 3, 1, 0));
}

#[test]
fn a_single_ssp_binding_changes_nothing_in_files_without_percent_d() {
    let Some(b) = read("da7219-tplg.bin") else { return };
    let t1 = Topology::parse(&b, None).unwrap();
    let t2 = Topology::parse(&b, Some(SspBinding { port: 1, tdm: 0 })).unwrap();
    assert_eq!(t1.header.name, t2.header.name);
    assert_eq!(t1.routes.as_slice(), t2.routes.as_slice());
    assert_eq!(t1.modcfgs_ext.as_slice(), t2.modcfgs_ext.as_slice(), "vindex was set by the topology already");
}

#[test]
fn every_prefix_of_a_real_topology_is_refused_without_panicking() {
    let Some(b) = read("da7219-tplg.bin") else { return };
    // cutting the file anywhere must produce a named error (or, at block boundaries, a smaller valid topology), never a panic
    let mut ok = 0;
    for n in (0..b.len()).step_by(7) {
        if Topology::parse(&b[..n], None).is_ok() {
            ok += 1;
        }
    }
    assert!(ok < 20, "{ok} prefixes parsed");
}

#[test]
fn a_flipped_byte_never_panics() {
    let Some(b) = read("da7219-tplg.bin") else { return };
    let mut bad = 0;
    for i in (0..b.len()).step_by(3) {
        let mut c = b.clone();
        c[i] ^= 0xFF;
        if Topology::parse(&c, None).is_err() {
            bad += 1;
        }
    }
    assert!(bad > 100, "{bad} of {} corruptions were refused", b.len() / 3);
}
