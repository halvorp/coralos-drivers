// SPDX-License-Identifier: GPL-2.0-only
//! The PROTECTED SPEAKER path (DECISIONS D17): the upstream DSM-free `max98373-tplg.bin` planned and booted against the scripted firmware.
//! A speaker path exists only when the caller names the gain ceiling; the ceiling is enforced in the plan (the peakvol handle) and the path
//! is refused by name if it could not be.
mod common;
use avs_boot_core::boot::*;
use avs_boot_core::config::BootConfig;
use avs_boot_core::error::*;
use avs_boot_core::plan::PlanError;
use avs_boot_core::volume::*;
use avs_topology_core::model::SspBinding;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use common::*;

/// The ceiling the device names (max98373-core `gain::CEILING_LINEAR`; restated here as a literal so the two crates do not depend on each other).
const CEILING: u32 = 539_423_503;

/// The speaker card is a machine with exactly one SSP and one TDM slot (SSP0, none): `%d` in the topology's names becomes `0` and the
/// copier's I2S gateway gets its virtual index (topology.c:398-422).
const SPEAKER_BINDING: SspBinding = SspBinding { port: 0, tdm: 0 };

fn speaker_cfg<'a>(topo: &'a Topology<'a>, nhlt: &'a Nhlt<'a>) -> BootConfig<'a> {
    let mut c = cfg(topo, nhlt);
    c.playback.fe_widget = SPEAKER_FE_WIDGET;
    c.playback.be_widget = SPEAKER_BE_WIDGET;
    c.playback.ssp_instance = 0;
    c.playback.speaker_ssp = &SPEAKER_SSP;
    c.playback.speaker_ceiling = Some(CEILING);
    c
}

fn run(edit: impl FnOnce(&mut BootConfig<'_>)) -> Option<(Result<avs_boot_core::running::Running, BootFailure>, Sys)> {
    let tb = speaker_tplg_bytes()?;
    let nb = nhlt_bytes();
    let topo = Topology::parse(&tb, Some(SPEAKER_BINDING)).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut sys = Sys::new(Fw::atlas(&topo));
    let mut c = speaker_cfg(&topo, &nhlt);
    edit(&mut c);
    let r = match Boot::new(&c) {
        Ok(mut b) => drive(&mut b, &mut sys),
        Err(e) => Err(e),
    };
    Some((r, sys))
}

fn plan_err(r: Result<avs_boot_core::running::Running, BootFailure>) -> PlanError {
    let f = r.unwrap_err();
    assert_eq!(f.stage, Stage::PathPlan);
    match f.reason {
        Reason::Plan(p) => p,
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_upstream_speaker_topology_boots_with_the_ceiling_named() {
    let Some((r, sys)) = run(|_| {}) else { return };
    let running = r.expect("the speaker path boots");
    assert_eq!(running.peakvols.len(), 1, "the topology's DSP Volume is the one volume stage");
    assert_eq!(running.peakvols[0].ceiling, CEILING, "the topology declares none, so the caller's ceiling is the ceiling");
    // the volume stage is initialised SILENT, whatever the ceiling is
    let init: Vec<_> = msgs(&sys, "init instance");
    assert!(init.len() >= 3, "host copier, peakvol, link-side copier and the SSP copier are initialised");
    // two pipelines: the back end (the SSP0 copier) and the front end (host copier, volume, copier)
    assert_eq!(running.pipelines.len(), 2);
}

#[test]
fn the_ceiling_is_enforced_by_the_handle_not_only_by_the_caller() {
    let Some((r, _)) = run(|_| {}) else { return };
    let h = r.unwrap().peakvols[0];
    assert!(h.set_volume(CEILING).is_ok());
    assert!(h.set_volume(0).is_ok());
    assert_eq!(h.set_volume(CEILING + 1).unwrap_err(), VolumeError::AboveCeiling { requested: CEILING + 1, ceiling: CEILING });
    assert!(h.set_volume(VOLUME_UNITY).is_err());
    assert!(h.set_volume(u32::MAX).is_err());
}

#[test]
fn without_the_ceiling_the_speaker_ssp_is_still_refused_by_name() {
    let Some((r, sys)) = run(|c| c.playback.speaker_ceiling = None) else { return };
    assert_eq!(plan_err(r), PlanError::SpeakerPathRefused { ssp_instance: 0 });
    assert_eq!(names(&sys), ["get hw config", "get fw config", "get modules info"], "refused before a pipeline exists");
}

#[test]
fn a_speaker_path_must_be_on_a_speaker_ssp() {
    // the board lists SSP3 as the amplifier link: this topology's back end is on SSP0
    let Some((r, _)) = run(|c| c.playback.speaker_ssp = &[3]) else { return };
    assert_eq!(plan_err(r), PlanError::NotASpeakerSsp { ssp_instance: 0 });
    // listed, but not the SSP the path was asked for
    let Some((r, _)) = run(|c| {
        c.playback.speaker_ssp = &[0, 1];
        c.playback.ssp_instance = 1;
    }) else {
        return;
    };
    assert_eq!(plan_err(r), PlanError::SspInstanceNotAllowed { found: 0, wanted: 1 });
    // an extra listed SSP changes nothing for the path that is on the right one
    let Some((r, _)) = run(|c| c.playback.speaker_ssp = &[0, 5]) else { return };
    assert!(r.is_ok());
}

#[test]
fn an_invalid_ceiling_is_refused() {
    for bad in [0u32, VOLUME_UNITY + 1, u32::MAX] {
        let Some((r, _)) = run(|c| c.playback.speaker_ceiling = Some(bad)) else { return };
        assert_eq!(plan_err(r), PlanError::SpeakerCeilingInvalid { ceiling: bad });
    }
    // unity itself is representable (and a caller is free to name it; the DEVICE names the DSM-free ceiling)
    let Some((r, _)) = run(|c| c.playback.speaker_ceiling = Some(VOLUME_UNITY)) else { return };
    assert_eq!(r.unwrap().peakvols[0].ceiling, VOLUME_UNITY);
}

#[test]
fn a_topology_declared_target_lowers_the_ceiling_and_never_raises_it() {
    let tb = match speaker_tplg_bytes() {
        Some(t) => t,
        None => return,
    };
    let nb = nhlt_bytes();
    for (declared, want) in [(1_000u32, 1_000u32), (CEILING, CEILING), (CEILING + 5, CEILING), (VOLUME_UNITY, CEILING)] {
        let mut topo = Topology::parse(&tb, Some(SPEAKER_BINDING)).unwrap();
        let k = topo.modcfgs_ext.iter().position(|e| avs_topology_core::topology::module_kind(&e.type_uuid) == avs_topology_core::topology::ModuleKind::Peakvol).unwrap();
        topo.modcfgs_ext[k].peakvol.target_volume = declared;
        let nhlt = Nhlt::parse(&nb, true).unwrap();
        let mut sys = Sys::new(Fw::atlas(&topo));
        let c = speaker_cfg(&topo, &nhlt);
        let r = drive(&mut Boot::new(&c).unwrap(), &mut sys).unwrap();
        assert_eq!(r.peakvols[0].ceiling, want, "declared {declared}");
    }
}

#[test]
fn a_speaker_path_with_no_volume_stage_is_refused() {
    // the headset topology has no peakvol module: named a speaker path (SSP1 listed as an amplifier link), nothing in the DSP could hold
    // its ceiling, so it is refused before any pipeline exists
    let Some(tb) = tplg_bytes() else { return };
    let nb = nhlt_bytes();
    let topo = Topology::parse(&tb, None).unwrap();
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut sys = Sys::new(Fw::atlas(&topo));
    let mut c = cfg(&topo, &nhlt);
    c.playback.speaker_ssp = &[1];
    c.playback.speaker_ceiling = Some(CEILING);
    let f = drive(&mut Boot::new(&c).unwrap(), &mut sys).unwrap_err();
    assert_eq!(f.stage, Stage::PathPlan);
    assert_eq!(f.reason, Reason::Plan(PlanError::SpeakerPathWithoutVolume));
    assert_eq!(names(&sys), ["get hw config", "get fw config", "get modules info"]);
}

#[test]
fn the_ssp0_copier_carries_the_max98373_nhlt_blob_not_the_da7219_s() {
    let Some((r, sys)) = run(|_| {}) else { return };
    r.unwrap();
    let blob = blob(88, MAX98373_SEED);
    let da = common::blob(88, DA7219_RENDER_SEED);
    let found = msgs(&sys, "init instance").iter().any(|(_, p)| p.windows(blob.len()).any(|w| w == blob.as_slice()));
    assert!(found, "the link copier's initialisation carries the SSP0 render configuration");
    assert!(!msgs(&sys, "init instance").iter().any(|(_, p)| p.windows(da.len()).any(|w| w == da.as_slice())));
}
