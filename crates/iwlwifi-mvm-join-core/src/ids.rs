// SPDX-License-Identifier: GPL-2.0-only
//! Command ids, flags, sizes and field values, each with the Linux FILE:LINE it was read from
//! (Linux v7.0-rc6, `drivers/net/wireless/intel/iwlwifi/`).

// ---- command ids (LEGACY_GROUP) ----
pub const REMOVE_STA: u8 = 0x19; // fw/api/commands.h:145
pub const TX_CMD: u8 = 0x1c; // fw/api/commands.h:153
pub const TIME_EVENT_CMD: u8 = 0x29; // fw/api/commands.h:214
pub const TIME_EVENT_NOTIFICATION: u8 = 0x2a; // fw/api/commands.h:219
pub const BINDING_CONTEXT_CMD: u8 = 0x2b; // fw/api/commands.h:225
pub const MCAST_FILTER_CMD: u8 = 0xd0; // fw/api/commands.h:527

// ---- context ids, colours and actions ----
pub const FW_CTXT_ID_POS: u32 = 0; // fw/api/context.h:22
pub const FW_CTXT_COLOR_POS: u32 = 8; // fw/api/context.h:24
pub const FW_CTXT_INVALID: u32 = 0xffff_ffff; // fw/api/context.h:26
pub const FW_CTXT_ACTION_ADD: u32 = 1; // fw/api/context.h:42
pub const FW_CTXT_ACTION_MODIFY: u32 = 2; // fw/api/context.h:43
pub const FW_CTXT_ACTION_REMOVE: u32 = 3; // fw/api/context.h:44

// ---- PHY context ----
pub const PHY_BAND_5: u8 = 0; // fw/api/phy-ctxt.h:11
pub const PHY_BAND_24: u8 = 1; // fw/api/phy-ctxt.h:12
pub const IWL_PHY_CHANNEL_MODE20: u8 = 0; // fw/api/phy-ctxt.h:16
pub const PHY_RX_CHAIN_VALID_POS: u32 = 1; // fw/api/phy-ctxt.h:76
pub const PHY_RX_CHAIN_CNT_POS: u32 = 10; // fw/api/phy-ctxt.h:85
pub const PHY_RX_CHAIN_MIMO_CNT_POS: u32 = 12; // fw/api/phy-ctxt.h:88
/// `sizeof(struct iwl_phy_context_cmd_v1)` with the 8-byte `iwl_fw_channel_info` (ULTRA_HB firmware).
pub const PHY_CTX_CMD_V1_UHB_LEN: usize = 40; // fw/api/phy-ctxt.h:126
/// The same without the 4 bytes `iwl_mvm_chan_info_padding` trims for a firmware without ULTRA_HB.
pub const PHY_CTX_CMD_V1_LEN: usize = 36; // mvm/mvm.h:2557

// ---- binding ----
pub const MAX_MACS_IN_BINDING: usize = 3; // fw/api/binding.h:13
/// `sizeof(struct iwl_binding_cmd_v1)`: id_and_color, action, macs[3], phy.
pub const BINDING_CMD_V1_LEN: usize = 24; // fw/api/binding.h:27

// ---- time event ----
pub const TE_BSS_STA_AGGRESSIVE_ASSOC: u32 = 0; // fw/api/time-event.h:13
pub const TE_V2_FRAG_NONE: u8 = 0; // fw/api/time-event.h:127
pub const TE_V2_NOTIF_HOST_EVENT_START: u16 = 1 << 0; // fw/api/time-event.h:168
pub const TE_V2_NOTIF_HOST_EVENT_END: u16 = 1 << 1; // fw/api/time-event.h:169
pub const TE_V2_START_IMMEDIATELY: u16 = 1 << 11; // fw/api/time-event.h:177
/// `sizeof(struct iwl_time_event_cmd)`.
pub const TIME_EVENT_CMD_LEN: usize = 36; // fw/api/time-event.h:214
/// `sizeof(struct iwl_time_event_resp)`.
pub const TIME_EVENT_RESP_LEN: usize = 16; // fw/api/time-event.h:238

// ---- station ----
pub const STA_MODE_ADD: u8 = 0; // fw/api/sta.h:171
pub const STA_MODE_MODIFY: u8 = 1; // fw/api/sta.h:172
pub const STA_MODIFY_TID_DISABLE_TX: u8 = 1 << 1; // fw/api/sta.h:156
pub const STA_MODIFY_QUEUES: u8 = 1 << 7; // fw/api/sta.h:162
pub const IWL_STA_LINK: u8 = 0; // fw/api/sta.h:276
pub const STA_FLG_RTS_MIMO_PROT: u32 = 1 << 17; // fw/api/sta.h:65
pub const STA_FLG_FAT_EN_MSK: u32 = 3 << 26; // fw/api/sta.h:91
pub const STA_FLG_MIMO_EN_MSK: u32 = 3 << 28; // fw/api/sta.h:96
/// `sizeof(struct iwl_mvm_add_sta_cmd)` (ADD_STA_CMD_API_S_VER_10).
pub const ADD_STA_CMD_LEN: usize = 48; // fw/api/sta.h:329
/// `sizeof(struct iwl_mvm_rm_sta_cmd)`.
pub const REMOVE_STA_CMD_LEN: usize = 4; // fw/api/sta.h:425
/// The AP's station id on a station interface (`iwl_mvm_find_free_sta_id`, mvm/sta.c: id 0 is free for a STATION vif).
pub const AP_STA_ID: u8 = 0; // mvm/sta.c:27

// ---- MAC context (the parts beyond the rt-core ADD) ----
pub const MAC_PROT_FLG_TGG_PROTECT: u32 = 1 << 3; // fw/api/mac.h:40
pub const MAC_FLG_SHORT_SLOT: u32 = 1 << 4; // fw/api/mac.h:46
pub const MAC_FLG_SHORT_PREAMBLE: u32 = 1 << 5; // fw/api/mac.h:47
pub const MAC_QOS_FLG_UPDATE_EDCA: u32 = 1 << 0; // fw/api/mac.h:263
pub const MAC_CTX_CMD_LEN: usize = 148; // fw/api/mac.h:321
/// `AC_NUM + 1` entries of `struct iwl_ac_qos` (the fifth is the CAB queue).
pub const MAC_CTX_AC_ENTRIES: usize = 5; // fw/api/mac.h:321
/// `hw->conf.listen_interval`, the mac80211 default of 10 beacon intervals.
pub const LISTEN_INTERVAL: u32 = 10; // mvm/mac-ctxt.c:717

// ---- TX ----
pub const TX_CMD_FLG_ACK: u32 = 1 << 3; // fw/api/tx.h:47
pub const TX_CMD_FLG_BT_DIS: u32 = 1 << 12; // fw/api/tx.h:56
pub const TX_CMD_FLG_SEQ_CTL: u32 = 1 << 13; // fw/api/tx.h:57
pub const TX_CMD_FLG_MH_PAD: u32 = 1 << 20; // fw/api/tx.h:62
pub const TX_CMD_SEC_CCM: u8 = 0x02; // fw/api/tx.h:123
pub const TX_CMD_SEC_MSK: u8 = 0x07; // fw/api/tx.h:104
pub const TX_CMD_LIFE_TIME_INFINITE: u32 = 0xffff_ffff; // fw/api/tx.h:134
pub const IWL_DEFAULT_TX_RETRY: u8 = 15; // fw/api/tx.h:147
pub const IWL_MGMT_DFAULT_RETRY_LIMIT: u8 = 3; // fw/api/tx.h:148
pub const IWL_RTS_DFAULT_RETRY_LIMIT: u8 = 60; // fw/api/tx.h:149
pub const PM_FRAME_NONE: u16 = 0; // fw/api/tx.h:99
pub const PM_FRAME_MGMT: u16 = 2; // fw/api/tx.h:100
pub const PM_FRAME_ASSOC: u16 = 3; // fw/api/tx.h:101
pub const IWL_TID_NON_QOS: u8 = 0; // fw/api/tx.h:142
pub const IWL_MAX_TID_COUNT: u8 = 8; // iwl-trans.h:271
pub const TX_CMD_OFFLD_MH_SIZE: u32 = 8; // fw/api/tx.h:174
pub const TX_CMD_OFFLD_PAD: u32 = 13; // fw/api/tx.h:175
/// `sizeof(struct iwl_tx_cmd_v6_params)`: the TX command before the 802.11 header.
pub const TX_CMD_PARAMS_LEN: usize = 56; // fw/api/tx.h:219
/// `sizeof(struct iwl_cmd_header)`: cmd, group, le16 sequence.
pub const CMD_HEADER_LEN: usize = 4; // fw/api/cmdhdr.h:65
/// Bytes of the device command in the first transfer buffer. // iwl-trans.h
pub const IWL_FIRST_TB_SIZE: usize = 20; // iwl-trans.h:712
pub const IWL_TX_CRC_SIZE: usize = 4; // pcie/gen1_2/tx.c:2055
pub const IWL_TX_DELIMITER_SIZE: usize = 4; // pcie/gen1_2/tx.c:2056
pub const TX_STATUS_MSK: u32 = 0xff; // fw/api/tx.h:352
pub const TX_STATUS_SUCCESS: u32 = 0x01; // fw/api/tx.h:353
pub const TX_STATUS_DIRECT_DONE: u32 = 0x02; // fw/api/tx.h:354
/// Offset of the first `agg_tx_status` in `TX_CMD`'s response for a firmware without the new TX API
/// (`struct iwl_tx_resp_v3`, the layout without `tx_queue` / `reserved2`).
pub const TX_RESP_STATUS_OFF: usize = 36; // fw/api/tx.h:584

// ---- queues ----
pub const IWL_MVM_DQA_MIN_MGMT_QUEUE: u8 = 5; // fw/api/txq.h:41
pub const IWL_MVM_TX_FIFO_BK: u8 = 0; // fw/api/txq.h:49
pub const IWL_MVM_TX_FIFO_BE: u8 = 1; // fw/api/txq.h:50
pub const IWL_MVM_TX_FIFO_VI: u8 = 2; // fw/api/txq.h:51
pub const IWL_MVM_TX_FIFO_VO: u8 = 3; // fw/api/txq.h:52

// ---- rates, version 1 (`rate_n_flags`) ----
pub const RATE_MCS_ANT_POS: u32 = 14; // fw/api/rs.h:544
pub const RATE_MCS_ANT_A_MSK: u32 = 1 << RATE_MCS_ANT_POS; // fw/api/rs.h:545
pub const RATE_MCS_ANT_B_MSK: u32 = 2 << RATE_MCS_ANT_POS; // fw/api/rs.h:546
pub const RATE_MCS_CCK_MSK_V1: u32 = 1 << 9; // fw/api/rs.h:449
pub const RATE_LEGACY_RATE_MSK_V1: u32 = 0xff; // fw/api/rs.h:526

// ---- RX descriptor status ----
pub const IWL_RX_MPDU_STATUS_CRC_OK: u32 = 1 << 0; // fw/api/rx.h:228
pub const IWL_RX_MPDU_STATUS_OVERRUN_OK: u32 = 1 << 1; // fw/api/rx.h:229
pub const IWL_RX_MPDU_STATUS_KEY_VALID: u32 = 1 << 3; // fw/api/rx.h:231
pub const IWL_RX_MPDU_STATUS_MIC_OK: u32 = 1 << 6; // fw/api/rx.h:233
pub const IWL_RX_MPDU_STATUS_REPLAY_ERROR: u32 = 1 << 7; // fw/api/rx.h:236
pub const IWL_RX_MPDU_STATUS_SEC_MASK: u32 = 0x7 << 8; // fw/api/rx.h:237
pub const IWL_RX_MPDU_STATUS_SEC_NONE: u32 = 0x0 << 8; // fw/api/rx.h:239
pub const IWL_RX_MPDU_STATUS_SEC_CCM: u32 = 0x2 << 8; // fw/api/rx.h:241
pub const IWL_RX_MPDU_STATUS_DECRYPTED: u32 = 1 << 11; // fw/api/rx.h:245
/// Offset of `desc->status` in the version-1 RX descriptor. // fw/api/rx.h (struct iwl_rx_mpdu_desc)
pub const RX_DESC_STATUS_OFF: usize = 12; // fw/api/rx.h:653
