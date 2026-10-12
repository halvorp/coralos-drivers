// SPDX-License-Identifier: GPL-2.0-only
//! The Pixelbook Go's REAL AVS firmware (ChromeOS recovery image, FIRMWARE-SHA256.tsv): base firmware and libraries.
//! Intel's blobs are redistributable only unmodified under their own licence, so NOTHING is committed here; the test reads
//! `$CORAL_AVS_FW_DIR` (default: the owner's recovery tree) and says SKIPPED by name when the files are absent.
use avs_fw_core::bdl::*;
use avs_fw_core::boot::*;
use avs_fw_core::effect::*;
use avs_fw_core::error::*;
use avs_fw_core::library::*;
use avs_fw_core::manifest::*;

mod common;

const DEFAULT_DIR: &str = "/home/peter/Projects/coral_handoffs/atlas/recovery/configs/lib/firmware/intel/avs";

fn dir() -> Option<std::path::PathBuf> {
    let d = std::env::var("CORAL_AVS_FW_DIR").unwrap_or_else(|_| DEFAULT_DIR.to_string());
    let p = std::path::PathBuf::from(d);
    if p.join("skl").join("dsp_basefw.bin").exists() {
        Some(p)
    } else {
        None
    }
}

fn skl(d: &std::path::Path, f: &str) -> Vec<u8> {
    std::fs::read(d.join("skl").join(f)).unwrap_or_else(|e| panic!("{f}: {e}"))
}

#[test]
fn the_pixelbook_gos_base_firmware() {
    let Some(d) = dir() else {
        eprintln!("the_pixelbook_gos_base_firmware: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return;
    };
    let img = skl(&d, "dsp_basefw.bin");
    assert_eq!(img.len(), 243_016, "not the recovery image's dsp_basefw.bin"); // FIRMWARE-SHA256.tsv
    let s = strip_verify(&img, ManifestFamily::Skl, Some(skl_min_version()), VersionPolicy::Enforce).unwrap();
    // measured 2026-10-12 with od(1): ext manifest $AE1 of 0x548 bytes, Skylake magic 6 at 0x548, manifest "$AM1" at 0x548+0x284
    let e = s.ext.unwrap();
    assert_eq!((e.id, e.len, e.version_major, e.version_minor, e.entries), (0x31454124, 0x548, 1, 0, 12));
    assert_eq!(s.stripped, 0x548);
    assert_eq!(s.body.len(), 243_016 - 0x548);
    assert_eq!(s.family, ManifestFamily::Skl);
    assert_eq!(s.manifest_offset, 0x284);
    assert_eq!(s.manifest.id, 0x314d4124); // "$AM1"
    assert_eq!(s.manifest.len, 0x34);
    assert_eq!(&s.manifest.name, b"ADSPFW\0\0");
    assert_eq!(s.manifest.preload_page_count, 0x3b);
    assert_eq!(s.manifest.img_flags, 0);
    assert_eq!(s.manifest.feature_mask, 0x1f);
    // PERIPHERALS-FROM-RECOVERY.md: "dsp_basefw header decodes to FW 9.21.0.4899"
    assert_eq!(s.manifest.version, FwVersion { major: 9, minor: 21, hotfix: 0, build: 4899 });
    assert_eq!(s.version_ignored, None);

    // a build older than the row's minimum is refused for this very file once the minimum is raised past it
    let newer = FwVersion { major: 9, minor: 21, hotfix: 0, build: 4900 };
    assert_eq!(strip_verify(&img, ManifestFamily::Skl, Some(newer), VersionPolicy::Enforce).unwrap_err(), FwError::FwVersionMismatch { found: s.manifest.version, min: newer });
    // and it is not an Apollo Lake image
    assert_eq!(strip_verify(&img, ManifestFamily::Apl, None, VersionPolicy::Enforce).unwrap_err(), FwError::ManifestFamilyMismatch { found: ManifestFamily::Skl, expected: ManifestFamily::Apl });

    // the boot plan for it: a 128 KiB ring is filled twice
    let l = ClDmaLayout::plan(&[Extent { addr: 0x1000_0000, len: 128 * 1024 }], 0x2000, 128 * 1024, 1).unwrap();
    let mut b = BaseFwBoot::new(&l, s.body.len() as u32).unwrap();
    let (log, out) = common::run(|i| b.next(i), common::ok_answer, 2000);
    assert_eq!(out, Output::Finished);
    let copied: u32 = log.iter().filter_map(|s| if let Op::CopyToDma { len, .. } = s.op { Some(len) } else { None }).sum();
    assert_eq!(copied as usize, s.body.len());
}

#[test]
fn the_two_libraries_and_the_library_table() {
    let Some(d) = dir() else {
        eprintln!("the_two_libraries_and_the_library_table: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return;
    };
    let dsm = skl(&d, "dsp_lib_dsm_core_spt_release.bin");
    let waves = skl(&d, "dsp_lib_waves5_9_spt_release.bin");
    let ds = strip_verify(&dsm, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap();
    let ws = strip_verify(&waves, ManifestFamily::Skl, None, VersionPolicy::Enforce).unwrap();
    // measured with od(1): extended manifests of 0xac and 0x98 bytes; manifest names DSMCORE and LIBMAXXA
    assert_eq!((ds.stripped, ws.stripped), (0xac, 0x98));
    assert_eq!(&ds.manifest.name, b"DSMCORE\0");
    assert_eq!(&ws.manifest.name, b"LIBMAXXA");
    assert_eq!(ds.manifest.version, FwVersion { major: 9, minor: 21, hotfix: 0, build: 4899 });
    assert_eq!(ws.manifest.version, FwVersion { major: 9, minor: 21, hotfix: 0, build: 4899 });

    // avs_dsp_load_libraries on a fresh table (da7219/max98373 topologies name these two libraries)
    let mut t = LibraryTable::new(16).unwrap();
    let batch = t.begin(2).unwrap();
    assert_eq!(t.decide(&ds.manifest.name), Decision::Load { lib_id: 1 });
    t.record(1, &ds.manifest.name);
    assert_eq!(t.decide(&ws.manifest.name), Decision::Load { lib_id: 2 });
    t.record(2, &ws.manifest.name);
    assert!(!t.nothing_new(batch));
    // loading the same two again loads nothing
    let again = t.begin(2).unwrap();
    assert_eq!(t.decide(&ds.manifest.name), Decision::AlreadyLoaded);
    assert!(t.nothing_new(again));
}

/// `guid_t` bytes of the canonical text `AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE` (first three fields little-endian).
fn guid_from_text(t: &str) -> [u8; 16] {
    let h: Vec<u8> = t.replace('-', "").as_bytes().chunks(2).map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap()).collect();
    let mut g = [0u8; 16];
    g[0..4].copy_from_slice(&[h[3], h[2], h[1], h[0]]);
    g[4..6].copy_from_slice(&[h[5], h[4]]);
    g[6..8].copy_from_slice(&[h[7], h[6]]);
    g[8..16].copy_from_slice(&h[8..16]);
    g
}

#[test]
fn module_file_names_name_files_that_exist_in_the_recovery_image() {
    let Some(d) = dir() else {
        eprintln!("module_file_names_name_files_that_exist_in_the_recovery_image: SKIPPED (set CORAL_AVS_FW_DIR=<.../intel/avs>)");
        return;
    };
    // the three dsp_mod_<uuid>.bin files FIRMWARE-SHA256.tsv lists under intel/avs/skl/
    for u in ["7CAD0808-AB10-CD23-EF45-12AB34CD56EF", "B489C2DE-0F96-42E1-8A2D-C25B5091EE49", "C75061F3-F2B2-4DCC-8F9F-82ABB4131E66"] {
        let mut b = [0u8; 96];
        let k = module_path(&guid_from_text(u), &mut b).unwrap();
        let rel = std::str::from_utf8(&b[..k]).unwrap();
        assert_eq!(rel, format!("intel/avs/skl/dsp_mod_{u}.bin"));
        // `rel` is "intel/avs/..." and the directory is ".../intel/avs": strip the shared prefix
        let file = d.join(rel.strip_prefix("intel/avs/").unwrap());
        assert!(file.exists(), "{} named by module_path does not exist", file.display());
    }
}
