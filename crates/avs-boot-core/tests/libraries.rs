// SPDX-License-Identifier: GPL-2.0-only
//! The library stage: loaded after the configuration queries (so the slot count is the firmware's), skipped when already resident,
//! bounded by the table, bracketed by the gating, and failing by name.
mod common;
use avs_boot_core::boot::*;
use avs_boot_core::config::LibImage;
use avs_boot_core::error::*;
use avs_fw_core::effect::Input as FwInput;
use avs_topology_core::model::Library;
use avs_topology_core::nhlt::Nhlt;
use avs_topology_core::topology::Topology;
use avs_topology_core::vendor::Name;
use common::*;

fn lib(name: &[u8; 8], len: u32) -> LibImage {
    LibImage { name: *name, image_len: len }
}

fn run(n_topo_libs: usize, images: Vec<LibImage>, fw: impl FnOnce(&mut Fw), setup: impl FnOnce(&mut Sys)) -> Option<(Result<avs_boot_core::running::Running, BootFailure>, Sys)> {
    let tb = tplg_bytes()?;
    let nb = nhlt_bytes();
    let mut topo = Topology::parse(&tb, None).unwrap();
    for i in 0..n_topo_libs {
        topo.libs.push(Library { name: Name::from_bytes(format!("lib{i}").as_bytes()).unwrap() });
    }
    let nhlt = Nhlt::parse(&nb, true).unwrap();
    let mut c = cfg(&topo, &nhlt);
    c.libs = images;
    let mut f = Fw::atlas(&topo);
    fw(&mut f);
    let mut sys = Sys::new(f);
    setup(&mut sys);
    let r = drive(&mut Boot::new(&c).unwrap(), &mut sys);
    Some((r, sys))
}

fn count(sys: &Sys, label: &str) -> usize {
    sys.trace.iter().filter(|t| matches!(t, T::Fw(l) if *l == label)).count()
}

#[test]
fn a_library_is_loaded_after_the_config_queries_and_before_gating_comes_back_on() {
    let Some((r, sys)) = run(1, vec![lib(b"LIBA\0\0\0\0", 20_000)], |_| {}, |_| {}) else { return };
    r.unwrap();
    // one LOAD_LIBRARY: the code loader sends and awaits it (loader.c:204-205)
    assert_eq!(count(&sys, "code-ipc-send"), 1);
    assert_eq!(count(&sys, "code-ipc-reply"), 1);
    let send = fw_pos(&sys, "code-ipc-send").unwrap();
    let fw_cfg = pos(&sys, |t| matches!(t, T::Ipc { name: "get fw config", .. })).unwrap();
    let off_last = fw_pos(&sys, "l1sen-off").unwrap();
    let on = fw_pos(&sys, "power-gating-on").unwrap();
    let mods = pos(&sys, |t| matches!(t, T::Ipc { name: "get modules info", .. })).unwrap();
    assert!(off_last < fw_cfg && fw_cfg < send && send < on && on < mods);
}

#[test]
fn a_library_whose_name_is_already_resident_is_not_loaded_twice() {
    let a = lib(b"LIBA\0\0\0\0", 20_000);
    let Some((r, sys)) = run(2, vec![a, a], |_| {}, |_| {}) else { return };
    r.unwrap();
    assert_eq!(count(&sys, "code-ipc-send"), 1, "loader.c:536-539 'Don't load anything that's already in DSP memory'");
    let Some((r, sys)) = run(2, vec![a, lib(b"LIBB\0\0\0\0", 9_000)], |_| {}, |_| {}) else { return };
    r.unwrap();
    assert_eq!(count(&sys, "code-ipc-send"), 2);
}

#[test]
fn the_library_images_must_match_the_topologys_table() {
    let Some((r, sys)) = run(1, vec![], |_| {}, |_| {}) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.reason), (Stage::Libraries, Reason::LibraryImagesMismatch { topology: 1, supplied: 0 }));
    assert_eq!(names(&sys), ["get hw config", "get fw config"]);
    assert!(f.gating_restored);
    let Some((r, _)) = run(0, vec![lib(b"LIBA\0\0\0\0", 1)], |_| {}, |_| {}) else { return };
    assert_eq!(r.unwrap_err().reason, Reason::LibraryImagesMismatch { topology: 0, supplied: 1 });
}

#[test]
fn the_firmwares_library_slots_bound_the_batch() {
    // loader.c:498-499: id + num_libs >= max_libs is refused, so one slot is always left free; slot 0 is the base firmware, so one library
    // needs max_libs >= 3
    let l = lib(b"LIBA\0\0\0\0", 20_000);
    let Some((r, _)) = run(1, vec![l], |f| f.max_libs = 3, |_| {}) else { return };
    r.unwrap();
    let Some((r, sys)) = run(1, vec![l], |f| f.max_libs = 2, |_| {}) else { return };
    let f = r.unwrap_err();
    assert_eq!(f.stage, Stage::Libraries);
    assert!(matches!(f.reason, Reason::FwSetup(_)));
    assert_eq!(count(&sys, "code-ipc-send"), 0);
    // more slots than a 4-bit library id can address is refused by the table
    let Some((r, _)) = run(1, vec![l], |f| f.max_libs = 17, |_| {}) else { return };
    assert!(matches!(r.unwrap_err().reason, Reason::FwSetup(_)));
}

#[test]
fn a_library_the_firmware_refuses_fails_by_name_and_the_gating_is_restored() {
    let Some((r, sys)) = run(1, vec![lib(b"LIBA\0\0\0\0", 20_000)], |_| {}, |s| s.fw_override.push(("code-ipc-reply", FwInput::IpcStatus(7)))) else { return };
    let f = r.unwrap_err();
    assert_eq!((f.stage, f.step), (Stage::Libraries, "code-ipc-reply"));
    assert!(matches!(f.reason, Reason::Fw(ff) if ff.cause == avs_fw_core::effect::Cause::IpcRefused { status: 7 }));
    assert!(f.gating_restored && f.dsp_dirty);
    assert!(fw_pos(&sys, "power-gating-on").unwrap() > fw_pos(&sys, "code-ipc-reply").unwrap());
    assert!(!names(&sys).contains(&"get modules info"), "nothing past the failed library");
}
