// SPDX-License-Identifier: GPL-2.0-only
//! Command ids, groups, constants and sizes the RT-image (REGULAR) bring-up and the passive scan use. Every value
//! carries the Linux FILE:LINE it was read from (Linux v7.0-rc6 tree at references/linux-ref,
//! `drivers/net/wireless/intel/iwlwifi/`).

// ---- groups ------------------------------------------------------------------------------------------------
/// `LEGACY_GROUP`: the group id 0 of commands that use the short header. // fw/api/commands.h:38
pub const LEGACY_GROUP: u8 = 0x0;
/// `IWL_ALWAYS_LONG_GROUP`/`LONG_GROUP`: legacy command ids sent with the wide header. // fw/api/cmdhdr.h:57; fw/api/commands.h:40
pub const LONG_GROUP: u8 = 0x1;
/// `SYSTEM_GROUP`. // fw/api/commands.h:41
pub const SYSTEM_GROUP: u8 = 0x2;
/// `PHY_OPS_GROUP`. // fw/api/commands.h:43
pub const PHY_OPS_GROUP: u8 = 0x4;
/// `DATA_PATH_GROUP`. // fw/api/commands.h:44
pub const DATA_PATH_GROUP: u8 = 0x5;

// ---- commands and notifications ----------------------------------------------------------------------------
/// `UCODE_ALIVE_NTFY`. // fw/api/commands.h:65
pub const UCODE_ALIVE_NTFY: u8 = 0x1;
/// `PHY_CONTEXT_CMD`. // fw/api/commands.h:87
pub const PHY_CONTEXT_CMD: u8 = 0x8;
/// `SCAN_CFG_CMD` (sent in the long group). // fw/api/commands.h:106; mvm/scan.c:1252
pub const SCAN_CFG_CMD: u8 = 0xc;
/// `SCAN_REQ_UMAC` (sent in the long group). // fw/api/commands.h:111; mvm/scan.c:2850
pub const SCAN_REQ_UMAC: u8 = 0xd;
/// `SCAN_ABORT_UMAC` (long group). // fw/api/commands.h:116
pub const SCAN_ABORT_UMAC: u8 = 0xe;
/// `SCAN_COMPLETE_UMAC`. // fw/api/commands.h:121
pub const SCAN_COMPLETE_UMAC: u8 = 0xf;
/// `SCAN_ITERATION_COMPLETE_UMAC`. // fw/api/commands.h:99
pub const SCAN_ITERATION_COMPLETE_UMAC: u8 = 0xb5;
/// `ADD_STA`. // fw/api/commands.h:140
pub const ADD_STA: u8 = 0x18;
/// `SCD_QUEUE_CFG`. // fw/api/commands.h:174
pub const SCD_QUEUE_CFG: u8 = 0x1d;
/// `FW_PAGING_BLOCK_CMD` (sent in the long group). // fw/api/commands.h:252; fw/paging.c:206
pub const FW_PAGING_BLOCK_CMD: u8 = 0x4f;
/// `CALIB_RES_NOTIF_PHY_DB`. // fw/api/commands.h:319
pub const CALIB_RES_NOTIF_PHY_DB: u8 = 0x6b;
/// `PHY_DB_CMD`. // fw/api/commands.h:324
pub const PHY_DB_CMD: u8 = 0x6c;
/// `POWER_TABLE_CMD` (the device power command). // fw/api/commands.h:329
pub const POWER_TABLE_CMD: u8 = 0x77;
/// `BT_CONFIG`. // fw/api/commands.h:480
pub const BT_CONFIG: u8 = 0x9b;
/// `MCC_UPDATE_CMD`. // fw/api/commands.h:459
pub const MCC_UPDATE_CMD: u8 = 0xc8;
/// `REPLY_RX_MPDU_CMD`. // fw/api/commands.h:432
pub const REPLY_RX_MPDU_CMD: u8 = 0xc1;
/// `DQA_ENABLE_CMD` (DATA_PATH_GROUP). // fw/api/datapath.h:18
pub const DQA_ENABLE_CMD: u8 = 0x0;
/// `SOC_CONFIGURATION_CMD` (SYSTEM_GROUP). // fw/api/commands.h:602
pub const SOC_CONFIGURATION_CMD: u8 = 0x01;
/// `TEMP_REPORTING_THRESHOLDS_CMD` (PHY_OPS_GROUP). // fw/api/phy.h:31
pub const TEMP_REPORTING_THRESHOLDS_CMD: u8 = 0x04;

// ---- paging ------------------------------------------------------------------------------------------------
/// `PAGE_2_EXP_SIZE`: 4 KiB pages. // fw/img.h:93
pub const PAGE_2_EXP_SIZE: u32 = 12;
/// `FW_PAGING_SIZE`. // fw/img.h:94
pub const FW_PAGING_SIZE: u32 = 1 << PAGE_2_EXP_SIZE;
/// `PAGE_PER_GROUP_2_EXP_SIZE`: 8 pages per block. // fw/img.h:96
pub const PAGE_PER_GROUP_2_EXP_SIZE: u32 = 3;
/// `NUM_OF_PAGE_PER_GROUP`. // fw/img.h:97
pub const NUM_OF_PAGE_PER_GROUP: u32 = 1 << PAGE_PER_GROUP_2_EXP_SIZE;
/// `PAGING_BLOCK_SIZE`: 32 KiB. // fw/img.h:99
pub const PAGING_BLOCK_SIZE: u32 = NUM_OF_PAGE_PER_GROUP * FW_PAGING_SIZE;
/// `BLOCK_2_EXP_SIZE`. // fw/img.h:101
pub const BLOCK_2_EXP_SIZE: u32 = PAGE_2_EXP_SIZE + PAGE_PER_GROUP_2_EXP_SIZE;
/// `PAGING_CMD_IS_SECURED`. // fw/img.h:115
pub const PAGING_CMD_IS_SECURED: u32 = 1 << 9;
/// `PAGING_CMD_IS_ENABLED`. // fw/img.h:116
pub const PAGING_CMD_IS_ENABLED: u32 = 1 << 8;
/// `PAGING_CMD_NUM_OF_PAGES_IN_LAST_GRP_POS`. // fw/img.h:117
pub const PAGING_CMD_NUM_OF_PAGES_IN_LAST_GRP_POS: u32 = 0;
/// `NUM_OF_FW_PAGING_BLOCKS`: 32 data blocks and one CSS block. // fw/api/paging.h:10
pub const NUM_OF_FW_PAGING_BLOCKS: usize = 33;
/// `sizeof(struct iwl_fw_paging_cmd)`: three `__le32` and 33 `__le32` addresses. // fw/api/paging.h:19-27
pub const FW_PAGING_CMD_LEN: usize = 12 + 4 * NUM_OF_FW_PAGING_BLOCKS;
/// `PAGING_SEPARATOR_SECTION` as a section's offset word. // fw/file.h:581
pub const PAGING_SEPARATOR_SECTION: u32 = 0xAAAA_BBBB;

// ---- small commands -----------------------------------------------------------------------------------------
/// `BT_COEX_NW`. // fw/api/coex.h:29
pub const BT_COEX_NW: u32 = 0x1;
/// `BT_COEX_MPLUT_ENABLED`. // fw/api/coex.h:35
pub const BT_COEX_MPLUT_ENABLED: u32 = 1 << 0;
/// `BT_COEX_SYNC2SCO_ENABLED`. // fw/api/coex.h:37
pub const BT_COEX_SYNC2SCO_ENABLED: u32 = 1 << 2;
/// `BT_COEX_HIGH_BAND_RET`. // fw/api/coex.h:39
pub const BT_COEX_HIGH_BAND_RET: u32 = 1 << 4;
/// `sizeof(struct iwl_bt_coex_cmd)`. // fw/api/coex.h:49-52
pub const BT_COEX_CMD_LEN: usize = 8;
/// `sizeof(struct iwl_dqa_enable_cmd)`: one `__le32`. // fw/api/datapath.h (cmd_queue)
pub const DQA_ENABLE_CMD_LEN: usize = 4;
/// `IWL_MVM_DQA_CMD_QUEUE`. // fw/api/txq.h:35
pub const IWL_MVM_DQA_CMD_QUEUE: u8 = 0;
/// `IWL_MVM_DQA_AUX_QUEUE`. // fw/api/txq.h:36
pub const IWL_MVM_DQA_AUX_QUEUE: u8 = 1;
/// `SOC_CONFIG_CMD_FLAGS_DISCRETE`. // fw/api/system.h:10
pub const SOC_CONFIG_CMD_FLAGS_DISCRETE: u32 = 1 << 0;
/// `sizeof(struct iwl_soc_configuration_cmd)`. // fw/api/system.h:27-30
pub const SOC_CONFIGURATION_CMD_LEN: usize = 8;
/// `DEVICE_POWER_FLAGS_POWER_SAVE_ENA_MSK`. // fw/api/power.h:158
pub const DEVICE_POWER_FLAGS_POWER_SAVE_ENA_MSK: u16 = 1 << 0;
/// `sizeof(struct iwl_device_power_cmd)`. // fw/api/power.h:171-175
pub const DEVICE_POWER_CMD_LEN: usize = 4;
/// `IWL_MAX_DTS_TRIPS`. // fw/api/phy.h:224
pub const IWL_MAX_DTS_TRIPS: usize = 8;
/// `sizeof(struct temp_report_ths_cmd)`. // fw/api/phy.h:232-235
pub const TEMP_REPORT_THS_CMD_LEN: usize = 4 + 2 * IWL_MAX_DTS_TRIPS;

// ---- stations and queues -----------------------------------------------------------------------------------
/// `MAC_INDEX_AUX`. // fw/api/mac.h:13
pub const MAC_INDEX_AUX: u32 = 4;
/// `IWL_STA_AUX_ACTIVITY`. // fw/api/sta.h:280
pub const IWL_STA_AUX_ACTIVITY: u8 = 4;
/// `IWL_MVM_TX_FIFO_MCAST`. // fw/api/txq.h:53
pub const IWL_MVM_TX_FIFO_MCAST: u8 = 5;
/// `IWL_MAX_TID_COUNT`: the "no tid" value an aux queue is configured with. // iwl-trans.h:271
pub const IWL_MAX_TID_COUNT: u8 = 8;
/// `IWL_FRAME_LIMIT`. // iwl-trans.h:273
pub const IWL_FRAME_LIMIT: u8 = 64;
/// `SCD_CFG_ENABLE_QUEUE`. // fw/api/tx.h:934
pub const SCD_CFG_ENABLE_QUEUE: u8 = 1;
/// `sizeof(struct iwl_scd_txq_cfg_cmd)`. // fw/api/tx.h:952-962
pub const SCD_TXQ_CFG_CMD_LEN: usize = 12;
/// `sizeof(struct iwl_mvm_add_sta_cmd)` (ADD_STA_CMD_API_S_VER_10). // fw/api/sta.h (struct iwl_mvm_add_sta_cmd)
pub const ADD_STA_CMD_LEN: usize = 48;
/// `ADD_STA_SUCCESS`. // fw/api/sta.h:413
pub const ADD_STA_SUCCESS: u32 = 0x1;
/// `IWL_ADD_STA_STATUS_MASK`. // fw/api/sta.h:198
pub const IWL_ADD_STA_STATUS_MASK: u32 = 0xFF;
/// The sta id `iwl_mvm_find_free_sta_id` hands the first internal (non-STATION) station: id 0 is reserved.
/// // mvm/sta.c:37-48
pub const AUX_STA_ID: u8 = 1;

// ---- MCC / LAR ---------------------------------------------------------------------------------------------
/// `MCC_SOURCE_GET_CURRENT`. // fw/api/nvm-reg.h:434
pub const MCC_SOURCE_GET_CURRENT: u8 = 0x10;
/// `sizeof(struct iwl_mcc_update_cmd)`. // fw/api/nvm-reg.h:284-290
pub const MCC_UPDATE_CMD_LEN: usize = 28;
/// `sizeof(struct iwl_mcc_update_resp_v3)` without `channels[]`. // fw/api/nvm-reg.h:319-328
pub const MCC_UPDATE_RESP_V3_HDR_LEN: usize = 16;

// ---- scan ---------------------------------------------------------------------------------------------------
/// `IWL_SCAN_DWELL_ACTIVE`. // mvm/scan.c:19
pub const IWL_SCAN_DWELL_ACTIVE: u8 = 10;
/// `IWL_SCAN_DWELL_PASSIVE`. // mvm/scan.c:20
pub const IWL_SCAN_DWELL_PASSIVE: u8 = 110;
/// `IWL_SCAN_DWELL_FRAGMENTED`. // mvm/scan.c:21
pub const IWL_SCAN_DWELL_FRAGMENTED: u8 = 44;
/// `IWL_SCAN_DWELL_EXTENDED`. // mvm/scan.c:22
pub const IWL_SCAN_DWELL_EXTENDED: u8 = 90;
/// `IWL_SCAN_ADWELL_MAX_BUDGET_FULL_SCAN`. // mvm/scan.c:26
pub const IWL_SCAN_ADWELL_MAX_BUDGET_FULL_SCAN: u16 = 300;
/// `IWL_SCAN_ADWELL_DEFAULT_LB_N_APS`. // mvm/scan.c:32
pub const IWL_SCAN_ADWELL_DEFAULT_LB_N_APS: u8 = 2;
/// `IWL_SCAN_ADWELL_DEFAULT_N_APS_SOCIAL`. // mvm/scan.c:34
pub const IWL_SCAN_ADWELL_DEFAULT_N_APS_SOCIAL: u8 = 10;
/// `IWL_SCAN_PRIORITY_EXT_6`. // fw/api/scan.h:325-331 (EXT_0_LOWEST = 0 .. EXT_6 = 6)
pub const IWL_SCAN_PRIORITY_EXT_6: u32 = 6;
/// `IWL_UMAC_SCAN_GEN_FLAGS_PASS_ALL`. // fw/api/scan.h:626
pub const IWL_UMAC_SCAN_GEN_FLAGS_PASS_ALL: u16 = 1 << 2;
/// `IWL_UMAC_SCAN_GEN_FLAGS_PASSIVE`. // fw/api/scan.h:627
pub const IWL_UMAC_SCAN_GEN_FLAGS_PASSIVE: u16 = 1 << 3;
/// `IWL_UMAC_SCAN_GEN_FLAGS_ADAPTIVE_DWELL`. // fw/api/scan.h:640
pub const IWL_UMAC_SCAN_GEN_FLAGS_ADAPTIVE_DWELL: u16 = 1 << 13;
/// `IWL_UMAC_SCAN_GEN_FLAGS2_ALLOW_CHNL_REORDER`. // fw/api/scan.h:656
pub const IWL_UMAC_SCAN_GEN_FLAGS2_ALLOW_CHNL_REORDER: u8 = 1 << 1;
/// `IWL_SCAN_CHANNEL_FLAG_EBS`. // fw/api/scan.h:269
pub const IWL_SCAN_CHANNEL_FLAG_EBS: u8 = 1 << 0;
/// `IWL_SCAN_CHANNEL_FLAG_EBS_ACCURATE`. // fw/api/scan.h:270
pub const IWL_SCAN_CHANNEL_FLAG_EBS_ACCURATE: u8 = 1 << 1;
/// `IWL_SCAN_CHANNEL_FLAG_CACHE_ADD`. // fw/api/scan.h:271
pub const IWL_SCAN_CHANNEL_FLAG_CACHE_ADD: u8 = 1 << 2;
/// `IWL_UCODE_TLV_FLAGS_EBS_SUPPORT`. // fw/file.h:203
pub const IWL_UCODE_TLV_FLAGS_EBS_SUPPORT: u32 = 1 << 25;
/// `PROBE_OPTION_MAX`. // fw/api/scan.h:27
pub const PROBE_OPTION_MAX: usize = 20;
/// `SCAN_OFFLOAD_PROBE_REQ_SIZE`. // fw/api/scan.h:55
pub const SCAN_OFFLOAD_PROBE_REQ_SIZE: usize = 512;
/// `IWL_MAX_SCHED_SCAN_PLANS`. // fw/api/scan.h:67
pub const IWL_MAX_SCHED_SCAN_PLANS: usize = 2;
/// `IEEE80211_MAX_SSID_LEN`. // include/linux/ieee80211.h
pub const IEEE80211_MAX_SSID_LEN: usize = 32;
/// `sizeof(struct iwl_ssid_ie)`. // fw/api/scan.h:44-48
pub const SSID_IE_LEN: usize = 2 + IEEE80211_MAX_SSID_LEN;
/// `sizeof(struct iwl_scan_channel_cfg_umac)`. // fw/api/scan.h (flags le32, channel_num u8, 3-byte union)
pub const SCAN_CHANNEL_CFG_UMAC_LEN: usize = 8;
/// `sizeof(struct iwl_scan_req_umac_tail_v1)`: schedule[2] + delay + reserved + probe_req_v1 + direct_scan[20].
/// // fw/api/scan.h:788-796
pub const SCAN_REQ_UMAC_TAIL_V1_LEN: usize = 12 + (4 + 4 * 2 + 4 + SCAN_OFFLOAD_PROBE_REQ_SIZE) + PROBE_OPTION_MAX * SSID_IE_LEN;
/// `IWL_SCAN_REQ_UMAC_SIZE_V8`: `sizeof(struct iwl_scan_req_umac)`. // fw/api/scan.h:940
pub const IWL_SCAN_REQ_UMAC_SIZE_V8: usize = 52;
/// `sizeof(struct iwl_umac_scan_complete)`. // fw/api/scan.h:1169-1177
pub const UMAC_SCAN_COMPLETE_LEN: usize = 16;
/// `sizeof(struct iwl_umac_scan_iter_complete_notif)` without `results[]`. // fw/api/scan.h:1284-1292
pub const UMAC_SCAN_ITER_COMPLETE_HDR_LEN: usize = 16;
/// `IWL_SCAN_OFFLOAD_COMPLETED`. // fw/api/scan.h:184
pub const IWL_SCAN_OFFLOAD_COMPLETED: u8 = 1;
/// `IWL_SCAN_OFFLOAD_ABORTED`. // fw/api/scan.h:185
pub const IWL_SCAN_OFFLOAD_ABORTED: u8 = 2;
/// `SCAN_CONFIG_*` flag bits. // fw/api/scan.h:452-474
pub const SCAN_CONFIG_FLAG_ACTIVATE: u32 = 1 << 0;
pub const SCAN_CONFIG_FLAG_ALLOW_CHUB_REQS: u32 = 1 << 3;
pub const SCAN_CONFIG_FLAG_SET_TX_CHAINS: u32 = 1 << 8;
pub const SCAN_CONFIG_FLAG_SET_RX_CHAINS: u32 = 1 << 9;
pub const SCAN_CONFIG_FLAG_SET_AUX_STA_ID: u32 = 1 << 10;
pub const SCAN_CONFIG_FLAG_SET_ALL_TIMES: u32 = 1 << 11;
pub const SCAN_CONFIG_FLAG_SET_CHANNEL_FLAGS: u32 = 1 << 13;
pub const SCAN_CONFIG_FLAG_SET_LEGACY_RATES: u32 = 1 << 14;
pub const SCAN_CONFIG_FLAG_SET_MAC_ADDR: u32 = 1 << 15;
pub const SCAN_CONFIG_FLAG_CLEAR_FRAGMENTED: u32 = 1 << 17;
/// `SCAN_CONFIG_N_CHANNELS(n)`: bits 26-31. // fw/api/scan.h:474
pub const SCAN_CONFIG_N_CHANNELS_POS: u32 = 26;
/// `scan_config_rates`: OFDM bits 0-7, CCK bits 8-11; supported rates are the same bits << 16. // fw/api/scan.h:479-494
pub const SCAN_CONFIG_RATES_ALL: u32 = 0xFFF;
pub const SCAN_CONFIG_SUPPORTED_RATE_POS: u32 = 16;
/// `iwl_channel_flags`. // fw/api/scan.h:497-502
pub const IWL_CHANNEL_FLAG_EBS: u8 = 1 << 0;
pub const IWL_CHANNEL_FLAG_ACCURATE_EBS: u8 = 1 << 1;
pub const IWL_CHANNEL_FLAG_EBS_ADD: u8 = 1 << 2;
pub const IWL_CHANNEL_FLAG_PRE_SCAN_PASSIVE2ACTIVE: u8 = 1 << 3;
/// `sizeof(struct iwl_scan_config_v1)` without `channel_array[]`. // fw/api/scan.h:538-550
pub const SCAN_CONFIG_V1_HDR_LEN: usize = 4 * 6 + 4 + 6 + 1 + 1;

// ---- RX ------------------------------------------------------------------------------------------------------
/// `IWL_RX_DESC_SIZE_V1`: `offsetofend(struct iwl_rx_mpdu_desc, v1)`. // fw/api/rx.h:737
pub const IWL_RX_DESC_SIZE_V1: usize = 48;
/// `IWL_RX_MPDU_STATUS_CRC_OK` / `_OVERRUN_OK`. // fw/api/rx.h:228-229
pub const IWL_RX_MPDU_STATUS_CRC_OK: u32 = 1 << 0;
pub const IWL_RX_MPDU_STATUS_OVERRUN_OK: u32 = 1 << 1;
/// `IWL_RX_MPDU_MFLG1_MIC_CRC_LEN_MASK`, applied as `u8_get_bits(…) << 1`. // fw/api/rx.h:177; mvm/rxmq.c:99-100
pub const IWL_RX_MPDU_MFLG1_MIC_CRC_LEN_MASK: u8 = 0xf0;
/// `IWL_RX_MPDU_MFLG2_PAD`. // fw/api/rx.h:187
pub const IWL_RX_MPDU_MFLG2_PAD: u8 = 0x20;
