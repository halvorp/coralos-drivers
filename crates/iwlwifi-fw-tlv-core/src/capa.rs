// SPDX-License-Identifier: GPL-2.0-only
//! The firmware's API and capability bitmaps, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/fw/file.h:271-566` (the two enums),
//! `drivers/net/wireless/intel/iwlwifi/fw/img.h:45-74` (the bitmaps and `fw_has_api` /
//! `fw_has_capa`) and `drivers/net/wireless/intel/iwlwifi/iwl-drv.c:540-580` (how a TLV word fills
//! them).
//!
//! Copyright (C) 2008-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! Every member of both enums is ported (34 API bits, 90 capability bits), each with its file.h line;
//! [`API_BITS`] / [`CAPA_BITS`] name them in enum order.

/// `NUM_IWL_UCODE_TLV_API`: one past the last API bit (SMART_FIFO_OFFLOAD = 68). // file.h:308-310
pub const NUM_IWL_UCODE_TLV_API: u32 = API_SMART_FIFO_OFFLOAD + 1;
/// `NUM_IWL_UCODE_TLV_CAPA`: one past the last capability bit (NAN_SYNC_SUPPORT = 4*32+2). // file.h:554-555
pub const NUM_IWL_UCODE_TLV_CAPA: u32 = CAPA_NAN_SYNC_SUPPORT + 1;
/// `DIV_ROUND_UP(NUM_IWL_UCODE_TLV_API, 32)`: the API words a TLV may index. // iwl-drv.c:548
pub const API_WORDS: usize = NUM_IWL_UCODE_TLV_API.div_ceil(32) as usize;
/// `DIV_ROUND_UP(NUM_IWL_UCODE_TLV_CAPA, 32)`. // iwl-drv.c:569
pub const CAPA_WORDS: usize = NUM_IWL_UCODE_TLV_CAPA.div_ceil(32) as usize;

/// A `DECLARE_BITMAP` of `W` 32-bit words (img.h:55-56), filled as iwl-drv.c:555-558 / :576-579 fill
/// it: bit `i` of the word at `api_index` is bit `i + 32 * api_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bits<const W: usize>(pub [u32; W]);

impl<const W: usize> Default for Bits<W> {
    fn default() -> Self {
        Bits([0; W])
    }
}

/// An `api_index` past the bitmap. Linux warns and DROPS the word (iwl-drv.c:548-553, :569-574);
/// the port reports it so the caller can log it by name, and the bitmap is left unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexPastBitmap {
    pub index: u32,
    pub words: usize,
    pub flags: u32,
}

impl<const W: usize> Bits<W> {
    /// `test_bit` (img.h:66, :73). A bit past the bitmap is not set.
    pub fn has(&self, bit: u32) -> bool {
        self.0.get((bit / 32) as usize).is_some_and(|w| w & (1 << (bit % 32)) != 0)
    }

    /// OR one TLV word in at `index`. // iwl-drv.c:555-558, :576-579
    pub fn set_word(&mut self, index: u32, flags: u32) -> Result<(), IndexPastBitmap> {
        match self.0.get_mut(index as usize) {
            Some(w) => {
                *w |= flags;
                Ok(())
            }
            None => Err(IndexPastBitmap { index, words: W, flags }),
        }
    }

    /// Every set bit, ascending.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        (0..(W as u32) * 32).filter(move |&b| self.has(b))
    }
}

/// `_api`: the API_CHANGES_SET bitmap. // img.h:55
pub type ApiSet = Bits<API_WORDS>;
/// `_capa`: the ENABLED_CAPABILITIES bitmap. // img.h:56
pub type CapaSet = Bits<CAPA_WORDS>;

// ── enum iwl_ucode_tlv_api ── file.h:271-321
pub const API_FRAGMENTED_SCAN: u32 = 8; // file.h:273
pub const API_WIFI_MCC_UPDATE: u32 = 9; // file.h:274
pub const API_LQ_SS_PARAMS: u32 = 18; // file.h:275
pub const API_NEW_VERSION: u32 = 20; // file.h:276
pub const API_SCAN_TSF_REPORT: u32 = 28; // file.h:277
pub const API_TKIP_MIC_KEYS: u32 = 29; // file.h:278
pub const API_STA_TYPE: u32 = 30; // file.h:279
pub const API_NAN2_VER2: u32 = 31; // file.h:280
pub const API_ADAPTIVE_DWELL: u32 = 32; // file.h:282
pub const API_OCE: u32 = 33; // file.h:283
pub const API_NEW_BEACON_TEMPLATE: u32 = 34; // file.h:284
pub const API_NEW_RX_STATS: u32 = 35; // file.h:285
pub const API_WOWLAN_KEY_MATERIAL: u32 = 36; // file.h:286
pub const API_QUOTA_LOW_LATENCY: u32 = 38; // file.h:287
pub const API_DEPRECATE_TTAK: u32 = 41; // file.h:288
pub const API_ADAPTIVE_DWELL_V2: u32 = 42; // file.h:289
pub const API_FRAG_EBS: u32 = 44; // file.h:290
pub const API_REDUCE_TX_POWER: u32 = 45; // file.h:291
pub const API_SHORT_BEACON_NOTIF: u32 = 46; // file.h:292
pub const API_BEACON_FILTER_V4: u32 = 47; // file.h:293
pub const API_REGULATORY_NVM_INFO: u32 = 48; // file.h:294
pub const API_FTM_NEW_RANGE_REQ: u32 = 49; // file.h:295
pub const API_SCAN_OFFLOAD_CHANS: u32 = 50; // file.h:296
pub const API_MBSSID_HE: u32 = 52; // file.h:297
pub const API_WOWLAN_TCP_SYN_WAKE: u32 = 53; // file.h:298
pub const API_FTM_RTT_ACCURACY: u32 = 54; // file.h:299
pub const API_SAR_TABLE_VER: u32 = 55; // file.h:300
pub const API_REDUCED_SCAN_CONFIG: u32 = 56; // file.h:301
pub const API_ADWELL_HB_DEF_N_AP: u32 = 57; // file.h:302
pub const API_SCAN_EXT_CHAN_VER: u32 = 58; // file.h:303
pub const API_BAND_IN_RX_DATA: u32 = 59; // file.h:304
pub const API_NO_HOST_DISABLE_TX: u32 = 66; // file.h:306
pub const API_INT_DBG_BUF_CLEAR: u32 = 67; // file.h:307
pub const API_SMART_FIFO_OFFLOAD: u32 = 68; // file.h:308

/// Every `enum iwl_ucode_tlv_api` member, in enum order. // file.h:271-309
pub const API_BITS: [(&str, u32); 34] = [
    ("FRAGMENTED_SCAN", API_FRAGMENTED_SCAN),
    ("WIFI_MCC_UPDATE", API_WIFI_MCC_UPDATE),
    ("LQ_SS_PARAMS", API_LQ_SS_PARAMS),
    ("NEW_VERSION", API_NEW_VERSION),
    ("SCAN_TSF_REPORT", API_SCAN_TSF_REPORT),
    ("TKIP_MIC_KEYS", API_TKIP_MIC_KEYS),
    ("STA_TYPE", API_STA_TYPE),
    ("NAN2_VER2", API_NAN2_VER2),
    ("ADAPTIVE_DWELL", API_ADAPTIVE_DWELL),
    ("OCE", API_OCE),
    ("NEW_BEACON_TEMPLATE", API_NEW_BEACON_TEMPLATE),
    ("NEW_RX_STATS", API_NEW_RX_STATS),
    ("WOWLAN_KEY_MATERIAL", API_WOWLAN_KEY_MATERIAL),
    ("QUOTA_LOW_LATENCY", API_QUOTA_LOW_LATENCY),
    ("DEPRECATE_TTAK", API_DEPRECATE_TTAK),
    ("ADAPTIVE_DWELL_V2", API_ADAPTIVE_DWELL_V2),
    ("FRAG_EBS", API_FRAG_EBS),
    ("REDUCE_TX_POWER", API_REDUCE_TX_POWER),
    ("SHORT_BEACON_NOTIF", API_SHORT_BEACON_NOTIF),
    ("BEACON_FILTER_V4", API_BEACON_FILTER_V4),
    ("REGULATORY_NVM_INFO", API_REGULATORY_NVM_INFO),
    ("FTM_NEW_RANGE_REQ", API_FTM_NEW_RANGE_REQ),
    ("SCAN_OFFLOAD_CHANS", API_SCAN_OFFLOAD_CHANS),
    ("MBSSID_HE", API_MBSSID_HE),
    ("WOWLAN_TCP_SYN_WAKE", API_WOWLAN_TCP_SYN_WAKE),
    ("FTM_RTT_ACCURACY", API_FTM_RTT_ACCURACY),
    ("SAR_TABLE_VER", API_SAR_TABLE_VER),
    ("REDUCED_SCAN_CONFIG", API_REDUCED_SCAN_CONFIG),
    ("ADWELL_HB_DEF_N_AP", API_ADWELL_HB_DEF_N_AP),
    ("SCAN_EXT_CHAN_VER", API_SCAN_EXT_CHAN_VER),
    ("BAND_IN_RX_DATA", API_BAND_IN_RX_DATA),
    ("NO_HOST_DISABLE_TX", API_NO_HOST_DISABLE_TX),
    ("INT_DBG_BUF_CLEAR", API_INT_DBG_BUF_CLEAR),
    ("SMART_FIFO_OFFLOAD", API_SMART_FIFO_OFFLOAD),
];

// ── enum iwl_ucode_tlv_capa ── file.h:454-566
pub const CAPA_D0I3_SUPPORT: u32 = 0; // file.h:456
pub const CAPA_LAR_SUPPORT: u32 = 1; // file.h:457
pub const CAPA_UMAC_SCAN: u32 = 2; // file.h:458
pub const CAPA_BEAMFORMER: u32 = 3; // file.h:459
pub const CAPA_TDLS_SUPPORT: u32 = 6; // file.h:460
pub const CAPA_TXPOWER_INSERTION_SUPPORT: u32 = 8; // file.h:461
pub const CAPA_DS_PARAM_SET_IE_SUPPORT: u32 = 9; // file.h:462
pub const CAPA_WFA_TPC_REP_IE_SUPPORT: u32 = 10; // file.h:463
pub const CAPA_QUIET_PERIOD_SUPPORT: u32 = 11; // file.h:464
pub const CAPA_DQA_SUPPORT: u32 = 12; // file.h:465
pub const CAPA_TDLS_CHANNEL_SWITCH: u32 = 13; // file.h:466
pub const CAPA_CNSLDTD_D3_D0_IMG: u32 = 17; // file.h:467
pub const CAPA_HOTSPOT_SUPPORT: u32 = 18; // file.h:468
pub const CAPA_CSUM_SUPPORT: u32 = 21; // file.h:469
pub const CAPA_RADIO_BEACON_STATS: u32 = 22; // file.h:470
pub const CAPA_P2P_SCM_UAPSD: u32 = 26; // file.h:471
pub const CAPA_BT_COEX_PLCR: u32 = 28; // file.h:472
pub const CAPA_LAR_MULTI_MCC: u32 = 29; // file.h:473
pub const CAPA_BT_COEX_RRC: u32 = 30; // file.h:474
pub const CAPA_GSCAN_SUPPORT: u32 = 31; // file.h:475
pub const CAPA_FRAGMENTED_PNVM_IMG: u32 = 32; // file.h:478
pub const CAPA_SOC_LATENCY_SUPPORT: u32 = 37; // file.h:479
pub const CAPA_STA_PM_NOTIF: u32 = 38; // file.h:480
pub const CAPA_BINDING_CDB_SUPPORT: u32 = 39; // file.h:481
pub const CAPA_CDB_SUPPORT: u32 = 40; // file.h:482
pub const CAPA_D0I3_END_FIRST: u32 = 41; // file.h:483
pub const CAPA_TLC_OFFLOAD: u32 = 43; // file.h:484
pub const CAPA_DYNAMIC_QUOTA: u32 = 44; // file.h:485
pub const CAPA_COEX_SCHEMA_2: u32 = 45; // file.h:486
pub const CAPA_CHANNEL_SWITCH_CMD: u32 = 46; // file.h:487
pub const CAPA_FTM_CALIBRATED: u32 = 47; // file.h:488
pub const CAPA_ULTRA_HB_CHANNELS: u32 = 48; // file.h:489
pub const CAPA_CS_MODIFY: u32 = 49; // file.h:490
pub const CAPA_SET_LTR_GEN2: u32 = 50; // file.h:491
pub const CAPA_SET_PPAG: u32 = 52; // file.h:492
pub const CAPA_TAS_CFG: u32 = 53; // file.h:493
pub const CAPA_SESSION_PROT_CMD: u32 = 54; // file.h:494
pub const CAPA_PROTECTED_TWT: u32 = 56; // file.h:495
pub const CAPA_FW_RESET_HANDSHAKE: u32 = 57; // file.h:496
pub const CAPA_PASSIVE_6GHZ_SCAN: u32 = 58; // file.h:497
pub const CAPA_HIDDEN_6GHZ_SCAN: u32 = 59; // file.h:498
pub const CAPA_BROADCAST_TWT: u32 = 60; // file.h:499
pub const CAPA_COEX_HIGH_PRIO: u32 = 61; // file.h:500
pub const CAPA_RFIM_SUPPORT: u32 = 62; // file.h:501
pub const CAPA_BAID_ML_SUPPORT: u32 = 63; // file.h:502
pub const CAPA_EXTENDED_DTS_MEASURE: u32 = 64; // file.h:505
pub const CAPA_SHORT_PM_TIMEOUTS: u32 = 65; // file.h:506
pub const CAPA_BT_MPLUT_SUPPORT: u32 = 67; // file.h:507
pub const CAPA_MULTI_QUEUE_RX_SUPPORT: u32 = 68; // file.h:508
pub const CAPA_CSA_AND_TBTT_OFFLOAD: u32 = 70; // file.h:509
pub const CAPA_BEACON_ANT_SELECTION: u32 = 71; // file.h:510
pub const CAPA_BEACON_STORING: u32 = 72; // file.h:511
pub const CAPA_LAR_SUPPORT_V3: u32 = 73; // file.h:512
pub const CAPA_CT_KILL_BY_FW: u32 = 74; // file.h:513
pub const CAPA_TEMP_THS_REPORT_SUPPORT: u32 = 75; // file.h:514
pub const CAPA_CTDP_SUPPORT: u32 = 76; // file.h:515
pub const CAPA_USNIFFER_UNIFIED: u32 = 77; // file.h:516
pub const CAPA_EXTEND_SHARED_MEM_CFG: u32 = 80; // file.h:517
pub const CAPA_LQM_SUPPORT: u32 = 81; // file.h:518
pub const CAPA_TX_POWER_ACK: u32 = 84; // file.h:519
pub const CAPA_D3_DEBUG: u32 = 87; // file.h:520
pub const CAPA_LED_CMD_SUPPORT: u32 = 88; // file.h:521
pub const CAPA_MCC_UPDATE_11AX_SUPPORT: u32 = 89; // file.h:522
pub const CAPA_CSI_REPORTING: u32 = 90; // file.h:523
pub const CAPA_DBG_SUSPEND_RESUME_CMD_SUPP: u32 = 92; // file.h:524
pub const CAPA_DBG_BUF_ALLOC_CMD_SUPP: u32 = 93; // file.h:525
pub const CAPA_BIOS_OVERRIDE_UNII4_US_CA: u32 = 96; // file.h:528
pub const CAPA_PSC_CHAN_SUPPORT: u32 = 98; // file.h:529
pub const CAPA_BIGTK_SUPPORT: u32 = 100; // file.h:530
pub const CAPA_SPP_AMSDU_SUPPORT: u32 = 103; // file.h:531
pub const CAPA_DRAM_FRAG_SUPPORT: u32 = 104; // file.h:532
pub const CAPA_DUMP_COMPLETE_SUPPORT: u32 = 105; // file.h:533
pub const CAPA_SYNCED_TIME: u32 = 106; // file.h:534
pub const CAPA_TIME_SYNC_BOTH_FTM_TM: u32 = 108; // file.h:535
pub const CAPA_BIGTK_TX_SUPPORT: u32 = 109; // file.h:536
pub const CAPA_MLD_API_SUPPORT: u32 = 110; // file.h:537
pub const CAPA_SCAN_DONT_TOGGLE_ANT: u32 = 111; // file.h:538
pub const CAPA_PPAG_CHINA_BIOS_SUPPORT: u32 = 112; // file.h:539
pub const CAPA_OFFLOAD_BTM_SUPPORT: u32 = 113; // file.h:540
pub const CAPA_STA_EXP_MFP_SUPPORT: u32 = 114; // file.h:541
pub const CAPA_SNIFF_VALIDATE_SUPPORT: u32 = 116; // file.h:542
pub const CAPA_CHINA_22_REG_SUPPORT: u32 = 117; // file.h:543
pub const CAPA_SECURE_LTF_SUPPORT: u32 = 121; // file.h:544
pub const CAPA_MONITOR_PASSIVE_CHANS: u32 = 122; // file.h:545
pub const CAPA_BIOS_OVERRIDE_5G9_FOR_CA: u32 = 123; // file.h:546
pub const CAPA_UHB_CANADA_TAS_SUPPORT: u32 = 124; // file.h:547
pub const CAPA_EXT_FSEQ_IMAGE_SUPPORT: u32 = 125; // file.h:548
pub const CAPA_RESET_DURING_ASSERT: u32 = 4*32+0; // file.h:552
pub const CAPA_FW_ACCEPTS_RAW_DSM_TABLE: u32 = 4*32+1; // file.h:553
pub const CAPA_NAN_SYNC_SUPPORT: u32 = 4*32+2; // file.h:554

/// Every `enum iwl_ucode_tlv_capa` member, in enum order. // file.h:454-554
pub const CAPA_BITS: [(&str, u32); 90] = [
    ("D0I3_SUPPORT", CAPA_D0I3_SUPPORT),
    ("LAR_SUPPORT", CAPA_LAR_SUPPORT),
    ("UMAC_SCAN", CAPA_UMAC_SCAN),
    ("BEAMFORMER", CAPA_BEAMFORMER),
    ("TDLS_SUPPORT", CAPA_TDLS_SUPPORT),
    ("TXPOWER_INSERTION_SUPPORT", CAPA_TXPOWER_INSERTION_SUPPORT),
    ("DS_PARAM_SET_IE_SUPPORT", CAPA_DS_PARAM_SET_IE_SUPPORT),
    ("WFA_TPC_REP_IE_SUPPORT", CAPA_WFA_TPC_REP_IE_SUPPORT),
    ("QUIET_PERIOD_SUPPORT", CAPA_QUIET_PERIOD_SUPPORT),
    ("DQA_SUPPORT", CAPA_DQA_SUPPORT),
    ("TDLS_CHANNEL_SWITCH", CAPA_TDLS_CHANNEL_SWITCH),
    ("CNSLDTD_D3_D0_IMG", CAPA_CNSLDTD_D3_D0_IMG),
    ("HOTSPOT_SUPPORT", CAPA_HOTSPOT_SUPPORT),
    ("CSUM_SUPPORT", CAPA_CSUM_SUPPORT),
    ("RADIO_BEACON_STATS", CAPA_RADIO_BEACON_STATS),
    ("P2P_SCM_UAPSD", CAPA_P2P_SCM_UAPSD),
    ("BT_COEX_PLCR", CAPA_BT_COEX_PLCR),
    ("LAR_MULTI_MCC", CAPA_LAR_MULTI_MCC),
    ("BT_COEX_RRC", CAPA_BT_COEX_RRC),
    ("GSCAN_SUPPORT", CAPA_GSCAN_SUPPORT),
    ("FRAGMENTED_PNVM_IMG", CAPA_FRAGMENTED_PNVM_IMG),
    ("SOC_LATENCY_SUPPORT", CAPA_SOC_LATENCY_SUPPORT),
    ("STA_PM_NOTIF", CAPA_STA_PM_NOTIF),
    ("BINDING_CDB_SUPPORT", CAPA_BINDING_CDB_SUPPORT),
    ("CDB_SUPPORT", CAPA_CDB_SUPPORT),
    ("D0I3_END_FIRST", CAPA_D0I3_END_FIRST),
    ("TLC_OFFLOAD", CAPA_TLC_OFFLOAD),
    ("DYNAMIC_QUOTA", CAPA_DYNAMIC_QUOTA),
    ("COEX_SCHEMA_2", CAPA_COEX_SCHEMA_2),
    ("CHANNEL_SWITCH_CMD", CAPA_CHANNEL_SWITCH_CMD),
    ("FTM_CALIBRATED", CAPA_FTM_CALIBRATED),
    ("ULTRA_HB_CHANNELS", CAPA_ULTRA_HB_CHANNELS),
    ("CS_MODIFY", CAPA_CS_MODIFY),
    ("SET_LTR_GEN2", CAPA_SET_LTR_GEN2),
    ("SET_PPAG", CAPA_SET_PPAG),
    ("TAS_CFG", CAPA_TAS_CFG),
    ("SESSION_PROT_CMD", CAPA_SESSION_PROT_CMD),
    ("PROTECTED_TWT", CAPA_PROTECTED_TWT),
    ("FW_RESET_HANDSHAKE", CAPA_FW_RESET_HANDSHAKE),
    ("PASSIVE_6GHZ_SCAN", CAPA_PASSIVE_6GHZ_SCAN),
    ("HIDDEN_6GHZ_SCAN", CAPA_HIDDEN_6GHZ_SCAN),
    ("BROADCAST_TWT", CAPA_BROADCAST_TWT),
    ("COEX_HIGH_PRIO", CAPA_COEX_HIGH_PRIO),
    ("RFIM_SUPPORT", CAPA_RFIM_SUPPORT),
    ("BAID_ML_SUPPORT", CAPA_BAID_ML_SUPPORT),
    ("EXTENDED_DTS_MEASURE", CAPA_EXTENDED_DTS_MEASURE),
    ("SHORT_PM_TIMEOUTS", CAPA_SHORT_PM_TIMEOUTS),
    ("BT_MPLUT_SUPPORT", CAPA_BT_MPLUT_SUPPORT),
    ("MULTI_QUEUE_RX_SUPPORT", CAPA_MULTI_QUEUE_RX_SUPPORT),
    ("CSA_AND_TBTT_OFFLOAD", CAPA_CSA_AND_TBTT_OFFLOAD),
    ("BEACON_ANT_SELECTION", CAPA_BEACON_ANT_SELECTION),
    ("BEACON_STORING", CAPA_BEACON_STORING),
    ("LAR_SUPPORT_V3", CAPA_LAR_SUPPORT_V3),
    ("CT_KILL_BY_FW", CAPA_CT_KILL_BY_FW),
    ("TEMP_THS_REPORT_SUPPORT", CAPA_TEMP_THS_REPORT_SUPPORT),
    ("CTDP_SUPPORT", CAPA_CTDP_SUPPORT),
    ("USNIFFER_UNIFIED", CAPA_USNIFFER_UNIFIED),
    ("EXTEND_SHARED_MEM_CFG", CAPA_EXTEND_SHARED_MEM_CFG),
    ("LQM_SUPPORT", CAPA_LQM_SUPPORT),
    ("TX_POWER_ACK", CAPA_TX_POWER_ACK),
    ("D3_DEBUG", CAPA_D3_DEBUG),
    ("LED_CMD_SUPPORT", CAPA_LED_CMD_SUPPORT),
    ("MCC_UPDATE_11AX_SUPPORT", CAPA_MCC_UPDATE_11AX_SUPPORT),
    ("CSI_REPORTING", CAPA_CSI_REPORTING),
    ("DBG_SUSPEND_RESUME_CMD_SUPP", CAPA_DBG_SUSPEND_RESUME_CMD_SUPP),
    ("DBG_BUF_ALLOC_CMD_SUPP", CAPA_DBG_BUF_ALLOC_CMD_SUPP),
    ("BIOS_OVERRIDE_UNII4_US_CA", CAPA_BIOS_OVERRIDE_UNII4_US_CA),
    ("PSC_CHAN_SUPPORT", CAPA_PSC_CHAN_SUPPORT),
    ("BIGTK_SUPPORT", CAPA_BIGTK_SUPPORT),
    ("SPP_AMSDU_SUPPORT", CAPA_SPP_AMSDU_SUPPORT),
    ("DRAM_FRAG_SUPPORT", CAPA_DRAM_FRAG_SUPPORT),
    ("DUMP_COMPLETE_SUPPORT", CAPA_DUMP_COMPLETE_SUPPORT),
    ("SYNCED_TIME", CAPA_SYNCED_TIME),
    ("TIME_SYNC_BOTH_FTM_TM", CAPA_TIME_SYNC_BOTH_FTM_TM),
    ("BIGTK_TX_SUPPORT", CAPA_BIGTK_TX_SUPPORT),
    ("MLD_API_SUPPORT", CAPA_MLD_API_SUPPORT),
    ("SCAN_DONT_TOGGLE_ANT", CAPA_SCAN_DONT_TOGGLE_ANT),
    ("PPAG_CHINA_BIOS_SUPPORT", CAPA_PPAG_CHINA_BIOS_SUPPORT),
    ("OFFLOAD_BTM_SUPPORT", CAPA_OFFLOAD_BTM_SUPPORT),
    ("STA_EXP_MFP_SUPPORT", CAPA_STA_EXP_MFP_SUPPORT),
    ("SNIFF_VALIDATE_SUPPORT", CAPA_SNIFF_VALIDATE_SUPPORT),
    ("CHINA_22_REG_SUPPORT", CAPA_CHINA_22_REG_SUPPORT),
    ("SECURE_LTF_SUPPORT", CAPA_SECURE_LTF_SUPPORT),
    ("MONITOR_PASSIVE_CHANS", CAPA_MONITOR_PASSIVE_CHANS),
    ("BIOS_OVERRIDE_5G9_FOR_CA", CAPA_BIOS_OVERRIDE_5G9_FOR_CA),
    ("UHB_CANADA_TAS_SUPPORT", CAPA_UHB_CANADA_TAS_SUPPORT),
    ("EXT_FSEQ_IMAGE_SUPPORT", CAPA_EXT_FSEQ_IMAGE_SUPPORT),
    ("RESET_DURING_ASSERT", CAPA_RESET_DURING_ASSERT),
    ("FW_ACCEPTS_RAW_DSM_TABLE", CAPA_FW_ACCEPTS_RAW_DSM_TABLE),
    ("NAN_SYNC_SUPPORT", CAPA_NAN_SYNC_SUPPORT),
];
