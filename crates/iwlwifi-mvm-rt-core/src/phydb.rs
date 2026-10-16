// SPDX-License-Identifier: GPL-2.0-only
//! The PHY database: the calibration results the INIT image reports (`CALIB_RES_NOTIF_PHY_DB`) kept so they can be
//! sent to the RT image (`PHY_DB_CMD`). Ported from Linux `iwl-phy-db.c:97-300` (`iwl_phy_db_get_section`,
//! `iwl_phy_db_set_section`, `iwl_send_phy_db_data`). The storage is a small fixed table: the firmware sends the
//! channel groups largest index first, the 9000 family has fewer than [`MAX_GROUPS`].
//!
//! A bound is named, not silent: a section bigger than [`MAX_SECTION`] or a group index at or over [`MAX_GROUPS`] is
//! refused (`PhyDbError`), where Linux allocates `chg_id + 1` entries without limit. The refusal is logged by the caller.

/// Section types. // iwl-phy-db.c:44-49
pub const CFG: u16 = 1;
pub const CALIB_NCH: u16 = 2;
pub const CALIB_CHG_PAPD: u16 = 4;
pub const CALIB_CHG_TXP: u16 = 5;
pub const MAX_TYPE: u16 = 6;

pub const MAX_GROUPS: usize = 16;
pub const MAX_SECTION: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyDbError {
    /// `pkt_len < sizeof(*phy_db_notif) + size`. // iwl-phy-db.c (iwl_phy_db_set_section): -EINVAL
    Short,
    /// `iwl_phy_db_get_section` returns NULL for types it does not know. // iwl-phy-db.c:86-102
    BadType(u16),
    /// CoralOS bound: a channel group id at or over [`MAX_GROUPS`].
    GroupOutOfRange(u16),
    /// CoralOS bound: a section longer than [`MAX_SECTION`] bytes.
    TooLarge(u16),
}

#[derive(Clone, Copy)]
struct Entry {
    size: u16,
    data: [u8; MAX_SECTION],
}

const EMPTY: Entry = Entry { size: 0, data: [0; MAX_SECTION] };

/// The stored sections. 2 + 2 * 16 entries of 1 KiB.
pub struct PhyDb {
    cfg: Entry,
    nch: Entry,
    papd: [Entry; MAX_GROUPS],
    txp: [Entry; MAX_GROUPS],
    /// `n_group_papd` / `n_group_txp`: largest index + 1 seen (the firmware sends the largest first). // iwl-phy-db.c:`iwl_phy_db_set_section`
    n_papd: u16,
    n_txp: u16,
}

impl Default for PhyDb {
    fn default() -> Self {
        Self::new()
    }
}

impl PhyDb {
    pub const fn new() -> PhyDb {
        PhyDb { cfg: EMPTY, nch: EMPTY, papd: [EMPTY; MAX_GROUPS], txp: [EMPTY; MAX_GROUPS], n_papd: 0, n_txp: 0 }
    }

    /// `iwl_phy_db_set_section` for a `CALIB_RES_NOTIF_PHY_DB` payload: `{ le16 type; le16 length; u8 data[] }`.
    pub fn set_section(&mut self, payload: &[u8]) -> Result<(), PhyDbError> {
        if payload.len() < CALIB_RES_PHY_DB_HDR {
            return Err(PhyDbError::Short);
        }
        let ty = u16::from_le_bytes([payload[0], payload[1]]);
        let size = u16::from_le_bytes([payload[2], payload[3]]) as usize;
        if payload.len() < CALIB_RES_PHY_DB_HDR + size {
            return Err(PhyDbError::Short);
        }
        if size > MAX_SECTION {
            return Err(PhyDbError::TooLarge(ty));
        }
        let data = &payload[CALIB_RES_PHY_DB_HDR..CALIB_RES_PHY_DB_HDR + size];
        let mut chg_id = 0u16;
        let (e, n): (&mut Entry, Option<&mut u16>) = match ty {
            CFG => (&mut self.cfg, None),
            CALIB_NCH => (&mut self.nch, None),
            CALIB_CHG_PAPD | CALIB_CHG_TXP => {
                chg_id = u16::from_le_bytes([*data.first().unwrap_or(&0), *data.get(1).unwrap_or(&0)]);
                if chg_id as usize >= MAX_GROUPS {
                    return Err(PhyDbError::GroupOutOfRange(chg_id));
                }
                if ty == CALIB_CHG_PAPD {
                    (&mut self.papd[chg_id as usize], Some(&mut self.n_papd))
                } else {
                    (&mut self.txp[chg_id as usize], Some(&mut self.n_txp))
                }
            }
            _ => return Err(PhyDbError::BadType(ty)),
        };
        e.data[..size].copy_from_slice(data);
        e.size = size as u16;
        if let Some(n) = n {
            *n = (*n).max(chg_id + 1);
        }
        Ok(())
    }

    /// The sections `iwl_send_phy_db_data` sends, in its order: CFG, CALIB_NCH (both even when empty), then every
    /// non-empty PAPD group `0..n_group_papd`, then every non-empty TXP group `0..n_group_txp`. Yields (type, bytes).
    /// // iwl-phy-db.c (iwl_send_phy_db_data, iwl_phy_db_send_all_channel_groups)
    pub fn send_order(&self) -> SendOrder<'_> {
        SendOrder { db: self, pos: 0 }
    }

    pub fn n_groups(&self) -> (u16, u16) {
        (self.n_papd, self.n_txp)
    }
}

const CALIB_RES_PHY_DB_HDR: usize = CALIB_RES_NOTIF_PHY_DB_HDR_LEN;
/// `sizeof(struct iwl_calib_res_notif_phy_db)` without `data[]`. // fw/api/cmdhdr.h:130-134
const CALIB_RES_NOTIF_PHY_DB_HDR_LEN: usize = 4;

/// The iterator of [`PhyDb::send_order`]: position 0 is CFG, 1 is CALIB_NCH, then PAPD groups, then TXP groups.
pub struct SendOrder<'a> {
    db: &'a PhyDb,
    pos: usize,
}

impl<'a> Iterator for SendOrder<'a> {
    type Item = (u16, &'a [u8]);
    fn next(&mut self) -> Option<Self::Item> {
        let db = self.db;
        loop {
            let p = self.pos;
            self.pos += 1;
            let e: (u16, &'a Entry) = match p {
                0 => (CFG, &db.cfg),
                1 => (CALIB_NCH, &db.nch),
                _ => {
                    let g = p - 2;
                    if g < db.n_papd as usize {
                        (CALIB_CHG_PAPD, &db.papd[g])
                    } else if g - (db.n_papd as usize) < db.n_txp as usize {
                        (CALIB_CHG_TXP, &db.txp[g - db.n_papd as usize])
                    } else {
                        return None;
                    }
                }
            };
            if p >= 2 && e.1.size == 0 {
                continue; // `if (!entry->size) continue;`
            }
            return Some((e.0, &e.1.data[..e.1.size as usize]));
        }
    }
}
