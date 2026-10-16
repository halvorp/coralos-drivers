// SPDX-License-Identifier: GPL-2.0-only
//! Host-command payload encoders for the commands `iwl_mvm_up` sends after the RT image's ALIVE. Each writes the
//! little-endian `__packed` Linux layout into a caller buffer and returns the length, or names why it refused.

use crate::ids::*;

/// An encoder refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// The caller buffer cannot hold the command.
    BufferTooSmall { need: usize, have: usize },
    /// More scan channels than the firmware's `N_SCAN_CHANNELS` TLV allows. // mvm/scan.c:1170-1171
    TooManyChannels { n: usize, max: usize },
    /// More paging blocks than `NUM_OF_FW_PAGING_BLOCKS`. // fw/api/paging.h:10
    TooManyPagingBlocks { n: usize },
}

pub(crate) fn check(buf: &mut [u8], need: usize) -> Result<(), EncodeError> {
    if buf.len() < need {
        return Err(EncodeError::BufferTooSmall { need, have: buf.len() });
    }
    buf[..need].fill(0);
    Ok(())
}

pub(crate) fn put32(buf: &mut [u8], off: usize, v: u32) {
    buf[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
pub(crate) fn put16(buf: &mut [u8], off: usize, v: u16) {
    buf[off..off + 2].copy_from_slice(&v.to_le_bytes());
}

/// `iwl_mvm_send_bt_init_conf`: `struct iwl_bt_coex_cmd { le32 mode; le32 enabled_modules; }` with mode `BT_COEX_NW`
/// and `enabled_modules = SYNC2SCO (IWL_MVM_BT_COEX_SYNC2SCO = 1) | MPLUT (when the firmware has BT_MPLUT_SUPPORT) |
/// HIGH_BAND_RET`. // mvm/coex.c (iwl_mvm_send_bt_init_conf); mvm/constants.h:41-42; mvm/mvm.h:1469-1474
pub fn encode_bt_config(mplut_supported: bool, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, BT_COEX_CMD_LEN)?;
    put32(buf, 0, BT_COEX_NW);
    let mut modules = BT_COEX_SYNC2SCO_ENABLED;
    if mplut_supported {
        modules |= BT_COEX_MPLUT_ENABLED;
    }
    modules |= BT_COEX_HIGH_BAND_RET;
    put32(buf, 4, modules);
    Ok(BT_COEX_CMD_LEN)
}

/// `iwl_mvm_send_dqa_cmd`: `struct iwl_dqa_enable_cmd { le32 cmd_queue; }` = `IWL_MVM_DQA_CMD_QUEUE`.
/// // mvm/fw.c:72-87 (iwl_mvm_send_dqa_cmd); fw/api/datapath.h:18
pub fn encode_dqa_enable(buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, DQA_ENABLE_CMD_LEN)?;
    put32(buf, 0, IWL_MVM_DQA_CMD_QUEUE as u32);
    Ok(DQA_ENABLE_CMD_LEN)
}

/// `iwl_set_soc_latency` for a DISCRETE part with the default `mac_cfg` (xtal_latency 0, ltr_delay NONE,
/// low_latency_xtal false; `iwl9000_mac_cfg`, cfg/9000.c:71-76): `flags = SOC_CONFIG_CMD_FLAGS_DISCRETE`, `latency = 0`.
/// // fw/init.c:59-95 (iwl_set_soc_latency); fw/api/system.h:27-30
pub fn encode_soc_config(buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, SOC_CONFIGURATION_CMD_LEN)?;
    put32(buf, 0, SOC_CONFIG_CMD_FLAGS_DISCRETE);
    put32(buf, 4, 0);
    Ok(SOC_CONFIGURATION_CMD_LEN)
}

/// `iwl_mvm_power_update_device` with the default power scheme: `struct iwl_device_power_cmd { le16 flags; le16
/// reserved; }`, `flags = DEVICE_POWER_FLAGS_POWER_SAVE_ENA_MSK`. // mvm/power.c (iwl_mvm_power_update_device)
pub fn encode_device_power(buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, DEVICE_POWER_CMD_LEN)?;
    put16(buf, 0, DEVICE_POWER_FLAGS_POWER_SAVE_ENA_MSK);
    Ok(DEVICE_POWER_CMD_LEN)
}

/// `iwl_mvm_send_temp_report_ths_cmd` with no thermal zone: an all-zero `struct temp_report_ths_cmd`.
/// // mvm/tt.c (iwl_mvm_send_temp_report_ths_cmd); fw/api/phy.h:232-235
pub fn encode_temp_report_ths(buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, TEMP_REPORT_THS_CMD_LEN)?;
    Ok(TEMP_REPORT_THS_CMD_LEN)
}

/// `iwl_mvm_update_mcc`: `struct iwl_mcc_update_cmd { le16 mcc; u8 source_id; u8 reserved; le32 key; u8 reserved2[20]; }`
/// with `mcc = alpha2[0] << 8 | alpha2[1]`. // mvm/nvm.c (iwl_mvm_update_mcc); fw/api/nvm-reg.h:284-290
pub fn encode_mcc_update(alpha2: [u8; 2], source_id: u8, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, MCC_UPDATE_CMD_LEN)?;
    put16(buf, 0, (alpha2[0] as u16) << 8 | alpha2[1] as u16);
    buf[2] = source_id;
    Ok(MCC_UPDATE_CMD_LEN)
}

/// `struct iwl_scd_txq_cfg_cmd` as `iwl_mvm_enable_txq` builds it for an aux queue (`iwl_mvm_enable_aux_snif_queue`:
/// fifo given, sta_id given, tid `IWL_MAX_TID_COUNT`, not aggregated, `frame_limit = IWL_FRAME_LIMIT`, ssn 0):
/// `{ token 0, sta_id, tid, scd_queue, action SCD_CFG_ENABLE_QUEUE, aggregate 0, tx_fifo, window, le16 ssn, le16 reserved }`.
/// // mvm/sta.c:973-1000, :1006-1020 (iwl_mvm_enable_txq, iwl_mvm_enable_aux_snif_queue); fw/api/tx.h:952-962
pub fn encode_scd_queue_cfg_enable(queue: u8, sta_id: u8, fifo: u8, ssn: u16, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, SCD_TXQ_CFG_CMD_LEN)?;
    buf[0] = 0; // token
    buf[1] = sta_id;
    buf[2] = IWL_MAX_TID_COUNT;
    buf[3] = queue;
    buf[4] = SCD_CFG_ENABLE_QUEUE;
    buf[5] = 0; // aggregate
    buf[6] = fifo;
    buf[7] = IWL_FRAME_LIMIT;
    put16(buf, 8, ssn);
    Ok(SCD_TXQ_CFG_CMD_LEN)
}

/// `iwl_mvm_add_int_sta_common` for the aux station on a firmware with `API_STA_TYPE` and ADD_STA v10:
/// `struct iwl_mvm_add_sta_cmd` (48 bytes) with `sta_id`, `mac_id_n_color = FW_CMD_ID_AND_COLOR(MAC_INDEX_AUX, 0)`,
/// `station_type = IWL_STA_AUX_ACTIVITY`, `tfd_queue_msk = BIT(aux_queue)`, `tid_disable_tx = 0xffff`, no address.
/// // mvm/sta.c:1693-1741, :2243-2265; fw/api/sta.h (ADD_STA_CMD_API_S_VER_10)
pub fn encode_add_sta_aux(sta_id: u8, aux_queue: u8, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, ADD_STA_CMD_LEN)?;
    // layout: add_modify u8 @0, awake_acs @1, tid_disable_tx le16 @2, mac_id_n_color le32 @4, addr[6] @8, reserved2 le16 @14,
    // sta_id @16, modify_mask @17, reserved3 le16 @18, station_flags le32 @20, station_flags_msk le32 @24,
    // add_immediate_ba_tid @28, remove_immediate_ba_tid @29, add_immediate_ba_ssn le16 @30, sleep_tx_count le16 @32,
    // sleep_state_flags u8 @34, station_type u8 @35, assoc_id le16 @36, beamform_flags le16 @38, tfd_queue_msk le32 @40,
    // rx_ba_window le16 @44, sp_length @46, uapsd_acs @47.
    put16(buf, 2, 0xffff);
    put32(buf, 4, MAC_INDEX_AUX); // FW_CMD_ID_AND_COLOR(MAC_INDEX_AUX, 0): id << 0 | color << 8
    buf[16] = sta_id;
    buf[35] = IWL_STA_AUX_ACTIVITY;
    put32(buf, 40, 1u32 << aux_queue);
    Ok(ADD_STA_CMD_LEN)
}

/// `struct iwl_phy_db_cmd { le16 type; le16 length; u8 data[]; }` header — the data chunk follows as a NOCOPY chunk.
/// // iwl-phy-db.c:305-330 (iwl_send_phy_db_cmd; struct iwl_phy_db_cmd is {le16 type; le16 length; u8 data[]})
pub fn encode_phy_db_hdr(ty: u16, length: u16) -> [u8; 4] {
    let t = ty.to_le_bytes();
    let l = length.to_le_bytes();
    [t[0], t[1], l[0], l[1]]
}
