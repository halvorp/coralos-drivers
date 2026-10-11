// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the ALIVE v3 notification.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/fw/api/alive.h:36-83` and
//! `drivers/net/wireless/intel/iwlwifi/mvm/fw.c:100-238`. Copyright (C) 2012-2014, 2018-2025 Intel
//! Corporation, and the Linux iwlwifi authors.

use iwlwifi_fw_alive_core::alive::*;
use iwlwifi_fw_alive_core::packet::parse_rx_packet;

/// A 68-byte v3 payload with every field distinct, at the byte offsets alive.h:41-83 gives.
fn payload(status: u16, umac_err: u32) -> Vec<u8> {
    let mut p = vec![];
    p.extend_from_slice(&status.to_le_bytes()); // :79
    p.extend_from_slice(&1u16.to_le_bytes()); // flags :80 (RFKILL bit)
    p.extend_from_slice(&46u32.to_le_bytes()); // ucode_major :53
    p.extend_from_slice(&0x7e3e_4b69u32.to_le_bytes()); // ucode_minor :54
    p.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // subtype, type, mac, opt :55-58
    p.extend_from_slice(&0x5555_6666u32.to_le_bytes()); // timestamp :59
    for w in [0x0081_3AF4u32, 0x0080_FB94, 0x0081_43BC, 0x0080_7C60, 0x0080_D040, 0x0080_0000, 0x0040_0000, 0x0000_1000] {
        p.extend_from_slice(&w.to_le_bytes()); // dbg_ptrs :42-49
    }
    p.extend_from_slice(&0x30u32.to_le_bytes()); // umac_major :69
    p.extend_from_slice(&0x31u32.to_le_bytes()); // umac_minor :70
    p.extend_from_slice(&umac_err.to_le_bytes()); // error_info_addr :64
    p.extend_from_slice(&0xC044_47B8u32.to_le_bytes()); // dbg_print_buff_addr :65
    p
}

fn packet(group: u8, cmd: u8, payload: &[u8]) -> Vec<u8> {
    let mut b = ((4 + payload.len()) as u32).to_le_bytes().to_vec();
    b.extend_from_slice(&[cmd, group, 0x00, 0x80]);
    b.extend_from_slice(payload);
    b
}

#[test]
fn constants_and_sizes_are_alive_h() {
    assert_eq!(LEGACY_GROUP, 0x0); // commands.h:39
    assert_eq!(UCODE_ALIVE_NTFY, 0x1); // commands.h:65
    assert_eq!(IWL_ALIVE_STATUS_ERR, 0xDEAD); // alive.h:36
    assert_eq!(IWL_ALIVE_STATUS_OK, 0xCAFE); // alive.h:37
    assert_eq!(IWL_ALIVE_FLG_RFKILL, 0x1); // alive.h:39
    assert_eq!(FW_ADDR_CACHE_CONTROL, 0xC000_0000); // img.h:121
    assert_eq!(LMAC_DEBUG_ADDRS_LEN, 32);
    assert_eq!(LMAC_ALIVE_LEN, 48);
    assert_eq!(UMAC_ALIVE_LEN, 16);
    assert_eq!(ALIVE_NTF_V3_LEN, 68);
    assert_eq!(ALIVE_V7_MIN_NOTIF_VER, 6); // mvm/fw.c:117
}

#[test]
fn every_v3_field_lands_where_alive_h_puts_it() {
    let raw = packet(0, 1, &payload(0xCAFE, 0x8088_00C4));
    let a = parse_alive_v3(3, &parse_rx_packet(&raw).unwrap()).unwrap();
    assert_eq!((a.status, a.flags), (0xCAFE, 1));
    assert!(a.is_ok());
    assert_eq!((a.lmac.ucode_major, a.lmac.ucode_minor), (46, 0x7e3e_4b69));
    assert_eq!((a.lmac.ver_subtype, a.lmac.ver_type, a.lmac.mac, a.lmac.opt), (0x11, 0x22, 0x33, 0x44));
    assert_eq!(a.lmac.timestamp, 0x5555_6666);
    assert_eq!(
        a.lmac.dbg_ptrs,
        LmacDebugAddrs {
            error_event_table_ptr: 0x0081_3AF4,
            log_event_table_ptr: 0x0080_FB94,
            cpu_register_ptr: 0x0081_43BC,
            dbgm_config_ptr: 0x0080_7C60,
            alive_counter_ptr: 0x0080_D040,
            scd_base_ptr: 0x0080_0000,
            st_fwrd_addr: 0x0040_0000,
            st_fwrd_size: 0x0000_1000,
        }
    );
    assert_eq!(a.umac, UmacAlive { umac_major: 0x30, umac_minor: 0x31, error_info_addr: 0x8088_00C4, dbg_print_buff_addr: 0xC044_47B8 });
    // mvm/fw.c:205-218: LMAC pointer as-is, UMAC with the cache-control bits stripped.
    assert_eq!(a.error_tables(), ErrorTablePtrs { lmac: 0x0081_3AF4, umac: Some(0x0088_00C4) });
}

#[test]
fn a_zero_umac_pointer_is_not_recorded_and_a_dead_status_is_not_ok() {
    let raw = packet(0, 1, &payload(0xDEAD, 0xC000_0000)); // only cache bits: zero after the strip
    let a = parse_alive_v3(3, &parse_rx_packet(&raw).unwrap()).unwrap();
    assert!(!a.is_ok()); // mvm/fw.c:221
    assert_eq!(a.error_tables().umac, None); // mvm/fw.c:216
}

#[test]
fn other_packets_versions_and_sizes_are_refused_by_name() {
    let p = payload(0xCAFE, 0);
    let not = packet(0, 2, &p);
    assert_eq!(parse_alive_v3(3, &parse_rx_packet(&not).unwrap()), Err(AliveError::NotAlive { group_id: 0, cmd: 2 }));
    let grp = packet(1, 1, &p);
    assert_eq!(parse_alive_v3(3, &parse_rx_packet(&grp).unwrap()), Err(AliveError::NotAlive { group_id: 1, cmd: 1 }));
    let raw = packet(0, 1, &p);
    let pkt = parse_rx_packet(&raw).unwrap();
    assert_eq!(parse_alive_v3(6, &pkt), Err(AliveError::LayoutNotPorted { notif_ver: 6 }));
    assert!(parse_alive_v3(5, &pkt).is_ok());
    for len in [67usize, 69, 144] {
        let mut q = p.clone();
        q.resize(len, 0);
        let raw = packet(0, 1, &q);
        assert_eq!(parse_alive_v3(3, &parse_rx_packet(&raw).unwrap()), Err(AliveError::UnsupportedSize { len, need: 68 }));
    }
}
