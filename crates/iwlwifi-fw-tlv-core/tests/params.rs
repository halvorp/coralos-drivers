// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the scalar / capability TLV arms of `iwl_parse_tlv_firmware`.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-drv.c`, `fw/file.h`, `fw/img.h` and
//! `fw/api/alive.h`. Copyright (C) 2005-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi
//! authors.

mod common;
use common::*;
use iwlwifi_fw_tlv_core::capa::*;
use iwlwifi_fw_tlv_core::params::*;

#[test]
fn tlv_type_numbers_are_file_h_47_93() {
    // Each by value against file.h (five-ways #6: pinned independently of the vectors that use them).
    assert_eq!(IWL_UCODE_TLV_PROBE_MAX_LEN, 6); // file.h:54
    assert_eq!(IWL_UCODE_TLV_RUNT_EVTLOG_PTR, 8); // file.h:57
    assert_eq!(IWL_UCODE_TLV_RUNT_EVTLOG_SIZE, 9); // file.h:58
    assert_eq!(IWL_UCODE_TLV_RUNT_ERRLOG_PTR, 10); // file.h:59
    assert_eq!(IWL_UCODE_TLV_INIT_EVTLOG_PTR, 11); // file.h:60
    assert_eq!(IWL_UCODE_TLV_INIT_EVTLOG_SIZE, 12); // file.h:61
    assert_eq!(IWL_UCODE_TLV_INIT_ERRLOG_PTR, 13); // file.h:62
    assert_eq!(IWL_UCODE_TLV_FLAGS, 18); // file.h:67
    assert_eq!(IWL_UCODE_TLV_PHY_SKU, 23); // file.h:72
    assert_eq!(IWL_UCODE_TLV_NUM_OF_CPU, 27); // file.h:76
    assert_eq!(IWL_UCODE_TLV_API_CHANGES_SET, 29); // file.h:78
    assert_eq!(IWL_UCODE_TLV_ENABLED_CAPABILITIES, 30); // file.h:79
    assert_eq!(IWL_UCODE_TLV_N_SCAN_CHANNELS, 31); // file.h:80
    assert_eq!(IWL_UCODE_TLV_PAGING, 32); // file.h:81
    assert_eq!(IWL_UCODE_TLV_FW_VERSION, 36); // file.h:84
    assert_eq!(IWL_UCODE_TLV_UMAC_DEBUG_ADDRS, 54); // file.h:92
    assert_eq!(IWL_UCODE_TLV_LMAC_DEBUG_ADDRS, 55); // file.h:93
    assert_eq!(IWL_DEFAULT_MAX_PROBE_LENGTH, 200); // file.h:574
    assert_eq!(IWL_DEFAULT_SCAN_CHANNELS, 40); // iwl-drv.c:97
    assert_eq!(FW_PAGING_SIZE, 4096); // img.h:94
    assert_eq!(PAGING_BLOCK_SIZE, 32 * 1024); // img.h:99
    assert_eq!(MAX_PAGING_IMAGE_SIZE, 1024 * 1024); // img.h:110
    assert_eq!(FW_ADDR_CACHE_CONTROL, 0xC000_0000); // img.h:121
    assert_eq!(LMAC_DEBUG_ADDRS_LEN, 32); // alive.h:41-50
    assert_eq!(UMAC_DEBUG_ADDRS_LEN, 8); // alive.h:63-66
}

#[test]
fn every_scalar_tlv_lands_in_its_field_by_type_number() {
    // The TLV types are written as LITERALS here, so a wrong constant routes the word elsewhere.
    let f = file(&[
        (6, words(&[0x200])),
        (8, words(&[0x0080_2000])),
        (9, words(&[64])),
        (10, words(&[0x0080_1000])),
        (11, words(&[0x0080_3000])),
        (12, words(&[32])),
        (13, words(&[0x0080_0000])),
        (18, words(&[0xfbfb_4b7f, 0xDEAD_BEEF])),
        (23, words(&[0x0033_0018])),
        (27, words(&[2])),
        (31, words(&[52])),
        (32, words(&[0x3b000])),
        (36, words(&[46, 0x7e3e_4b69, 0x1_0000_0002u64 as u32])),
    ]);
    let i = parse_fw_tlvs(&f).unwrap();
    assert_eq!(i.max_probe_length, 0x200);
    assert_eq!((i.inst_evtlog_ptr, i.inst_evtlog_size, i.inst_errlog_ptr), (0x0080_2000, 64, 0x0080_1000));
    assert_eq!((i.init_evtlog_ptr, i.init_evtlog_size, i.init_errlog_ptr), (0x0080_3000, 32, 0x0080_0000));
    assert_eq!(i.flags, 0xfbfb_4b7f); // iwl-drv.c:922: only the first word
    assert_eq!(i.phy_config, Some(0x0033_0018));
    assert!(i.is_dual_cpus());
    assert_eq!(i.n_scan_channels, 52);
    assert_eq!(i.paging_mem_size, 0x3b000);
    assert_eq!(i.fw_version, Some((46, 0x7e3e_4b69, 2))); // local_comp is a u8 (iwl-drv.c:1073)
}

#[test]
fn absent_tlvs_keep_linux_defaults() {
    let i = parse_fw_tlvs(&file(&[])).unwrap();
    assert_eq!((i.max_probe_length, i.n_scan_channels, i.num_of_cpu), (200, 40, 1));
    assert!(!i.is_dual_cpus());
    assert_eq!((i.phy_config, i.fw_version, i.paging_mem_size), (None, None, 0));
}

#[test]
fn api_and_capa_tlvs_fill_their_bitmaps_and_repeat_words_or_in() {
    let f = file(&[
        (30, words(&[1, 1 << 12])), // capa 44 DYNAMIC_QUOTA
        (30, words(&[1, 1 << 5])),  // capa 37 SOC_LATENCY_SUPPORT, ORed
        (30, words(&[0, 1 << 2])),  // capa 2 UMAC_SCAN
        (29, words(&[1, 1 << 10])), // api 42 ADAPTIVE_DWELL_V2
        (30, words(&[9, 1])),       // dropped, named
    ]);
    let i = parse_fw_tlvs(&f).unwrap();
    assert!(i.has_capa(CAPA_DYNAMIC_QUOTA) && i.has_capa(CAPA_SOC_LATENCY_SUPPORT) && i.has_capa(CAPA_UMAC_SCAN));
    assert!(!i.has_capa(CAPA_TLC_OFFLOAD));
    assert!(i.has_api(API_ADAPTIVE_DWELL_V2) && !i.has_capa(42));
    assert_eq!(i.dropped, Some(IndexPastBitmap { index: 9, words: 5, flags: 1 }));
}

#[test]
fn phy_sku_fields_are_file_h_602_618() {
    assert_eq!(FW_PHY_CFG_RADIO_TYPE, 0x3); // file.h:604
    assert_eq!(FW_PHY_CFG_RADIO_STEP, 0xC); // file.h:606
    assert_eq!(FW_PHY_CFG_RADIO_DASH, 0x30); // file.h:608
    assert_eq!(FW_PHY_CFG_TX_CHAIN, 0xF_0000); // file.h:610
    assert_eq!(FW_PHY_CFG_RX_CHAIN, 0xF0_0000); // file.h:612
    assert_eq!((FW_PHY_CFG_RADIO_STEP_POS, FW_PHY_CFG_RADIO_DASH_POS), (2, 4)); // file.h:605,607
    assert_eq!((FW_PHY_CFG_TX_CHAIN_POS, FW_PHY_CFG_RX_CHAIN_POS), (16, 20)); // file.h:609,611
    // A word with every field distinct: type 1, step 2, dash 3, tx 0x5, rx 0xA.
    let w = 1 | (2 << 2) | (3 << 4) | (0x5 << 16) | (0xA << 20);
    assert_eq!(phy_radio_cfg(w), (1, 2, 3));
    assert_eq!(phy_valid_ant(w), (0x5, 0xA));
}

#[test]
fn each_bad_length_num_of_cpu_and_paging_value_refuses_the_file_by_name() {
    for (ty, data) in [
        (6u32, vec![1u8, 2]),
        (18, vec![]),
        (18, vec![1, 2, 3, 4, 5]),
        (23, vec![0; 8]),
        (27, vec![2]),
        (29, words(&[0])),
        (30, words(&[0, 0, 0])),
        (31, vec![0; 2]),
        (32, vec![0; 8]),
        (36, words(&[1, 2])),
        (54, words(&[1, 2, 3])),
        (55, words(&[1; 7])),
    ] {
        let len = data.len();
        assert_eq!(parse_fw_tlvs(&file(&[(ty, data)])), Err(ParamsError::InvalidTlvLen { ty, len }), "type {ty}");
    }
    assert_eq!(parse_fw_tlvs(&file(&[(27, words(&[3]))])), Err(ParamsError::NumOfCpu { n: 3 }));
    assert_eq!(parse_fw_tlvs(&file(&[(27, words(&[0]))])), Err(ParamsError::NumOfCpu { n: 0 }));
    assert_eq!(
        parse_fw_tlvs(&file(&[(32, words(&[0x10_1000]))])),
        Err(ParamsError::PagingTooLarge { size: 0x10_1000, max: 0x10_0000 })
    );
    assert_eq!(
        parse_fw_tlvs(&file(&[(32, words(&[0x3b800]))])),
        Err(ParamsError::PagingNotPageMultiple { size: 0x3b800, page: 0x1000 })
    );
    assert!(parse_fw_tlvs(&file(&[(32, words(&[0x10_0000]))])).is_ok()); // exactly the max
}

#[test]
fn debug_addr_tlvs_name_no_error_table_below_family_22000() {
    let lmac = [0xC081_3AF4, 2, 3, 4, 5, 6, 7, 8];
    let f = file(&[(55, words(&lmac)), (54, words(&[0x8088_00C4, 0xC044_47B8]))]);
    let i = parse_fw_tlvs(&f).unwrap();
    assert_eq!(i.lmac_debug_addrs, Some(lmac));
    assert_eq!(i.umac_debug_addrs, Some((0x8088_00C4, 0xC044_47B8)));
    assert_eq!(i.tlv_error_tables(false), TlvErrorTables::default()); // iwl-drv.c:1338-1340
    assert_eq!(
        i.tlv_error_tables(true),
        TlvErrorTables { lmac: Some(0x0081_3AF4), umac: Some(0x0088_00C4) } // :1341-1343, :1357-1359
    );
}
