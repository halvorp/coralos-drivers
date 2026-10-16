// SPDX-License-Identifier: GPL-2.0-only
//! The PHY database keeps what CALIB_RES_NOTIF_PHY_DB reported and replays it in iwl_send_phy_db_data's order.
use iwlwifi_mvm_rt_core::phydb::*;

fn notif(ty: u16, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&ty.to_le_bytes());
    p.extend_from_slice(&(data.len() as u16).to_le_bytes());
    p.extend_from_slice(data);
    p
}

fn group(g: u16, tail: &[u8]) -> Vec<u8> {
    let mut d = g.to_le_bytes().to_vec();
    d.extend_from_slice(tail);
    d
}

#[test]
fn replays_cfg_nch_then_papd_then_txp_groups_in_index_order_skipping_empty_ones() {
    let mut db = PhyDb::new();
    // the firmware sends the largest group index first (iwl-phy-db.c: "so we can use it to know how much we should allocate")
    db.set_section(&notif(CALIB_CHG_TXP, &group(2, &[0x22]))).unwrap();
    db.set_section(&notif(CALIB_CHG_TXP, &group(0, &[0x20]))).unwrap();
    db.set_section(&notif(CALIB_CHG_PAPD, &group(1, &[0x11]))).unwrap();
    db.set_section(&notif(CFG, &[0xc0, 0xc1])).unwrap();
    db.set_section(&notif(CALIB_NCH, &[0xd0])).unwrap();
    assert_eq!(db.n_groups(), (2, 3));
    let got: Vec<(u16, Vec<u8>)> = db.send_order().map(|(t, d)| (t, d.to_vec())).collect();
    assert_eq!(
        got,
        vec![
            (1, vec![0xc0, 0xc1]),
            (2, vec![0xd0]),
            (4, group(1, &[0x11])), // PAPD group 0 is empty: skipped
            (5, group(0, &[0x20])),
            // TXP group 1 is empty: skipped
            (5, group(2, &[0x22])),
        ]
    );
}

#[test]
fn an_empty_cfg_and_nch_are_still_sent() {
    let db = PhyDb::new();
    let got: Vec<(u16, usize)> = db.send_order().map(|(t, d)| (t, d.len())).collect();
    assert_eq!(got, vec![(1, 0), (2, 0)], "iwl_send_phy_db_data sends both before looking at the groups");
}

#[test]
fn a_later_notification_replaces_the_same_section() {
    let mut db = PhyDb::new();
    db.set_section(&notif(CFG, &[1, 2, 3])).unwrap();
    db.set_section(&notif(CFG, &[9])).unwrap();
    assert_eq!(db.send_order().next(), Some((1, &[9u8][..])));
}

#[test]
fn refusals_are_named() {
    let mut db = PhyDb::new();
    assert_eq!(db.set_section(&[1, 0, 5]), Err(PhyDbError::Short));
    assert_eq!(db.set_section(&[1, 0, 9, 0, 1, 2]), Err(PhyDbError::Short), "length says 9, 2 follow");
    assert_eq!(db.set_section(&notif(3, &[1])), Err(PhyDbError::BadType(3)), "IWL_PHY_DB_UNUSED");
    assert_eq!(db.set_section(&notif(6, &[1])), Err(PhyDbError::BadType(6)));
    assert_eq!(db.set_section(&notif(CALIB_CHG_PAPD, &group(16, &[]))), Err(PhyDbError::GroupOutOfRange(16)));
    assert_eq!(db.set_section(&notif(CFG, &vec![0u8; MAX_SECTION + 1])), Err(PhyDbError::TooLarge(CFG)));
    assert_eq!(db.n_groups(), (0, 0), "a refused section leaves no trace");
}
