// SPDX-License-Identifier: GPL-2.0-only
//! Byte-level vectors for the post-ALIVE commands. Expected bytes are written out by hand from the Linux struct
//! definitions cited beside each (fw/api/*.h), not derived from the encoders.
use iwlwifi_mvm_rt_core::encode::*;
use iwlwifi_mvm_rt_core::ids::*;

#[test]
fn bt_config_is_mode_nw_and_sync2sco_plus_high_band_ret_with_mplut_when_advertised() {
    let mut b = [0xaau8; 16];
    // struct iwl_bt_coex_cmd { le32 mode = BT_COEX_NW (1); le32 enabled_modules } fw/api/coex.h:49-52
    // modules = SYNC2SCO BIT(2) | HIGH_BAND_RET BIT(4) = 0x14 ; with MPLUT BIT(0) = 0x15 (fw/api/coex.h:35-39)
    assert_eq!(encode_bt_config(false, &mut b), Ok(8));
    assert_eq!(&b[..8], &[1, 0, 0, 0, 0x14, 0, 0, 0]);
    assert_eq!(encode_bt_config(true, &mut b), Ok(8));
    assert_eq!(&b[..8], &[1, 0, 0, 0, 0x15, 0, 0, 0]);
    assert_eq!(b[8], 0xaa, "nothing past the command is touched");
}

#[test]
fn small_commands_are_the_linux_literals() {
    let mut b = [0xaau8; 32];
    assert_eq!(encode_dqa_enable(&mut b), Ok(4)); // iwl_dqa_enable_cmd { cmd_queue = IWL_MVM_DQA_CMD_QUEUE (0) }
    assert_eq!(&b[..4], &[0, 0, 0, 0]);
    assert_eq!(encode_soc_config(&mut b), Ok(8)); // flags = SOC_CONFIG_CMD_FLAGS_DISCRETE (BIT(0)), latency = 0
    assert_eq!(&b[..8], &[1, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(encode_device_power(&mut b), Ok(4)); // iwl_device_power_cmd { le16 flags = POWER_SAVE_ENA (BIT(0)); le16 reserved }
    assert_eq!(&b[..4], &[1, 0, 0, 0]);
    assert_eq!(encode_temp_report_ths(&mut b), Ok(20)); // temp_report_ths_cmd { le32 num_temps; le16 thresholds[8] }, all zero
    assert!(b[..20].iter().all(|&x| x == 0));
    assert_eq!(b[20], 0xaa);
}

#[test]
fn mcc_update_zz_get_current_is_28_bytes() {
    let mut b = [0xaau8; 40];
    assert_eq!(encode_mcc_update(*b"ZZ", MCC_SOURCE_GET_CURRENT, &mut b), Ok(28));
    // iwl_mcc_update_cmd { le16 mcc = 'Z'<<8|'Z' = 0x5a5a; u8 source_id = 0x10; u8 reserved; le32 key; u8 reserved2[20] }
    let mut want = [0u8; 28];
    want[0] = 0x5a;
    want[1] = 0x5a;
    want[2] = 0x10;
    assert_eq!(&b[..28], &want);
    assert_eq!(b[28], 0xaa);
    // 'U','S' = 0x5553 little-endian is 53 55
    assert_eq!(encode_mcc_update(*b"US", 0, &mut b), Ok(28));
    assert_eq!(&b[..3], &[0x53, 0x55, 0]);
}

#[test]
fn scd_queue_cfg_for_the_aux_queue() {
    let mut b = [0xaau8; 16];
    // iwl_scd_txq_cfg_cmd { token, sta_id, tid, scd_queue, action, aggregate, tx_fifo, window, le16 ssn, le16 reserved }
    // aux: queue 1, sta 1, fifo MCAST (5), tid IWL_MAX_TID_COUNT (8), ENABLE (1), window IWL_FRAME_LIMIT (64)
    assert_eq!(encode_scd_queue_cfg_enable(1, 1, 5, 0, &mut b), Ok(12));
    assert_eq!(&b[..12], &[0, 1, 8, 1, 1, 0, 5, 64, 0, 0, 0, 0]);
    assert_eq!(b[12], 0xaa);
}

#[test]
fn add_sta_for_the_aux_station_matches_the_v10_layout() {
    let mut b = [0xaau8; 64];
    assert_eq!(encode_add_sta_aux(1, 1, &mut b), Ok(48));
    // struct iwl_mvm_add_sta_cmd (ADD_STA_CMD_API_S_VER_10, fw/api/sta.h), offsets counted from the struct:
    // add_modify@0 awake_acs@1 tid_disable_tx@2 mac_id_n_color@4 addr@8 reserved2@14 sta_id@16 modify_mask@17 reserved3@18
    // station_flags@20 station_flags_msk@24 add_immediate_ba_tid@28 remove_immediate_ba_tid@29 add_immediate_ba_ssn@30
    // sleep_tx_count@32 sleep_state_flags@34 station_type@35 assoc_id@36 beamform_flags@38 tfd_queue_msk@40
    // rx_ba_window@44 sp_length@46 uapsd_acs@47
    let mut want = [0u8; 48];
    want[2] = 0xff; // tid_disable_tx = 0xffff
    want[3] = 0xff;
    want[4] = 4; // FW_CMD_ID_AND_COLOR(MAC_INDEX_AUX = 4, 0)
    want[16] = 1; // sta_id
    want[35] = 4; // IWL_STA_AUX_ACTIVITY
    want[40] = 2; // tfd_queue_msk = BIT(aux queue 1)
    assert_eq!(&b[..48], &want);
    assert_eq!(b[48], 0xaa);
}

#[test]
fn mac_context_add_for_an_unassociated_station_is_148_bytes() {
    let mut b = [0xaau8; 160];
    let mac = [0x12, 0x22, 0x33, 0x44, 0x55, 0x66];
    assert_eq!(encode_mac_ctxt_sta_add(0, mac, &mut b), Ok(148));
    // struct iwl_mac_ctx_cmd (fw/api/mac.h), offsets counted from the struct, expected values from the Linux enums:
    let mut want = [0u8; 148];
    want[4] = 1; // action = FW_CTXT_ACTION_ADD
    want[8] = 5; // mac_type = FW_MAC_TYPE_BSS_STA (FIRST=AUX=1, LISTENER, PIBSS, IBSS, BSS_STA)
    // tsf_id TSF_ID_A = 0 ; id_and_color = id 0 | color 0
    want[16..22].copy_from_slice(&mac); // node_addr ; bssid_addr@24 stays zero
    want[32] = 0x0f; // cck_rates: 1,2,5.5,11 (IWL_RATE_*_INDEX 0..3)
    want[36] = 0x15; // ofdm_rates: 6 (bit 0), 12 (bit 2), 24 (bit 4) of IWL_FIRST_OFDM_RATE = 4
    want[52] = 0x44; // filter_flags = ACCEPT_GRP BIT(2) | IN_BEACON BIT(6)
    want[132] = 10; // listen_interval = IWL_MVM_CONN_LISTEN_INTERVAL
    assert_eq!(&b[..148], &want[..]);
    assert_eq!(b[148], 0xaa);
    // another id lands in id_and_color's low byte
    encode_mac_ctxt_sta_add(2, mac, &mut b).unwrap();
    assert_eq!(b[0], 2);
    assert_eq!(encode_mac_ctxt_sta_add(0, mac, &mut [0u8; 147]), Err(EncodeError::BufferTooSmall { need: 148, have: 147 }));
}

#[test]
fn phy_db_header_and_buffer_refusal() {
    assert_eq!(encode_phy_db_hdr(4, 0x0123), [4, 0, 0x23, 0x01]);
    let mut small = [0u8; 3];
    assert_eq!(encode_bt_config(false, &mut small), Err(EncodeError::BufferTooSmall { need: 8, have: 3 }));
    assert_eq!(encode_mcc_update(*b"ZZ", 0x10, &mut [0u8; 27]), Err(EncodeError::BufferTooSmall { need: 28, have: 27 }));
}

#[test]
fn id_constants_are_pinned_by_value() {
    // fw/api/commands.h, datapath.h, system.h, phy.h, fw/api/cmdhdr.h
    assert_eq!((LEGACY_GROUP, LONG_GROUP, SYSTEM_GROUP, PHY_OPS_GROUP, DATA_PATH_GROUP), (0, 1, 2, 4, 5));
    assert_eq!((SCAN_CFG_CMD, SCAN_REQ_UMAC, SCAN_ABORT_UMAC, SCAN_COMPLETE_UMAC), (0x0c, 0x0d, 0x0e, 0x0f));
    assert_eq!(SCAN_ITERATION_COMPLETE_UMAC, 0xb5);
    assert_eq!((ADD_STA, SCD_QUEUE_CFG, FW_PAGING_BLOCK_CMD), (0x18, 0x1d, 0x4f));
    assert_eq!((CALIB_RES_NOTIF_PHY_DB, PHY_DB_CMD, POWER_TABLE_CMD, BT_CONFIG, MCC_UPDATE_CMD, REPLY_RX_MPDU_CMD), (0x6b, 0x6c, 0x77, 0x9b, 0xc8, 0xc1));
    assert_eq!((DQA_ENABLE_CMD, SOC_CONFIGURATION_CMD, TEMP_REPORTING_THRESHOLDS_CMD), (0, 1, 4));
    assert_eq!((MAC_CONTEXT_CMD, FW_CTXT_ACTION_ADD, FW_MAC_TYPE_BSS_STA, TSF_ID_A), (0x28, 1, 5, 0)); // commands.h:208, context.h:42, mac.h:64-86
    assert_eq!((MAC_FILTER_ACCEPT_GRP, MAC_FILTER_IN_BEACON, MAC_CTX_CMD_LEN, IWL_MVM_CONN_LISTEN_INTERVAL), (4, 0x40, 148, 10));
}
