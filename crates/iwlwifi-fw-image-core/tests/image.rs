// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the image sections, the default calibration and the command-version table. Expected
//! values are written out by hand from the Linux lines cited beside each.

use iwlwifi_fw_image_core::calib::*;
use iwlwifi_fw_image_core::cmdver::*;
use iwlwifi_fw_image_core::sections::*;
use iwlwifi_fw_tlv_core::container::{IWL_TLV_UCODE_MAGIC, TLV_UCODE_HEADER_LEN};

fn blob(tlvs: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut b = vec![0u8; TLV_UCODE_HEADER_LEN];
    b[4..8].copy_from_slice(&IWL_TLV_UCODE_MAGIC.to_le_bytes());
    for (ty, data) in tlvs {
        b.extend_from_slice(&ty.to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        while b.len() % 4 != 0 {
            b.push(0);
        }
    }
    b
}

fn sec(offset: u32, data: &[u8]) -> Vec<u8> {
    let mut v = offset.to_le_bytes().to_vec();
    v.extend_from_slice(data);
    v
}

#[test]
fn the_tlv_type_numbers_are_the_linux_ones() {
    assert_eq!(IWL_UCODE_TLV_SEC_RT, 19); // fw/file.h:68
    assert_eq!(IWL_UCODE_TLV_SEC_INIT, 20); // fw/file.h:69
    assert_eq!(IWL_UCODE_TLV_SEC_WOWLAN, 21); // fw/file.h:70
    assert_eq!(IWL_UCODE_TLV_DEF_CALIB, 22); // fw/file.h:71
    assert_eq!(IWL_UCODE_TLV_SECURE_SEC_RT, 24); // fw/file.h:73
    assert_eq!(IWL_UCODE_TLV_SECURE_SEC_INIT, 25); // fw/file.h:74
    assert_eq!(IWL_UCODE_TLV_SECURE_SEC_WOWLAN, 26); // fw/file.h:75
    assert_eq!(IWL_UCODE_TLV_CMD_VERSIONS, 48); // fw/file.h:88
    assert_eq!(CPU1_CPU2_SEPARATOR_SECTION, 0xFFFFCCCC); // fw/file.h:580
    assert_eq!(PAGING_SEPARATOR_SECTION, 0xAAAABBBB); // fw/file.h:581
    assert_eq!(IWL_FW_CMD_VER_UNKNOWN, 99); // fw/file.h:1027
    assert_eq!(LONG_GROUP, 1); // fw/api/commands.h:40
    assert_eq!(IWL_UCODE_TYPE_MAX, 4); // fw/img.h:30
    assert_eq!((UcodeType::Regular as u32, UcodeType::Init as u32, UcodeType::Wowlan as u32), (0, 1, 2)); // fw/img.h:26-28
}

#[test]
fn each_image_takes_exactly_its_own_section_tlvs_in_file_order() {
    let b = blob(&[
        (IWL_UCODE_TLV_SEC_RT, sec(0x100, b"rt0!")),
        (IWL_UCODE_TLV_SEC_INIT, sec(0x200, b"in0!")),
        (IWL_UCODE_TLV_SECURE_SEC_RT, sec(CPU1_CPU2_SEPARATOR_SECTION, b"")),
        (IWL_UCODE_TLV_SECURE_SEC_INIT, sec(0x300, b"in1!!")),
        (IWL_UCODE_TLV_SEC_WOWLAN, sec(0x400, b"wow")),
        (IWL_UCODE_TLV_SEC_RT, sec(0x500, b"rt1")),
    ]);
    let rt: Vec<_> = sections(&b, UcodeType::Regular).unwrap().map(|s| s.unwrap()).collect();
    assert_eq!(rt.iter().map(|s| s.offset).collect::<Vec<_>>(), [0x100, CPU1_CPU2_SEPARATOR_SECTION, 0x500]);
    assert_eq!(rt[0].data, b"rt0!");
    assert!(rt[1].data.is_empty() && rt[1].is_separator());
    let init: Vec<_> = sections(&b, UcodeType::Init).unwrap().map(|s| s.unwrap()).collect();
    assert_eq!(init.iter().map(|s| (s.offset, s.data)).collect::<Vec<_>>(), [(0x200, &b"in0!"[..]), (0x300, &b"in1!!"[..])]);
    let wow: Vec<_> = sections(&b, UcodeType::Wowlan).unwrap().map(|s| s.unwrap()).collect();
    assert_eq!(wow.len(), 1);
    assert_eq!(wow[0].offset, 0x400);
}

#[test]
fn a_section_tlv_shorter_than_its_offset_word_is_refused_by_name_and_ends_the_walk() {
    let b = blob(&[(IWL_UCODE_TLV_SEC_RT, vec![1, 2, 3]), (IWL_UCODE_TLV_SEC_RT, sec(8, b"x"))]);
    let mut it = sections(&b, UcodeType::Regular).unwrap();
    assert_eq!(it.next().unwrap(), Err(ImageError::SectionTooShort { tlv_len: 3 }));
    assert!(it.next().is_none(), "no section is yielded past a refusal");
}

#[test]
fn a_broken_container_is_the_containers_error_not_a_short_list() {
    let mut b = blob(&[(IWL_UCODE_TLV_SEC_RT, sec(8, b"x"))]);
    b[4] ^= 0xFF; // the magic
    assert!(matches!(sections(&b, UcodeType::Regular), Err(ImageError::Container(_))));
}

#[test]
fn num_sections_counts_to_the_next_separator_of_either_kind() {
    let s = |o| Section { offset: o, data: b"d" };
    let list = [s(1), s(2), s(CPU1_CPU2_SEPARATOR_SECTION), s(3), s(PAGING_SEPARATOR_SECTION), s(4)];
    assert_eq!(num_sections(&list, 0), 2); // internal.h:857-861
    assert_eq!(num_sections(&list, 3), 1);
    assert_eq!(num_sections(&list, 5), 1);
    assert_eq!(num_sections(&list, 2), 0, "starting on a separator counts nothing");
    assert_eq!(num_sections(&list, 99), 0, "past the end counts nothing");
}

#[test]
fn only_the_two_separator_offsets_are_separators() {
    for (o, sep) in [(CPU1_CPU2_SEPARATOR_SECTION, true), (PAGING_SEPARATOR_SECTION, true), (0, false), (0xFFFF_CCCD, false), (0xAAAA_BBBA, false)] {
        assert_eq!(Section { offset: o, data: b"" }.is_separator(), sep, "{o:#x}");
    }
}

fn calib(ty: u32, flow: u32, event: u32) -> Vec<u8> {
    [ty, flow, event].iter().flat_map(|w| w.to_le_bytes()).collect()
}

#[test]
fn default_calib_takes_the_last_entry_for_the_asked_image() {
    let b = blob(&[
        (IWL_UCODE_TLV_DEF_CALIB, calib(1, 0x11, 0x12)),
        (IWL_UCODE_TLV_DEF_CALIB, calib(0, 0x21, 0x22)),
        (IWL_UCODE_TLV_DEF_CALIB, calib(1, 0x31, 0x32)), // iwl-drv.c:532-535 overwrites
    ]);
    assert_eq!(default_calib(&b, UcodeType::Init), Ok(Some(CalibCtrl { flow_trigger: 0x31, event_trigger: 0x32 })));
    assert_eq!(default_calib(&b, UcodeType::Regular), Ok(Some(CalibCtrl { flow_trigger: 0x21, event_trigger: 0x22 })));
    assert_eq!(default_calib(&b, UcodeType::Wowlan), Ok(None));
}

#[test]
fn default_calib_refuses_a_wrong_length_or_a_ucode_type_past_the_table() {
    let bad_len = blob(&[(IWL_UCODE_TLV_DEF_CALIB, vec![0; 8])]);
    assert_eq!(default_calib(&bad_len, UcodeType::Init), Err(CalibError::InvalidTlvLen { len: 8 })); // iwl-drv.c:1015
    let bad_ty = blob(&[(IWL_UCODE_TLV_DEF_CALIB, calib(4, 1, 2))]);
    assert_eq!(default_calib(&bad_ty, UcodeType::Init), Err(CalibError::WrongUcodeType { ucode_type: 4 })); // iwl-drv.c:527-530
    let max_ok = blob(&[(IWL_UCODE_TLV_DEF_CALIB, calib(3, 1, 2))]);
    assert_eq!(default_calib(&max_ok, UcodeType::Init), Ok(None), "type 3 < TYPE_MAX is accepted");
}

fn cv(entries: &[[u8; 4]]) -> Vec<u8> {
    entries.iter().flatten().copied().collect()
}

#[test]
fn cmd_ver_lookup_maps_group_zero_to_long_group_but_notif_ver_does_not() {
    // entries: cmd, group, cmd_ver, notif_ver
    let b = blob(&[(IWL_UCODE_TLV_CMD_VERSIONS, cv(&[[0x01, 0x00, 7, 3], [0x0d, 0x01, 8, 2], [0x05, 0x04, 99, 99]]))]);
    let t = cmd_versions(&b).unwrap();
    assert_eq!(t.len(), 3);
    assert_eq!(t.cmd_ver(1, 0x0d, 0), 8, "LONG_GROUP command");
    assert_eq!(t.cmd_ver(0, 0x0d, 0), 8, "group 0 is looked up under LONG_GROUP (fw/img.c:14)");
    assert_eq!(t.cmd_ver(0, 0x01, 42), 42, "an entry recorded under group 0 is never found by a group-0 lookup");
    assert_eq!(t.notif_ver(0, 0x01, 0), 3, "notif_ver keeps the group (fw/img.c:34-48)");
    assert_eq!(t.notif_ver(1, 0x0d, 0), 2);
    assert_eq!(t.cmd_ver(4, 0x05, 5), 5, "99 is UNKNOWN: the default");
    assert_eq!(t.notif_ver(4, 0x05, 6), 6);
    assert_eq!(t.cmd_ver(9, 9, 1), 1, "absent: the default");
}

#[test]
fn a_trailing_partial_entry_is_dropped_and_a_second_table_is_refused() {
    let mut v = cv(&[[1, 1, 5, 5]]);
    v.extend_from_slice(&[9, 9]);
    let b = blob(&[(IWL_UCODE_TLV_CMD_VERSIONS, v.clone())]);
    assert_eq!(cmd_versions(&b).unwrap().len(), 1); // iwl-drv.c:1376-1382
    let two = blob(&[(IWL_UCODE_TLV_CMD_VERSIONS, v.clone()), (IWL_UCODE_TLV_CMD_VERSIONS, v)]);
    assert_eq!(cmd_versions(&two), Err(CmdVerError::Duplicate)); // iwl-drv.c:1383-1384
    let none = blob(&[]);
    assert!(cmd_versions(&none).unwrap().is_empty());
    assert_eq!(cmd_versions(&none).unwrap().cmd_ver(1, 1, 77), 77);
}

/// The real -46 blob, when pointed at (the same convention as iwlwifi-fw-tlv-core's real_blob test).
#[test]
fn the_9260_46_blob_when_pointed_at() {
    let Ok(path) = std::env::var("CORAL_IWL_UCODE") else {
        eprintln!("the_9260_46_blob_when_pointed_at: SKIPPED (set CORAL_IWL_UCODE=<decompressed -46 blob>)");
        return;
    };
    let b = std::fs::read(&path).unwrap();
    for ty in [UcodeType::Init, UcodeType::Regular] {
        let secs: Vec<_> = sections(&b, ty).unwrap().map(|s| s.unwrap()).collect();
        eprintln!("{ty:?}: {:?}", secs.iter().map(|s| (s.offset, s.data.len())).collect::<Vec<_>>());
    }
    let t = cmd_versions(&b).unwrap();
    eprintln!("cmd versions: {}", t.len());
    eprintln!("alive notif_ver {} scan cmd_ver {}", t.notif_ver(0, 1, 255), t.cmd_ver(1, 0x0d, 255));
    eprintln!("phy_cfg cmd_ver {} nvm_access {} tx_ant {}", t.cmd_ver(0, 0x6a, 255), t.cmd_ver(0, 0x88, 255), t.cmd_ver(0, 0x98, 255));
    eprintln!("calib init {:?} regular {:?}", default_calib(&b, UcodeType::Init), default_calib(&b, UcodeType::Regular));
}
