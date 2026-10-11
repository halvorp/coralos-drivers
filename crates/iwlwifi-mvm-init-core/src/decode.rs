// SPDX-License-Identifier: GPL-2.0-only
//! Bounds-checked decoders for the two firmware messages the INIT flow parses. Ported from Linux
//! `fw/api/nvm-reg.h` (`struct iwl_nvm_access_resp`), `fw/api/cmdhdr.h`
//! (`struct iwl_calib_res_notif_phy_db`), `mvm/nvm.c` (`iwl_nvm_read_chunk`) and `iwl-phy-db.c`
//! (`iwl_phy_db_set_section`, `iwl_phy_db_get_section`).

use crate::ids::*;

/// A decoder refused its input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload is shorter than the fixed header.
    ShortHeader { need: usize, have: usize },
    /// The header's `length` claims more data than the payload holds.
    ShortData { claimed: usize, have: usize },
    /// PHY-DB `type` that `iwl_phy_db_get_section` maps to NULL (iwl-phy-db.c:86, :102): 0, 3 or >= 6.
    UnknownPhyDbType { ty: u16 },
    /// CoralOS bound, NOT Linux: PAPD/TXP entries carry a leading `le16` channel-group id
    /// (iwl-phy-db.c:167-168, :181-182) that Linux reads without checking the length is >= 2.
    PhyDbGroupIdMissing { ty: u16, length: usize },
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// `struct iwl_nvm_access_resp` (fw/api/nvm-reg.h:125-130): `le16 offset, length, type, status;
/// u8 data[]`. `data` is exactly the `length` bytes the firmware claims (bounded by the payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmAccessResp<'a> {
    pub offset: u16,
    pub length: u16,
    pub section: u16,
    pub status: u16,
    pub data: &'a [u8],
}

/// Decode an NVM_ACCESS_CMD response payload. Linux (`iwl_nvm_read_chunk`, mvm/nvm.c:100-104) casts
/// the payload and trusts `length` — the length-vs-payload check here is a CoralOS bound.
pub fn decode_nvm_access_resp(payload: &[u8]) -> Result<NvmAccessResp<'_>, DecodeError> {
    if payload.len() < NVM_ACCESS_RESP_HDR_LEN {
        return Err(DecodeError::ShortHeader { need: NVM_ACCESS_RESP_HDR_LEN, have: payload.len() });
    }
    let length = le16(payload, 2);
    let have = payload.len() - NVM_ACCESS_RESP_HDR_LEN;
    // A non-zero status carries no data Linux reads (nvm.c:105-127), so only a SUCCESS response
    // is held to its claimed length.
    let status = le16(payload, 6);
    let take = if status == READ_NVM_CHUNK_SUCCEED {
        if (length as usize) > have {
            return Err(DecodeError::ShortData { claimed: length as usize, have });
        }
        length as usize
    } else {
        0
    };
    Ok(NvmAccessResp {
        offset: le16(payload, 0),
        length,
        section: le16(payload, 4),
        status,
        data: &payload[NVM_ACCESS_RESP_HDR_LEN..NVM_ACCESS_RESP_HDR_LEN + take],
    })
}

/// Which `iwl_phy_db` entry a notification fills (`iwl_phy_db_get_section`, iwl-phy-db.c:86-104).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhyDbSection {
    Cfg,
    CalibNch,
    ChgPapd,
    ChgTxp,
}

/// `struct iwl_calib_res_notif_phy_db` (fw/api/cmdhdr.h:130-134): `le16 type; le16 length; u8
/// data[]`. ONE entry per notification: `iwl_phy_db_set_section` (iwl-phy-db.c:144-215) stores
/// exactly one `{type, size, data}` per packet. Bytes after `length` are ignored, as Linux
/// (`pkt_len < sizeof + size` is the only length test, iwl-phy-db.c:161).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CalibResPhyDb<'a> {
    pub ty: u16,
    pub section: PhyDbSection,
    pub data: &'a [u8],
    /// The channel-group id, for `ChgPapd` / `ChgTxp` only (iwl-phy-db.c:168, :182).
    pub chg_id: Option<u16>,
}

/// Decode a CALIB_RES_NOTIF_PHY_DB payload as `iwl_phy_db_set_section` does, up to (not including)
/// the stateful `chg_id >= n_group_*` test, which needs the allocated PHY DB.
pub fn decode_calib_res_phy_db(payload: &[u8]) -> Result<CalibResPhyDb<'_>, DecodeError> {
    if payload.len() < CALIB_RES_PHY_DB_HDR_LEN {
        return Err(DecodeError::ShortHeader { need: CALIB_RES_PHY_DB_HDR_LEN, have: payload.len() });
    }
    let ty = le16(payload, 0);
    let size = le16(payload, 2) as usize;
    let have = payload.len() - CALIB_RES_PHY_DB_HDR_LEN;
    if size > have {
        return Err(DecodeError::ShortData { claimed: size, have });
    }
    let data = &payload[CALIB_RES_PHY_DB_HDR_LEN..CALIB_RES_PHY_DB_HDR_LEN + size];
    let section = match ty {
        IWL_PHY_DB_CFG => PhyDbSection::Cfg,
        IWL_PHY_DB_CALIB_NCH => PhyDbSection::CalibNch,
        IWL_PHY_DB_CALIB_CHG_PAPD => PhyDbSection::ChgPapd,
        IWL_PHY_DB_CALIB_CHG_TXP => PhyDbSection::ChgTxp,
        _ => return Err(DecodeError::UnknownPhyDbType { ty }),
    };
    let chg_id = match section {
        PhyDbSection::ChgPapd | PhyDbSection::ChgTxp => {
            if data.len() < 2 {
                return Err(DecodeError::PhyDbGroupIdMissing { ty, length: data.len() });
            }
            Some(le16(data, 0))
        }
        _ => None,
    };
    Ok(CalibResPhyDb { ty, section, data, chg_id })
}
