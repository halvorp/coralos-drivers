// SPDX-License-Identifier: GPL-2.0-only
//! The library table (loader.c:489-552) and the file names (loader.c:222-223, :509-510, :563).
use avs_fw_core::error::FwError;
use avs_fw_core::library::*;

fn n(s: &str) -> [u8; 8] {
    let mut b = [0u8; 8];
    b[..s.len()].copy_from_slice(s.as_bytes());
    b
}

#[test]
fn slot_zero_is_basefw_and_the_next_id_is_one() {
    // loader.c:719-720
    let t = LibraryTable::new(16).unwrap();
    assert_eq!(t.name(0), Some(b"BASEFW\0\0"));
    assert_eq!(t.next_id(), 1);
}

#[test]
fn a_new_library_takes_the_next_slot_and_a_known_one_is_skipped() {
    let mut t = LibraryTable::new(16).unwrap();
    let batch = t.begin(2).unwrap();
    assert_eq!(batch.start, 1);
    assert_eq!(t.decide(&n("DSMCORE")), Decision::Load { lib_id: 1 });
    t.record(1, &n("DSMCORE"));
    assert_eq!(t.decide(&n("LIBMAXXA")), Decision::Load { lib_id: 2 });
    t.record(2, &n("LIBMAXXA"));
    // loader.c:536-539
    assert_eq!(t.decide(&n("DSMCORE")), Decision::AlreadyLoaded);
    assert_eq!(t.decide(&n("LIBMAXXA")), Decision::AlreadyLoaded);
    assert_eq!(t.decide(&n("BASEFW")), Decision::AlreadyLoaded);
    assert_eq!(t.next_id(), 3);
    assert!(!t.nothing_new(batch));
}

#[test]
fn a_batch_that_loads_nothing_new_says_so() {
    // loader.c:551 `return start == id ? 1 : 0;`
    let mut t = LibraryTable::new(16).unwrap();
    t.record(1, &n("DSMCORE"));
    let batch = t.begin(1).unwrap();
    assert_eq!(batch.start, 2);
    assert_eq!(t.decide(&n("DSMCORE")), Decision::AlreadyLoaded);
    assert!(t.nothing_new(batch));
}

#[test]
fn the_table_leaves_one_slot_free_like_linux() {
    // loader.c:498: `if (id + num_libs >= adev->fw_cfg.max_libs_count) return -EINVAL;`
    let t = LibraryTable::new(4).unwrap(); // slots 0..3, slot 0 taken
    assert!(t.begin(2).is_ok()); // 1 + 2 = 3 < 4
    let e = t.begin(3).unwrap_err(); // 1 + 3 = 4 >= 4
    assert_eq!(e, FwError::LibraryTableFull { next_id: 1, wanted: 3, max_libs: 4 });
    assert_eq!(e.name(), "library-table-full");
    assert!(t.begin(0).is_ok());
}

#[test]
fn a_firmware_reporting_more_than_sixteen_slots_is_refused() {
    // lib_id is 4 bits wide (messages.h:82)
    assert!(LibraryTable::new(16).is_ok());
    assert_eq!(LibraryTable::new(17).unwrap_err(), FwError::LibraryIdWidth { max_libs: 17 });
}

#[test]
fn a_full_boot_forgets_every_library_but_basefw() {
    // loader.c:621-623
    let mut t = LibraryTable::new(16).unwrap();
    t.record(1, &n("DSMCORE"));
    t.record(2, &n("LIBMAXXA"));
    t.clear_for_full_boot();
    assert_eq!(t.next_id(), 1);
    assert_eq!(t.name(0), Some(b"BASEFW\0\0"));
    assert_eq!(t.decide(&n("DSMCORE")), Decision::Load { lib_id: 1 });
}

#[test]
fn name_compare_is_strncmp_over_eight_bytes() {
    assert!(lib_names_equal(b"ABCDEFGH", b"ABCDEFGH"));
    assert!(!lib_names_equal(b"ABCDEFGH", b"ABCDEFGX"));
    // equal up to the NUL: bytes after it are not compared
    assert!(lib_names_equal(b"AB\0XXXXX", b"AB\0YYYYY"));
    assert!(!lib_names_equal(b"AB\0XXXXX", b"ABC\0YYYY"));
    // a name that is a prefix of another is NOT equal
    assert!(!lib_names_equal(b"DSMCORE\0", b"DSMCOREX"));
}

#[test]
fn firmware_paths() {
    let mut b = [0u8; 64];
    let k = firmware_path(AVS_BASEFW_FILENAME, &mut b).unwrap();
    assert_eq!(&b[..k], b"intel/avs/skl/dsp_basefw.bin"); // loader.c:36-37, core.c:816, core.c:949
    let k = firmware_path("dsp_lib_dsm_core_spt_release.bin", &mut b).unwrap();
    assert_eq!(&b[..k], b"intel/avs/skl/dsp_lib_dsm_core_spt_release.bin");
    let mut tiny = [0u8; 10];
    assert_eq!(firmware_path("x.bin", &mut tiny), Err(FwError::NameBufferTooSmall { need: 13, have: 10 }));
    assert_eq!(AVS_ROOT_DIR, "intel/avs");
    assert_eq!(AVS_BASEFW_FILENAME, "dsp_basefw.bin");
    assert_eq!(SKL_SPEC_NAME, "skl");
}

/// `GUID_INIT(a, b, c, d0..d7)` as `guid_t.b[]` (include/linux/uuid.h): the first three fields little-endian.
fn guid(a: u32, b: u16, c: u16, d: [u8; 8]) -> [u8; 16] {
    let mut g = [0u8; 16];
    g[0..4].copy_from_slice(&a.to_le_bytes());
    g[4..6].copy_from_slice(&b.to_le_bytes());
    g[6..8].copy_from_slice(&c.to_le_bytes());
    g[8..].copy_from_slice(&d);
    g
}

#[test]
fn module_file_names_use_pul_ordering_and_uppercase() {
    let mut b = [0u8; 96];
    // AVS_PROBE_MOD_UUID (messages.h:645) <-> the file on the Pixelbook Go's recovery image
    let probe = guid(0x7CAD0808, 0xAB10, 0xCD23, [0xEF, 0x45, 0x12, 0xAB, 0x34, 0xCD, 0x56, 0xEF]);
    let k = module_path(&probe, &mut b).unwrap();
    assert_eq!(&b[..k], b"intel/avs/skl/dsp_mod_7CAD0808-AB10-CD23-EF45-12AB34CD56EF.bin");
    // AVS_COPIER_MOD_UUID (messages.h:621)
    let copier = guid(0x9BA00C83, 0xCA12, 0x4A83, [0x94, 0x3C, 0x1F, 0xA2, 0xE8, 0x2F, 0x9D, 0xDA]);
    let k = module_path(&copier, &mut b).unwrap();
    assert_eq!(&b[..k], b"intel/avs/skl/dsp_mod_9BA00C83-CA12-4A83-943C-1FA2E82F9DDA.bin");
    // 0x0F nibbles and zero fields keep their leading zeros
    let z = guid(0x0000_00F0, 0x000F, 0x0A00, [0, 1, 2, 3, 4, 5, 6, 7]);
    let k = module_path(&z, &mut b).unwrap();
    assert_eq!(&b[..k], b"intel/avs/skl/dsp_mod_000000F0-000F-0A00-0001-020304050607.bin");
    let mut tiny = [0u8; 20];
    assert!(matches!(module_path(&probe, &mut tiny), Err(FwError::NameBufferTooSmall { .. })));
    // exactly the right size is accepted, one less is refused
    let mut exact = [0u8; 62];
    assert_eq!(module_path(&probe, &mut exact), Ok(62));
    let mut short = [0u8; 61];
    assert!(module_path(&probe, &mut short).is_err());
}

#[test]
fn a_one_character_name_still_occupies_its_slot() {
    // the slot is free when its FIRST byte is NUL (loader.c:496 `lib_names[id][0] == '\0'`), not its second
    let mut t = LibraryTable::new(16).unwrap();
    t.record(1, &n("X"));
    assert_eq!(t.next_id(), 2);
}

#[test]
fn a_full_table_reports_max_libs_and_ignores_an_out_of_range_record() {
    let mut t = LibraryTable::new(16).unwrap();
    for i in 1..16 {
        t.record(i, &n("LIBXXXXX"));
    }
    assert_eq!(t.next_id(), 16);
    t.record(16, &n("OVERFLOW")); // no slot 16: ignored, not a panic
    assert_eq!(t.next_id(), 16);
    assert!(t.begin(1).is_err());
    assert_eq!(t.name(16), None);
}
