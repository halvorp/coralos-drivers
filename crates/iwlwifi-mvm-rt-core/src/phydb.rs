// SPDX-License-Identifier: GPL-2.0-only
//! The PHY database: the calibration results the INIT image reports (`CALIB_RES_NOTIF_PHY_DB`) kept so they can be
//! sent to the RT image (`PHY_DB_CMD`). Ported from Linux `iwl-phy-db.c:97-300` (`iwl_phy_db_get_section`,
//! `iwl_phy_db_set_section`, `iwl_send_phy_db_data`).
//!
//! Storage is allocated as Linux allocates it: a section holds exactly the bytes the firmware sent (`kmemdup`), and
//! the PAPD / TXP channel-group tables are sized from the FIRST group index seen (`kzalloc(chg_id + 1)`; the firmware
//! sends the largest index first). There is no CoralOS size cap: an earlier fixed 1 KiB entry refused every TXP
//! section the Atlas 9260 (fw 46) reported (`[IWL-PHYDB REFUSED TooLarge(5)]` x8), so the RT image ran with no TX
//! power calibration. An allocation failure is a named refusal (`NoMem`), as Linux returns `-ENOMEM`.

extern crate alloc;
use alloc::vec::Vec;

/// Section types. // iwl-phy-db.c:44-49
pub const CFG: u16 = 1;
pub const CALIB_NCH: u16 = 2;
pub const CALIB_CHG_PAPD: u16 = 4;
pub const CALIB_CHG_TXP: u16 = 5;
pub const MAX_TYPE: u16 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyDbError {
    /// `pkt_len < sizeof(*phy_db_notif) + size`. // iwl-phy-db.c (iwl_phy_db_set_section): -EINVAL
    Short,
    /// `iwl_phy_db_get_section` returns NULL for types it does not know. // iwl-phy-db.c:86-102
    BadType(u16),
    /// `iwl_phy_db_get_section` returns NULL for `chg_id >= n_group_*`: a group index past the table the first
    /// (largest) index sized. Carries (type, chg_id, n_groups). // iwl-phy-db.c:95-101
    GroupOutOfRange(u16, u16, u16),
    /// `kzalloc` / `kmemdup` failed: -ENOMEM. Carries (type, bytes asked).
    NoMem(u16, usize),
}

/// What [`PhyDb::set_section`] stored, for the caller's ring line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub ty: u16,
    /// The channel group (0 for CFG / CALIB_NCH).
    pub chg_id: u16,
    pub size: usize,
}

/// The stored sections.
#[derive(Default)]
pub struct PhyDb {
    cfg: Vec<u8>,
    nch: Vec<u8>,
    /// `calib_ch_group_papd` / `calib_ch_group_txp`; the length is `n_group_*`. Empty until the first group arrives.
    papd: Vec<Vec<u8>>,
    txp: Vec<Vec<u8>>,
}

fn dup(ty: u16, data: &[u8]) -> Result<Vec<u8>, PhyDbError> {
    let mut v = Vec::new();
    v.try_reserve_exact(data.len()).map_err(|_| PhyDbError::NoMem(ty, data.len()))?;
    v.extend_from_slice(data);
    Ok(v)
}

fn groups(ty: u16, n: usize) -> Result<Vec<Vec<u8>>, PhyDbError> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| PhyDbError::NoMem(ty, n * core::mem::size_of::<Vec<u8>>()))?;
    v.resize_with(n, Vec::new);
    Ok(v)
}

impl PhyDb {
    pub const fn new() -> PhyDb {
        PhyDb { cfg: Vec::new(), nch: Vec::new(), papd: Vec::new(), txp: Vec::new() }
    }

    /// `iwl_phy_db_set_section` for a `CALIB_RES_NOTIF_PHY_DB` payload: `{ le16 type; le16 length; u8 data[] }`.
    pub fn set_section(&mut self, payload: &[u8]) -> Result<Stored, PhyDbError> {
        if payload.len() < CALIB_RES_NOTIF_PHY_DB_HDR_LEN {
            return Err(PhyDbError::Short);
        }
        let ty = u16::from_le_bytes([payload[0], payload[1]]);
        let size = u16::from_le_bytes([payload[2], payload[3]]) as usize;
        if payload.len() < CALIB_RES_NOTIF_PHY_DB_HDR_LEN + size {
            return Err(PhyDbError::Short);
        }
        let data = &payload[CALIB_RES_NOTIF_PHY_DB_HDR_LEN..CALIB_RES_NOTIF_PHY_DB_HDR_LEN + size];
        let mut chg_id = 0u16;
        let entry: &mut Vec<u8> = match ty {
            CFG => &mut self.cfg,
            CALIB_NCH => &mut self.nch,
            CALIB_CHG_PAPD | CALIB_CHG_TXP => {
                // `le16_to_cpup((__le16 *)phy_db_notif->data)`: Linux reads two bytes whatever `size` says.
                chg_id = u16::from_le_bytes([*data.first().unwrap_or(&0), *data.get(1).unwrap_or(&0)]);
                let table = if ty == CALIB_CHG_PAPD { &mut self.papd } else { &mut self.txp };
                if table.is_empty() {
                    *table = groups(ty, chg_id as usize + 1)?; // "Firmware sends the largest index first"
                }
                let n = table.len() as u16;
                table.get_mut(chg_id as usize).ok_or(PhyDbError::GroupOutOfRange(ty, chg_id, n))?
            }
            _ => return Err(PhyDbError::BadType(ty)),
        };
        // kfree(entry->data); entry->data = kmemdup(...); on failure entry->size = 0.
        entry.clear();
        *entry = dup(ty, data)?;
        Ok(Stored { ty, chg_id, size })
    }

    /// The sections `iwl_send_phy_db_data` sends, in its order: CFG, CALIB_NCH (both even when empty), then every
    /// non-empty PAPD group `0..n_group_papd`, then every non-empty TXP group `0..n_group_txp`. Yields (type, bytes).
    /// // iwl-phy-db.c (iwl_send_phy_db_data, iwl_phy_db_send_all_channel_groups)
    pub fn send_order(&self) -> impl Iterator<Item = (u16, &[u8])> {
        let fixed = [(CFG, &self.cfg[..]), (CALIB_NCH, &self.nch[..])];
        let papd = self.papd.iter().filter(|e| !e.is_empty()).map(|e| (CALIB_CHG_PAPD, &e[..]));
        let txp = self.txp.iter().filter(|e| !e.is_empty()).map(|e| (CALIB_CHG_TXP, &e[..]));
        fixed.into_iter().chain(papd).chain(txp)
    }

    /// (`n_group_papd`, `n_group_txp`).
    pub fn n_groups(&self) -> (u16, u16) {
        (self.papd.len() as u16, self.txp.len() as u16)
    }
}

/// `sizeof(struct iwl_calib_res_notif_phy_db)` without `data[]`. // fw/api/cmdhdr.h:130-134
const CALIB_RES_NOTIF_PHY_DB_HDR_LEN: usize = 4;
