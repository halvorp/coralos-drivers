// SPDX-License-Identifier: GPL-2.0-only
//! Everything that can be refused BEFORE a single pipeline exists: speakers, wrong gateways, missing firmware, bad topology, bad config.
//! Each test mutates the parsed real da7219 topology (or the scripted firmware) and asserts the NAMED refusal, its stage, and that the DSP
//! was asked for nothing past the three configuration queries.
mod common;
use avs_boot_core::boot::*;
use avs_boot_core::error::*;
use avs_boot_core::plan::PlanError;
use avs_boot_core::volume::*;
use avs_ipc_core::header::*;
use avs_ipc_core::modcfg::*;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::{ModuleKind, Topology};
use common::*;

/// Parse, let the test mutate, run to the end.
fn run_with(mutate: impl FnOnce(&mut Topology<'_>), fw: impl FnOnce(&mut Fw), cfgf: impl FnOnce(&mut avs_boot_core::config::BootConfig<'_>)) -> Option<(Result<avs_boot_core::running::Running, BootFailure>, Sys)> {
    let tb = tplg_bytes()?;
    let nb = nhlt_bytes();
    let mut topo = Topology::parse(&tb, None).unwrap();
    mutate(&mut topo);
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut f = Fw::atlas(&topo);
    fw(&mut f);
    let mut sys = Sys::new(f);
    let mut c = cfg(&topo, &nhlt);
    cfgf(&mut c);
    let r = match Boot::new(&c) {
        Ok(mut b) => drive(&mut b, &mut sys),
        Err(e) => Err(e),
    };
    Some((r, sys))
}

fn plan_err(r: Result<avs_boot_core::running::Running, BootFailure>) -> PlanError {
    let f = r.unwrap_err();
    assert_eq!(f.stage, Stage::PathPlan);
    assert_eq!(f.name(), "plan-refused");
    match f.reason {
        Reason::Plan(p) => p,
        other => panic!("{other:?}"),
    }
}

const QUERIES: [&str; 3] = ["get hw config", "get fw config", "get modules info"];

#[test]
fn a_back_end_on_ssp0_is_the_speaker_path_and_is_refused_before_any_pipeline() {
    // the topology's ssp1p_be copier is ext config 3 with virtual index 16 = instance 1; point it at SSP0 (instance 0), any time slot
    for vindex in [0x00u8, 0x01, 0x0f] {
        let Some((r, sys)) = run_with(|t| t.modcfgs_ext[3].copier.vindex = vindex, |_| {}, |_| {}) else { return };
        assert_eq!(plan_err(r), PlanError::SpeakerPathRefused { ssp_instance: 0 });
        assert_eq!(names(&sys), QUERIES, "the DSP was asked for its configuration and nothing else");
    }
}

#[test]
fn any_other_ssp_is_refused_by_name_too() {
    for inst in [2u8, 3, 15] {
        let Some((r, sys)) = run_with(|t| t.modcfgs_ext[3].copier.vindex = inst << 4, |_| {}, |_| {}) else { return };
        assert_eq!(plan_err(r), PlanError::SspInstanceNotAllowed { found: inst as u32, wanted: 1 });
        assert_eq!(names(&sys), QUERIES);
    }
}

#[test]
fn a_gateway_that_does_not_belong_to_its_role_is_refused() {
    // the front end's copier must be the host output; the back end's the I2S output
    let Some((r, _)) = run_with(|t| t.modcfgs_ext[2].copier.dma_type = AVS_DMA_I2S_LINK_OUTPUT, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::WrongGatewayForRole { dma_type: AVS_DMA_I2S_LINK_OUTPUT, role: "front end" });
    let Some((r, _)) = run_with(|t| t.modcfgs_ext[3].copier.dma_type = AVS_DMA_HDA_HOST_OUTPUT, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::WrongGatewayForRole { dma_type: AVS_DMA_HDA_HOST_OUTPUT, role: "back end" });
    // capture, DMIC and HDA-link gateways have no place in a playback path
    for dt in [AVS_DMA_HDA_HOST_INPUT, AVS_DMA_HDA_LINK_OUTPUT, AVS_DMA_HDA_LINK_INPUT, AVS_DMA_DMIC_LINK_INPUT, AVS_DMA_I2S_LINK_INPUT, 99] {
        let Some((r, _)) = run_with(|t| t.modcfgs_ext[3].copier.dma_type = dt, |_| {}, |_| {}) else { return };
        assert_eq!(plan_err(r), PlanError::GatewayNotAllowed { dma_type: dt }, "{dt}");
    }
}

#[test]
fn a_module_kind_the_path_does_not_build_is_refused_not_guessed() {
    let Some((r, _)) = run_with(|t| t.modcfgs_ext[2].type_uuid = AVS_MIXIN_MOD_UUID, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::ModuleKindNotSupported { kind: ModuleKind::Mixin });
}

#[test]
fn the_firmware_must_have_the_module_loaded() {
    let Some((r, sys)) = run_with(|_| {}, |f| f.modules.clear(), |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::ModuleNotInFirmware { uuid: AVS_COPIER_MOD_UUID });
    assert_eq!(names(&sys), QUERIES);
    let Some((r, _)) = run_with(|_| {}, |f| f.modules[0].loaded = false, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::ModuleNotResident { module_id: 1 });
}

#[test]
fn pipeline_and_instance_limits_come_from_the_firmware() {
    let Some((r, _)) = run_with(|_| {}, |f| f.max_ppl = 1, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::PipelinesExhausted { max: 1 });
    assert!(run_with(|_| {}, |f| f.max_ppl = 2, |_| {}).unwrap().0.is_ok(), "exactly two pipelines fit exactly two slots");
    let Some((r, _)) = run_with(|_| {}, |f| f.modules[0].max_inst = 1, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::ModuleInstancesExhausted { module_id: 1, max: 1 });
    // a firmware that allows no pipelines at all is refused at the configuration stage
    let Some((r, sys)) = run_with(|_| {}, |f| f.max_ppl = 0, |_| {}) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.name()), (Stage::ConfigQuery, "firmware-limit"));
    assert_eq!(names(&sys), ["get hw config", "get fw config"]);
}

#[test]
fn the_firmware_must_be_new_enough_and_have_the_ssp_port() {
    // the boundary: (9, 21, 0, 4732) is accepted (core.c:817), one build lower is not
    assert!(run_with(|_| {}, |f| f.version = (9, 21, 0, 4732), |_| {}).unwrap().0.is_ok());
    for v in [(9, 21, 0, 4731), (9, 20, 9, 9999), (8, 99, 9, 9999)] {
        let Some((r, sys)) = run_with(|_| {}, |f| f.version = v, |_| {}) else { return };
        let f = r.unwrap_err();
        assert_eq!(f.stage, Stage::ConfigQuery);
        assert_eq!(f.reason, Reason::FwVersionTooOld { have: v, need: (9, 21, 0, 4732) });
        assert_eq!(names(&sys), ["get hw config", "get fw config"]);
    }
    assert!(run_with(|_| {}, |f| f.version = (10, 0, 0, 0), |_| {}).unwrap().0.is_ok(), "newer is fine");
    // SSP1 needs a controller count of at least 2
    assert!(run_with(|_| {}, |f| f.ssp_ctrl_count = 2, |_| {}).unwrap().0.is_ok());
    let Some((r, _)) = run_with(|_| {}, |f| f.ssp_ctrl_count = 1, |_| {}) else { return };
    let f = r.unwrap_err();
    assert_eq!(f.reason, Reason::SspPortMissing { port: 1, ctrl_count: 1 });
}

#[test]
fn the_pcm_parameters_must_match_a_path_variant() {
    // da7219-tplg.bin has only 48 kHz stereo 16-bit variants (real_blob.rs)
    let Some((r, _)) = run_with(|_| {}, |_| {}, |c| c.playback.fe.rate = 44100) else { return };
    assert_eq!(plan_err(r), PlanError::NoVariant { widget: "front end" });
    let Some((r, _)) = run_with(|_| {}, |_| {}, |c| c.playback.be.valid_bits = 24) else { return };
    assert_eq!(plan_err(r), PlanError::NoVariant { widget: "back end" });
    let Some((r, _)) = run_with(|_| {}, |_| {}, |c| c.playback.fe_widget = b"nope") else { return };
    assert_eq!(plan_err(r), PlanError::TemplateNotFound { widget: "front end" });
    let Some((r, _)) = run_with(|_| {}, |_| {}, |c| c.playback.be_widget = b"ssp0p_be") else { return };
    assert_eq!(plan_err(r), PlanError::TemplateNotFound { widget: "back end" });
}

#[test]
fn a_missing_gateway_blob_is_refused_not_defaulted() {
    // an NHLT with only the speaker SSP0 render endpoint: SSP1 render has no format configuration (path.c:277-335 -ENOENT)
    let Some(tb) = tplg_bytes() else { return };
    let topo = Topology::parse(&tb, None).unwrap();
    let nb = only_speaker_table();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut sys = Sys::new(Fw::atlas(&topo));
    let c = cfg(&topo, &nhlt);
    let r = drive(&mut Boot::new(&c).unwrap(), &mut sys);
    assert!(matches!(plan_err(r), PlanError::Nhlt(_)));
    assert!(!names(&sys).contains(&"create pipeline"));
}

fn only_speaker_table() -> Vec<u8> {
    use avs_topology_core::nhlt::*;
    let dw = |x: u32| x.to_le_bytes();
    let mut wave = Vec::new();
    wave.extend_from_slice(&0xFFFEu16.to_le_bytes());
    wave.extend_from_slice(&4u16.to_le_bytes());
    wave.extend_from_slice(&48000u32.to_le_bytes());
    wave.extend_from_slice(&(48000u32 * 4 * 4).to_le_bytes());
    wave.extend_from_slice(&16u16.to_le_bytes());
    wave.extend_from_slice(&32u16.to_le_bytes());
    wave.extend_from_slice(&22u16.to_le_bytes());
    wave.extend_from_slice(&32u16.to_le_bytes());
    wave.extend_from_slice(&0xFu32.to_le_bytes());
    wave.extend_from_slice(&[0x01, 0, 0, 0, 0, 0, 0x10, 0, 0x80, 0, 0, 0xAA, 0, 0x38, 0x9B, 0x71]);
    wave.extend_from_slice(&dw(88));
    wave.extend_from_slice(&blob(88, MAX98373_SEED));
    let mut body = vec![ACPI_NHLT_LINKTYPE_SSP, 0];
    body.extend_from_slice(&0x8086u16.to_le_bytes());
    body.extend_from_slice(&ACPI_NHLT_DEVICEID_I2S.to_le_bytes());
    body.extend_from_slice(&1u16.to_le_bytes());
    body.extend_from_slice(&0x123u32.to_le_bytes());
    body.push(ACPI_NHLT_DEVICETYPE_CODEC);
    body.push(ACPI_NHLT_DIR_RENDER);
    body.push(0);
    body.extend_from_slice(&dw(2));
    body.extend_from_slice(&[0, 0]);
    body.push(1);
    body.extend(wave);
    body.push(0);
    let mut ep = dw(4 + body.len() as u32).to_vec();
    ep.extend(body);
    let mut v = Vec::new();
    v.extend_from_slice(b"NHLT");
    v.extend_from_slice(&[0u8; 4]);
    v.push(1);
    v.push(0);
    v.extend_from_slice(b"GOOGLE");
    v.extend_from_slice(b"ATLASMAX");
    v.extend_from_slice(&dw(0));
    v.extend_from_slice(b"CORL");
    v.extend_from_slice(&dw(1));
    v.push(1);
    v.extend(ep);
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&dw(n));
    let sum = v.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    v[9] = 0u8.wrapping_sub(sum);
    v
}

#[test]
fn modules_on_a_secondary_core_or_with_initial_configs_are_refused() {
    let Some((r, _)) = run_with(|t| t.modules[3].core_id = 1, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::SecondaryCore { core_id: 1 });
    let Some((r, _)) = run_with(|t| t.modules[2].nconfig_ids = 1, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::InitConfigsNotSupported);
}

#[test]
fn a_binding_that_cannot_be_resolved_is_refused_and_so_are_back_end_bindings() {
    // the front end's binding names template 3 (ssp1p_be); template 1 (ssp1c_be, the capture back end) was not created
    let Some((r, _)) = run_with(|t| t.bindings[1].target_path_tmpl_id = 1, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::BindingTargetMissing { what: "target path template was not created" });
    let Some((r, _)) = run_with(|t| t.bindings[1].target_ppl_id = 9, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::BindingTargetMissing { what: "target pipeline" });
    let Some((r, _)) = run_with(|t| t.bindings[1].target_mod_id = 9, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::BindingTargetMissing { what: "target module" });
    let Some((r, _)) = run_with(|t| t.bindings[1].mod_id = 9, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::BindingTargetMissing { what: "own module" });
    // the back end (pipeline of ssp1p_be, topology pipeline index 3) claims a binding
    let Some((r, _)) = run_with(|t| t.pipelines[3].nbindings = 1, |_| {}, |_| {}) else { return };
    assert_eq!(plan_err(r), PlanError::BackEndBindings);
}

// ---- the peakvol ceiling ------------------------------------------------------------------------------------------------------------
fn peakvol_fe(target: u32) -> impl FnOnce(&mut Topology<'_>) {
    move |t| {
        t.modcfgs_ext[2].type_uuid = AVS_PEAKVOL_MOD_UUID;
        t.modcfgs_ext[2].peakvol.target_volume = target;
    }
}

#[test]
fn a_peakvol_module_with_no_usable_ceiling_is_refused() {
    for declared in [0u32, 0x8000_0000, u32::MAX] {
        let Some((r, sys)) = run_with(peakvol_fe(declared), |_| {}, |_| {}) else { return };
        assert_eq!(plan_err(r), PlanError::PeakvolCeilingUnset { module_id: 2, declared });
        assert_eq!(names(&sys), QUERIES);
    }
}

#[test]
fn a_peakvol_module_starts_silent_and_its_ceiling_is_enforced() {
    let ceiling = 0x4000_0000;
    let Some((r, sys)) = run_with(peakvol_fe(ceiling), |_| {}, |_| {}) else { return };
    let run = r.unwrap();
    assert_eq!(run.peakvols.len(), 1);
    let pv = run.peakvols[0];
    assert_eq!((pv.module_id, pv.instance_id, pv.ceiling), (2, 0, ceiling));
    // the init parameter block: copier-free peakvol config = base (40 bytes) then one volume (channel_id, target_volume, ...); target 0
    let inits = msgs(&sys, "init instance");
    let (h, param) = inits[1];
    assert_eq!(MOD_MODULE_ID.get(h), 2);
    assert_eq!(param.len(), 40 + 24);
    assert_eq!(u32::from_le_bytes(param[40..44].try_into().unwrap()), u32::MAX, "all channels");
    assert_eq!(u32::from_le_bytes(param[44..48].try_into().unwrap()), 0, "initialised SILENT, not at unity as Linux does");
    // at the ceiling: one large-config message to module 2 instance 0, param id AVS_PEAKVOL_VOLUME
    let m = pv.set_volume(ceiling).unwrap();
    assert_eq!(m.len(), 1);
    assert_eq!((MOD_MODULE_ID.get(m[0].header), MOD_INSTANCE_ID.get(m[0].header)), (2, 0));
    assert_eq!(LARGE_PARAM_ID.get(m[0].header), AVS_PEAKVOL_VOLUME as u32);
    assert_eq!(u32::from_le_bytes(m[0].payload[4..8].try_into().unwrap()), ceiling);
    assert!(pv.set_volume(0).is_ok());
    // one above: refused by name, nothing built
    assert_eq!(pv.set_volume(ceiling + 1), Err(VolumeError::AboveCeiling { requested: ceiling + 1, ceiling }));
    assert_eq!(pv.set_volume(VOLUME_UNITY), Err(VolumeError::AboveCeiling { requested: VOLUME_UNITY, ceiling }));
}

#[test]
fn boot_new_refuses_a_bad_host_stream_before_touching_anything() {
    let Some(tb) = tplg_bytes() else { return };
    let nb = nhlt_bytes();
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut c = cfg(&topo, &nhlt);
    c.host.playback_stream = 7;
    let f = Boot::new(&c).err().unwrap();
    assert_eq!((f.stage, f.name(), f.dsp_dirty), (Stage::HostStream, "hda-refused", false));
    let mut c = cfg(&topo, &nhlt);
    c.host.format_bits = 12;
    assert!(matches!(Boot::new(&c).err().unwrap().reason, Reason::Hda(_)));
    let mut c = cfg(&topo, &nhlt);
    c.host.bdl_addr = 0x3000_0040;
    assert!(matches!(Boot::new(&c).err().unwrap().reason, Reason::Hda(hda_ext_core::error::HdaError::UnalignedDmaAddress { .. })));
    let mut c = cfg(&topo, &nhlt);
    c.playback.fe.rate = 47999;
    assert!(Boot::new(&c).is_err(), "no stream format for 47999 Hz");
}
