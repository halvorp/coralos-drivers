// SPDX-License-Identifier: GPL-2.0-only
//! The real Wireless-AC 9260 firmware, when pointed at: `CORAL_IWL_UCODE=<path>` to the DECOMPRESSED
//! `iwlwifi-9260-th-b0-jf-b0-46.ucode` (linux-firmware; e.g. `xz -dc
//! /lib/firmware/intel/iwlwifi/iwlwifi-9260-th-b0-jf-b0-46.ucode.xz`; 1 490 508 bytes, sha256
//! 05f4c12514fddfd56fea660b3d2eb6b6e6f4a87a324d5d40464050312ddca911 — the same bytes ChromeOS ships
//! for the Pixelbook Go). The blob is Intel's, redistributable only unmodified under its own licence,
//! so it is NOT committed here; unset, the test says SKIPPED by name.
//!
//! Exercises the port of Linux `drivers/net/wireless/intel/iwlwifi/iwl-drv.c` (iwl_parse_tlv_firmware).
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation, and the Linux iwlwifi authors.

use iwlwifi_fw_tlv_core::capa::*;
use iwlwifi_fw_tlv_core::container::*;
use iwlwifi_fw_tlv_core::params::*;

#[test]
fn the_9260_46_blob_when_pointed_at() {
    let Ok(path) = std::env::var("CORAL_IWL_UCODE") else {
        eprintln!("the_9260_46_blob_when_pointed_at: SKIPPED (set CORAL_IWL_UCODE=<decompressed -46 blob>)");
        return;
    };
    let b = std::fs::read(&path).expect("read CORAL_IWL_UCODE");
    assert_eq!(b.len(), 1_490_508, "not the -46 blob");
    let h = parse_header(&b).unwrap();
    assert!(h.human_readable.starts_with(b"release/core43::7e3e4b69"));
    assert!(tlvs(&b).unwrap().all(|t| t.is_ok()), "every TLV fits, padding included");

    let i = parse_fw_tlvs(&b).unwrap();
    // Measured 2026-10-11 (a TLV walk outside this crate agreed word for word).
    assert_eq!(i.capa.0, [0x9def_137f, 0x0003_f262, 0x1991_7edb, 0x0020_0002, 0]);
    assert_eq!(i.api.0, [0xdd9b_fffb, 0x00c2_6fdd, 0]);
    assert_eq!(i.dropped, None);
    let present = [
        CAPA_LAR_SUPPORT, CAPA_UMAC_SCAN, CAPA_DQA_SUPPORT, CAPA_SOC_LATENCY_SUPPORT, CAPA_DYNAMIC_QUOTA,
        CAPA_COEX_SCHEMA_2, CAPA_MULTI_QUEUE_RX_SUPPORT, CAPA_LAR_SUPPORT_V3, CAPA_CT_KILL_BY_FW,
        CAPA_TEMP_THS_REPORT_SUPPORT, CAPA_CTDP_SUPPORT, CAPA_EXTEND_SHARED_MEM_CFG, CAPA_TX_POWER_ACK,
    ];
    for c in present {
        assert!(i.has_capa(c), "capa {c} expected on -46");
    }
    // The bits the 9260 flow branches on that are ABSENT: rate scaling is the host's (no
    // TLC_OFFLOAD), session protection is the TIME_EVENT path, LTR is the gen1 command, no CDB.
    for c in [CAPA_TLC_OFFLOAD, CAPA_SESSION_PROT_CMD, CAPA_SET_LTR_GEN2, CAPA_CDB_SUPPORT, CAPA_LAR_MULTI_MCC] {
        assert!(!i.has_capa(c), "capa {c} must be absent on -46");
    }
    for a in [API_WIFI_MCC_UPDATE, API_NEW_VERSION, API_ADAPTIVE_DWELL, API_ADAPTIVE_DWELL_V2, API_NEW_RX_STATS, API_REDUCE_TX_POWER, API_SAR_TABLE_VER] {
        assert!(i.has_api(a), "api {a} expected on -46");
    }
    assert_eq!((i.n_scan_channels, i.max_probe_length, i.flags), (52, 0x200, 0xfbfb_4b7f));
    assert_eq!(i.phy_config, Some(0x0033_0018));
    assert_eq!(phy_valid_ant(0x0033_0018), (0x3, 0x3));
    assert_eq!(phy_radio_cfg(0x0033_0018), (0, 2, 1));
    assert!(i.is_dual_cpus());
    assert_eq!(i.paging_mem_size, 0x3b000);
    assert_eq!(i.fw_version, Some((46, 0x7e3e_4b69, 0)));
    // The DEBUG_ADDRS TLVs are present, and on a 9000-family part they name nothing.
    assert_eq!(i.umac_debug_addrs, Some((0x0088_00c4, 0xc044_47b8)));
    assert!(i.lmac_debug_addrs.is_some());
    assert_eq!(i.tlv_error_tables(false), TlvErrorTables::default());
    assert_eq!((i.init_errlog_ptr, i.inst_errlog_ptr), (0, 0)); // no legacy DVM pointers in an mvm blob
}
