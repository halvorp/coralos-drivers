// SPDX-License-Identifier: GPL-2.0-only
//! Decoder vectors. Ported from Linux `fw/api/nvm-reg.h:125-130`, `fw/api/cmdhdr.h:130-134`,
//! `mvm/nvm.c:100-104`, `iwl-phy-db.c:43-50,86-104,144-215`. Copyright Intel Corporation.

use iwlwifi_mvm_init_core::decode::*;

#[test]
fn nvm_resp_layout_is_offset_length_type_status_data() {
    // nvm-reg.h:125-130: le16 offset, length, type, status, then data.
    let p = [0x00, 0x08, 0x03, 0x00, 0x0c, 0x00, 0x00, 0x00, 0xAA, 0xBB, 0xCC, 0xDD];
    let r = decode_nvm_access_resp(&p).unwrap();
    assert_eq!(r.offset, 0x0800);
    assert_eq!(r.length, 3);
    assert_eq!(r.section, 12);
    assert_eq!(r.status, 0);
    assert_eq!(r.data, &[0xAA, 0xBB, 0xCC]); // exactly `length` bytes, the 4th is trailing
}

#[test]
fn nvm_resp_status_is_the_fourth_field() {
    let p = [0, 0, 0, 0, 0, 0, 0x01, 0x00];
    assert_eq!(decode_nvm_access_resp(&p).unwrap().status, 1);
    let p = [0, 0, 0, 0, 0, 0, 0x02, 0x01];
    assert_eq!(decode_nvm_access_resp(&p).unwrap().status, 0x0102);
}

#[test]
fn nvm_resp_refuses_short_and_lying_input() {
    assert_eq!(decode_nvm_access_resp(&[]), Err(DecodeError::ShortHeader { need: 8, have: 0 }));
    assert_eq!(decode_nvm_access_resp(&[0; 7]), Err(DecodeError::ShortHeader { need: 8, have: 7 }));
    // success status, length claims 4, only 3 follow
    let p = [0, 0, 4, 0, 0, 0, 0, 0, 1, 2, 3];
    assert_eq!(decode_nvm_access_resp(&p), Err(DecodeError::ShortData { claimed: 4, have: 3 }));
    // garbage length 0xffff with an error status carries no data Linux reads: accepted, empty data
    let p = [0, 0, 0xff, 0xff, 0, 0, 1, 0];
    let r = decode_nvm_access_resp(&p).unwrap();
    assert_eq!((r.status, r.length, r.data.len()), (1, 0xffff, 0));
}

#[test]
fn phy_db_entry_header_and_data() {
    // cmdhdr.h:130-134: le16 type, le16 length, data. One entry per notification.
    let p = [0x01, 0x00, 0x02, 0x00, 0x11, 0x22, 0x33];
    let e = decode_calib_res_phy_db(&p).unwrap();
    assert_eq!(e.ty, 1);
    assert_eq!(e.section, PhyDbSection::Cfg);
    assert_eq!(e.data, &[0x11, 0x22]); // the third byte is trailing, ignored (iwl-phy-db.c:161)
    assert_eq!(e.chg_id, None);
}

#[test]
fn phy_db_every_section_type_classifies() {
    // iwl-phy-db.c:44-48, :89-104: 1 CFG, 2 NCH, 4 PAPD, 5 TXP are valid; 0, 3, 6.. are not.
    let ok = |ty: u8, data: &[u8]| {
        let mut p = vec![ty, 0, data.len() as u8, 0];
        p.extend_from_slice(data);
        decode_calib_res_phy_db(&p).map(|e| (e.section, e.chg_id))
    };
    assert_eq!(ok(1, &[9]).unwrap().0, PhyDbSection::Cfg);
    assert_eq!(ok(2, &[9]).unwrap().0, PhyDbSection::CalibNch);
    assert_eq!(ok(4, &[7, 0]).unwrap().0, PhyDbSection::ChgPapd);
    assert_eq!(ok(5, &[8, 0]).unwrap().0, PhyDbSection::ChgTxp);
    assert_eq!(ok(4, &[7, 0]).unwrap().1, Some(7)); // iwl-phy-db.c:168
    assert_eq!(ok(5, &[0x34, 0x12, 0xff]).unwrap().1, Some(0x1234)); // le16, :182
    for bad in [0u8, 3, 6, 7, 0xff] {
        assert_eq!(ok(bad, &[1, 2]), Err(DecodeError::UnknownPhyDbType { ty: bad as u16 }));
    }
    // 16-bit type: 0x0101 is not type 1
    let p = [0x01, 0x01, 0, 0];
    assert_eq!(decode_calib_res_phy_db(&p), Err(DecodeError::UnknownPhyDbType { ty: 0x0101 }));
}

#[test]
fn phy_db_refuses_short_and_lying_input() {
    assert_eq!(decode_calib_res_phy_db(&[]), Err(DecodeError::ShortHeader { need: 4, have: 0 }));
    assert_eq!(decode_calib_res_phy_db(&[1, 0, 0]), Err(DecodeError::ShortHeader { need: 4, have: 3 }));
    // iwl-phy-db.c:161: pkt_len < sizeof + size
    assert_eq!(
        decode_calib_res_phy_db(&[1, 0, 5, 0, 1, 2, 3, 4]),
        Err(DecodeError::ShortData { claimed: 5, have: 4 })
    );
    // CoralOS bound: PAPD/TXP need 2 bytes for the group id.
    assert_eq!(
        decode_calib_res_phy_db(&[4, 0, 1, 0, 9]),
        Err(DecodeError::PhyDbGroupIdMissing { ty: 4, length: 1 })
    );
    assert_eq!(
        decode_calib_res_phy_db(&[5, 0, 0, 0]),
        Err(DecodeError::PhyDbGroupIdMissing { ty: 5, length: 0 })
    );
    // a zero-length CFG entry is valid
    assert!(decode_calib_res_phy_db(&[1, 0, 0, 0]).unwrap().data.is_empty());
}
