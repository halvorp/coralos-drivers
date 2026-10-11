// SPDX-License-Identifier: GPL-2.0-only
//! Linux-literal vectors for the API / capability bitmaps.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/file.h` (the enums),
//! `drivers/net/wireless/intel/iwlwifi/fw/img.h` (`fw_has_api` / `fw_has_capa`) and
//! `drivers/net/wireless/intel/iwlwifi/iwl-drv.c` (`iwl_set_ucode_api_flags` /
//! `iwl_set_ucode_capabilities`). Copyright (C) 2008-2014, 2018-2025 Intel Corporation, and the Linux
//! iwlwifi authors.
//!
//! The expected (name, bit) pairs below are typed out from file.h, one line each — never derived from
//! the production tables they check.

use iwlwifi_fw_tlv_core::capa::*;

const API_EXPECTED: [(&str, u32); 34] = [
        ("FRAGMENTED_SCAN", 8), // file.h:273
        ("WIFI_MCC_UPDATE", 9), // file.h:274
        ("LQ_SS_PARAMS", 18), // file.h:275
        ("NEW_VERSION", 20), // file.h:276
        ("SCAN_TSF_REPORT", 28), // file.h:277
        ("TKIP_MIC_KEYS", 29), // file.h:278
        ("STA_TYPE", 30), // file.h:279
        ("NAN2_VER2", 31), // file.h:280
        ("ADAPTIVE_DWELL", 32), // file.h:282
        ("OCE", 33), // file.h:283
        ("NEW_BEACON_TEMPLATE", 34), // file.h:284
        ("NEW_RX_STATS", 35), // file.h:285
        ("WOWLAN_KEY_MATERIAL", 36), // file.h:286
        ("QUOTA_LOW_LATENCY", 38), // file.h:287
        ("DEPRECATE_TTAK", 41), // file.h:288
        ("ADAPTIVE_DWELL_V2", 42), // file.h:289
        ("FRAG_EBS", 44), // file.h:290
        ("REDUCE_TX_POWER", 45), // file.h:291
        ("SHORT_BEACON_NOTIF", 46), // file.h:292
        ("BEACON_FILTER_V4", 47), // file.h:293
        ("REGULATORY_NVM_INFO", 48), // file.h:294
        ("FTM_NEW_RANGE_REQ", 49), // file.h:295
        ("SCAN_OFFLOAD_CHANS", 50), // file.h:296
        ("MBSSID_HE", 52), // file.h:297
        ("WOWLAN_TCP_SYN_WAKE", 53), // file.h:298
        ("FTM_RTT_ACCURACY", 54), // file.h:299
        ("SAR_TABLE_VER", 55), // file.h:300
        ("REDUCED_SCAN_CONFIG", 56), // file.h:301
        ("ADWELL_HB_DEF_N_AP", 57), // file.h:302
        ("SCAN_EXT_CHAN_VER", 58), // file.h:303
        ("BAND_IN_RX_DATA", 59), // file.h:304
        ("NO_HOST_DISABLE_TX", 66), // file.h:306
        ("INT_DBG_BUF_CLEAR", 67), // file.h:307
        ("SMART_FIFO_OFFLOAD", 68), // file.h:308
];

const CAPA_EXPECTED: [(&str, u32); 90] = [
        ("D0I3_SUPPORT", 0), // file.h:456
        ("LAR_SUPPORT", 1), // file.h:457
        ("UMAC_SCAN", 2), // file.h:458
        ("BEAMFORMER", 3), // file.h:459
        ("TDLS_SUPPORT", 6), // file.h:460
        ("TXPOWER_INSERTION_SUPPORT", 8), // file.h:461
        ("DS_PARAM_SET_IE_SUPPORT", 9), // file.h:462
        ("WFA_TPC_REP_IE_SUPPORT", 10), // file.h:463
        ("QUIET_PERIOD_SUPPORT", 11), // file.h:464
        ("DQA_SUPPORT", 12), // file.h:465
        ("TDLS_CHANNEL_SWITCH", 13), // file.h:466
        ("CNSLDTD_D3_D0_IMG", 17), // file.h:467
        ("HOTSPOT_SUPPORT", 18), // file.h:468
        ("CSUM_SUPPORT", 21), // file.h:469
        ("RADIO_BEACON_STATS", 22), // file.h:470
        ("P2P_SCM_UAPSD", 26), // file.h:471
        ("BT_COEX_PLCR", 28), // file.h:472
        ("LAR_MULTI_MCC", 29), // file.h:473
        ("BT_COEX_RRC", 30), // file.h:474
        ("GSCAN_SUPPORT", 31), // file.h:475
        ("FRAGMENTED_PNVM_IMG", 32), // file.h:478
        ("SOC_LATENCY_SUPPORT", 37), // file.h:479
        ("STA_PM_NOTIF", 38), // file.h:480
        ("BINDING_CDB_SUPPORT", 39), // file.h:481
        ("CDB_SUPPORT", 40), // file.h:482
        ("D0I3_END_FIRST", 41), // file.h:483
        ("TLC_OFFLOAD", 43), // file.h:484
        ("DYNAMIC_QUOTA", 44), // file.h:485
        ("COEX_SCHEMA_2", 45), // file.h:486
        ("CHANNEL_SWITCH_CMD", 46), // file.h:487
        ("FTM_CALIBRATED", 47), // file.h:488
        ("ULTRA_HB_CHANNELS", 48), // file.h:489
        ("CS_MODIFY", 49), // file.h:490
        ("SET_LTR_GEN2", 50), // file.h:491
        ("SET_PPAG", 52), // file.h:492
        ("TAS_CFG", 53), // file.h:493
        ("SESSION_PROT_CMD", 54), // file.h:494
        ("PROTECTED_TWT", 56), // file.h:495
        ("FW_RESET_HANDSHAKE", 57), // file.h:496
        ("PASSIVE_6GHZ_SCAN", 58), // file.h:497
        ("HIDDEN_6GHZ_SCAN", 59), // file.h:498
        ("BROADCAST_TWT", 60), // file.h:499
        ("COEX_HIGH_PRIO", 61), // file.h:500
        ("RFIM_SUPPORT", 62), // file.h:501
        ("BAID_ML_SUPPORT", 63), // file.h:502
        ("EXTENDED_DTS_MEASURE", 64), // file.h:505
        ("SHORT_PM_TIMEOUTS", 65), // file.h:506
        ("BT_MPLUT_SUPPORT", 67), // file.h:507
        ("MULTI_QUEUE_RX_SUPPORT", 68), // file.h:508
        ("CSA_AND_TBTT_OFFLOAD", 70), // file.h:509
        ("BEACON_ANT_SELECTION", 71), // file.h:510
        ("BEACON_STORING", 72), // file.h:511
        ("LAR_SUPPORT_V3", 73), // file.h:512
        ("CT_KILL_BY_FW", 74), // file.h:513
        ("TEMP_THS_REPORT_SUPPORT", 75), // file.h:514
        ("CTDP_SUPPORT", 76), // file.h:515
        ("USNIFFER_UNIFIED", 77), // file.h:516
        ("EXTEND_SHARED_MEM_CFG", 80), // file.h:517
        ("LQM_SUPPORT", 81), // file.h:518
        ("TX_POWER_ACK", 84), // file.h:519
        ("D3_DEBUG", 87), // file.h:520
        ("LED_CMD_SUPPORT", 88), // file.h:521
        ("MCC_UPDATE_11AX_SUPPORT", 89), // file.h:522
        ("CSI_REPORTING", 90), // file.h:523
        ("DBG_SUSPEND_RESUME_CMD_SUPP", 92), // file.h:524
        ("DBG_BUF_ALLOC_CMD_SUPP", 93), // file.h:525
        ("BIOS_OVERRIDE_UNII4_US_CA", 96), // file.h:528
        ("PSC_CHAN_SUPPORT", 98), // file.h:529
        ("BIGTK_SUPPORT", 100), // file.h:530
        ("SPP_AMSDU_SUPPORT", 103), // file.h:531
        ("DRAM_FRAG_SUPPORT", 104), // file.h:532
        ("DUMP_COMPLETE_SUPPORT", 105), // file.h:533
        ("SYNCED_TIME", 106), // file.h:534
        ("TIME_SYNC_BOTH_FTM_TM", 108), // file.h:535
        ("BIGTK_TX_SUPPORT", 109), // file.h:536
        ("MLD_API_SUPPORT", 110), // file.h:537
        ("SCAN_DONT_TOGGLE_ANT", 111), // file.h:538
        ("PPAG_CHINA_BIOS_SUPPORT", 112), // file.h:539
        ("OFFLOAD_BTM_SUPPORT", 113), // file.h:540
        ("STA_EXP_MFP_SUPPORT", 114), // file.h:541
        ("SNIFF_VALIDATE_SUPPORT", 116), // file.h:542
        ("CHINA_22_REG_SUPPORT", 117), // file.h:543
        ("SECURE_LTF_SUPPORT", 121), // file.h:544
        ("MONITOR_PASSIVE_CHANS", 122), // file.h:545
        ("BIOS_OVERRIDE_5G9_FOR_CA", 123), // file.h:546
        ("UHB_CANADA_TAS_SUPPORT", 124), // file.h:547
        ("EXT_FSEQ_IMAGE_SUPPORT", 125), // file.h:548
        ("RESET_DURING_ASSERT", 128), // file.h:552
        ("FW_ACCEPTS_RAW_DSM_TABLE", 129), // file.h:553
        ("NAN_SYNC_SUPPORT", 130), // file.h:554
];

#[test]
fn every_api_member_of_file_h_is_ported_with_its_linux_value() {
    assert_eq!(API_BITS.len(), 34); // file.h:271-309 has 34 members
    assert_eq!(API_BITS, API_EXPECTED);
}

#[test]
fn every_capa_member_of_file_h_is_ported_with_its_linux_value() {
    assert_eq!(CAPA_BITS.len(), 90); // file.h:454-554 has 90 members
    assert_eq!(CAPA_BITS, CAPA_EXPECTED);
}

#[test]
fn the_bitmap_sizes_follow_the_last_member_of_each_enum() {
    // file.h:308 SMART_FIFO_OFFLOAD = 68; file.h:554 NAN_SYNC_SUPPORT = 4*32+2.
    assert_eq!(NUM_IWL_UCODE_TLV_API, 69);
    assert_eq!(NUM_IWL_UCODE_TLV_CAPA, 131);
    // iwl-drv.c:548, :569 DIV_ROUND_UP(NUM, 32)
    assert_eq!(API_WORDS, 3);
    assert_eq!(CAPA_WORDS, 5);
}

/// The family test (five-ways #5): EVERY member, driven through the TLV word the firmware would send
/// (index = bit / 32, flags = 1 << bit % 32, written out from the literal), must test set under its
/// own constant and under no other member.
#[test]
fn every_capa_bit_set_by_its_tlv_word_is_seen_only_under_its_own_name() {
    for (name, lit) in CAPA_EXPECTED {
        let mut set = CapaSet::default();
        set.set_word(lit / 32, 1 << (lit % 32)).unwrap();
        let (_, c) = CAPA_BITS.iter().copied().find(|(n, _)| *n == name).unwrap();
        assert!(set.has(c), "{name}");
        for (other, oc) in CAPA_BITS {
            if other != name {
                assert!(!set.has(oc), "{name} also reads as {other}");
            }
        }
        assert_eq!(set.iter().collect::<Vec<_>>(), vec![lit]);
    }
}

#[test]
fn every_api_bit_set_by_its_tlv_word_is_seen_only_under_its_own_name() {
    for (name, lit) in API_EXPECTED {
        let mut set = ApiSet::default();
        set.set_word(lit / 32, 1 << (lit % 32)).unwrap();
        let (_, c) = API_BITS.iter().copied().find(|(n, _)| *n == name).unwrap();
        assert!(set.has(c), "{name}");
        for (other, oc) in API_BITS {
            if other != name {
                assert!(!set.has(oc), "{name} also reads as {other}");
            }
        }
    }
}

#[test]
fn the_zero_valued_d0i3_bit_is_bit_0_of_word_0() {
    // five-ways #2: CAPA_D0I3_SUPPORT = 0 (file.h:456) pinned by behaviour.
    let mut set = CapaSet::default();
    set.set_word(0, 0x1).unwrap();
    assert!(set.has(CAPA_D0I3_SUPPORT));
    assert!(!set.has(CAPA_LAR_SUPPORT));
    let mut other = CapaSet::default();
    other.set_word(0, 0x2).unwrap();
    assert!(!other.has(CAPA_D0I3_SUPPORT));
}

#[test]
fn words_or_in_and_an_index_past_the_bitmap_is_refused_by_name_leaving_it_unchanged() {
    let mut set = CapaSet::default();
    set.set_word(1, 0x0000_0800).unwrap(); // bit 43
    set.set_word(1, 0x0000_1000).unwrap(); // bit 44 ORs in
    assert_eq!(set.0, [0, 0x1800, 0, 0, 0]);
    assert!(set.has(CAPA_TLC_OFFLOAD) && set.has(CAPA_DYNAMIC_QUOTA));
    assert_eq!(set.set_word(5, 0xFFFF_FFFF), Err(IndexPastBitmap { index: 5, words: 5, flags: 0xFFFF_FFFF }));
    assert_eq!(set.0, [0, 0x1800, 0, 0, 0]);
    let mut api = ApiSet::default();
    assert_eq!(api.set_word(3, 1), Err(IndexPastBitmap { index: 3, words: 3, flags: 1 }));
    assert!(!set.has(160), "a bit past the bitmap is never set");
}
