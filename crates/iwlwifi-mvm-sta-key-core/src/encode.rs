// SPDX-License-Identifier: GPL-2.0-only
//! `ADD_STA_KEY` encoders (`iwl_mvm_send_sta_key`, `__iwl_mvm_remove_sta_key`).

use crate::ids::*;

/// An encoded command body: `len` valid bytes of `bytes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmd {
    pub bytes: [u8; CMD_V2_LEN],
    pub len: usize,
}

impl Cmd {
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeRefusal {
    /// Key id above 3 (a data key id is 0..=3, `STA_KEY_MAX_DATA_KEY_NUM` = 4, sta.h:192).
    KeyId(u8),
    /// Key slot at or above `STA_KEY_MAX_NUM`.
    KeyOffset(u8),
    /// A command version this port has no layout for (1, 2 and 3 exist in Linux).
    CmdVersion(u8),
}

/// The firmware's receive-sequence field for a CCMP packet number: `ieee80211_get_key_rx_seq`
/// hands back the PN most-significant byte first, and `iwl_mvm_send_sta_key` places it with a
/// two-byte hole after the first two bytes for command versions below 3 (sta.c:3628-3645).
/// `rsc` is the EAPOL Key RSC as a number (the six little-endian wire bytes already folded).
pub fn rx_seq(rsc: u64, cmd_ver: u8) -> [u8; 16] {
    let be = (rsc & 0xffff_ffff_ffff).to_be_bytes(); // be[2..8] = pn[0..6], MSB first
    let hole = if cmd_ver >= 3 { 0 } else { 2 };
    let mut o = [0u8; 16];
    o[0] = be[2];
    o[1] = be[3];
    o[2 + hole] = be[4];
    o[3 + hole] = be[5];
    o[4 + hole] = be[6];
    o[5 + hole] = be[7];
    o
}

/// Install (or replace) a 128-bit CCMP key for station `sta_id`.
///
/// `mcast` is true for a group key (sta.c:3591). `tx_pn` is the transmit packet number the
/// firmware starts from for command versions >= 2 (`key->tx_pn`); it is ignored for version 1,
/// whose layout has no field for it.
pub fn ccmp(
    cmd_ver: u8,
    sta_id: u8,
    key_offset: u8,
    key_id: u8,
    mcast: bool,
    key: &[u8; 16],
    rx_rsc: u64,
    tx_pn: u64,
) -> Result<Cmd, EncodeRefusal> {
    if key_id > 3 {
        return Err(EncodeRefusal::KeyId(key_id));
    }
    if key_offset as usize >= STA_KEY_MAX_NUM {
        return Err(EncodeRefusal::KeyOffset(key_offset));
    }
    if !(1..=3).contains(&cmd_ver) {
        return Err(EncodeRefusal::CmdVersion(cmd_ver));
    }
    let mut flags = ((key_id as u16) << STA_KEY_FLG_KEYID_POS) & STA_KEY_FLG_KEYID_MSK;
    flags |= STA_KEY_FLG_WEP_KEY_MAP | STA_KEY_FLG_CCM;
    if mcast {
        flags |= STA_KEY_MULTICAST;
    }
    let mut c = Cmd { bytes: [0; CMD_V2_LEN], len: if cmd_ver >= 2 { CMD_V2_LEN } else { CMD_V1_LEN } };
    c.bytes[0] = sta_id;
    c.bytes[1] = key_offset;
    c.bytes[2..4].copy_from_slice(&flags.to_le_bytes());
    c.bytes[4..20].copy_from_slice(key);
    c.bytes[36..52].copy_from_slice(&rx_seq(rx_rsc, cmd_ver));
    if cmd_ver >= 2 {
        // rx_mic_key (52..60) and tx_mic_key (60..68) stay zero for CCMP.
        c.bytes[68..76].copy_from_slice(&tx_pn.to_le_bytes());
    }
    Ok(c)
}

/// Remove the key in `key_offset` (`__iwl_mvm_remove_sta_key`): no encryption, `STA_KEY_NOT_VALID`.
/// `v2_size` mirrors Linux's `fw_has_api(TKIP_MIC_KEYS)` choice of command length (sta.c:2640).
pub fn remove(
    sta_id: u8,
    key_offset: u8,
    key_id: u8,
    mcast: bool,
    v2_size: bool,
) -> Result<Cmd, EncodeRefusal> {
    if key_id > 3 {
        return Err(EncodeRefusal::KeyId(key_id));
    }
    if key_offset as usize >= STA_KEY_MAX_NUM {
        return Err(EncodeRefusal::KeyOffset(key_offset));
    }
    let mut flags = ((key_id as u16) << STA_KEY_FLG_KEYID_POS) & STA_KEY_FLG_KEYID_MSK;
    flags |= STA_KEY_FLG_WEP_KEY_MAP | STA_KEY_NOT_VALID; // STA_KEY_FLG_NO_ENC is 0
    if mcast {
        flags |= STA_KEY_MULTICAST;
    }
    let mut c = Cmd { bytes: [0; CMD_V2_LEN], len: if v2_size { CMD_V2_LEN } else { CMD_V1_LEN } };
    c.bytes[0] = sta_id;
    c.bytes[1] = key_offset;
    c.bytes[2..4].copy_from_slice(&flags.to_le_bytes());
    Ok(c)
}
