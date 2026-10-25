// SPDX-License-Identifier: GPL-2.0-only
//! SCAN_CFG_CMD v1 and the passive SCAN_REQ_UMAC v8, byte for byte. Expected values are written out from fw/api/scan.h.
use iwlwifi_mvm_rt_core::encode::EncodeError;
use iwlwifi_mvm_rt_core::ids::*;
use iwlwifi_mvm_rt_core::scanreq::*;

fn le32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}

#[test]
fn sizes_are_the_struct_sizes() {
    // iwl_scan_req_umac v8 = 52 (fw/api/scan.h:940); channel cfg = 8; tail v1 = 12 + 528 + 20 * 34 = 1220 (:788-796)
    assert_eq!(IWL_SCAN_REQ_UMAC_SIZE_V8, 52);
    assert_eq!(SCAN_CHANNEL_CFG_UMAC_LEN, 8);
    assert_eq!(SCAN_REQ_UMAC_TAIL_V1_LEN, 1220);
    assert_eq!(scan_req_umac_len(52), 52 + 8 * 52 + 1220);
    assert_eq!(scan_req_umac_len(52), 1688);
    assert_eq!(SCAN_CONFIG_V1_HDR_LEN, 36);
}

#[test]
fn scan_config_v1_bytes() {
    let ch = [1u8, 6, 11];
    let mut b = [0xaau8; 128];
    let c = ScanConfig { tx_chains: 3, rx_chains: 3, mac_addr: [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc], bcast_sta_id: 1, channels: &ch, n_scan_channels: 52 };
    assert_eq!(encode_scan_config_v1(&c, &mut b), Ok(36 + 52));
    // flags: ACTIVATE 1 | ALLOW_CHUB 8 | TX 0x100 | RX 0x200 | AUX 0x400 | ALL_TIMES 0x800 | CHANNEL_FLAGS 0x2000 |
    // LEGACY_RATES 0x4000 | MAC_ADDR 0x8000 | CLEAR_FRAGMENTED 0x20000 | 3 channels << 26   (fw/api/scan.h:452-474)
    assert_eq!(le32(&b, 0), 0x0c02_ef09);
    assert_eq!(le32(&b, 4), 3);
    assert_eq!(le32(&b, 8), 3);
    assert_eq!(le32(&b, 12), 0x0fff_0fff); // 12 basic rates, same bits << 16 (:479-494)
    assert_eq!(le32(&b, 16), 0); // out_of_channel_time (UNASSOC)
    assert_eq!(le32(&b, 20), 0); // suspend_time
    assert_eq!(&b[24..28], &[10, 110, 44, 90]); // dwell active passive fragmented extended
    assert_eq!(&b[28..34], &[0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc]);
    assert_eq!(b[34], 1); // bcast_sta_id
    assert_eq!(b[35], 0x0f); // EBS | ACCURATE_EBS | EBS_ADD | PRE_SCAN_PASSIVE2ACTIVE
    assert_eq!(&b[36..39], &[1, 6, 11]);
    assert!(b[39..88].iter().all(|&x| x == 0), "the rest of channel_array is zero");
    assert_eq!(b[88], 0xaa);
}

#[test]
fn scan_config_refuses_more_channels_than_the_firmware_holds() {
    let ch = [1u8; 53];
    let c = ScanConfig { tx_chains: 3, rx_chains: 3, mac_addr: [0; 6], bcast_sta_id: 1, channels: &ch, n_scan_channels: 52 };
    assert_eq!(encode_scan_config_v1(&c, &mut [0u8; 200]), Err(EncodeError::TooManyChannels { n: 53, max: 52 }));
}

#[test]
fn scan_req_umac_v8_passive_head_channels_and_tail() {
    let ch = [1u8, 6, 36];
    let mut b = vec![0xaau8; 1700];
    let r = ScanRequest { uid: 0, channels: &ch, n_scan_channels: 52, sa: [2, 0, 0, 0, 0, 1], scan_start_mac_id: 0, use_ebs: true };
    assert_eq!(encode_scan_req_umac_v8(&r, &mut b), Ok(1688));
    assert_eq!(b[1688], 0xaa, "nothing past the command is touched");
    // head (fw/api/scan.h iwl_scan_req_umac): flags@0 uid@4 ooc_priority@8 general_flags@12 reserved@14 scan_start_mac_id@15
    assert_eq!(le32(&b, 0), 0);
    assert_eq!(le32(&b, 4), 0);
    assert_eq!(le32(&b, 8), 6); // IWL_SCAN_PRIORITY_EXT_6
    // PASSIVE BIT(3) | PASS_ALL BIT(2) | ADAPTIVE_DWELL BIT(13) = 0x200c
    assert_eq!(u16::from_le_bytes([b[12], b[13]]), 0x200c);
    assert_eq!(b[15], 0);
    // v8 @16: active_dwell[2]@16 reserved2@18 adwell_default_n_aps@19 social@20 general_flags2@21 adwell_max_budget@22
    // max_out_time[2]@24 suspend_time[2]@32 scan_priority@40 passive_dwell[2]@44 num_of_fragments[2]@46 channel@48
    assert_eq!(&b[16..18], &[10, 0]);
    assert_eq!(b[19], 2);
    assert_eq!(b[20], 10);
    assert_eq!(b[21], 2); // ALLOW_CHNL_REORDER BIT(1)
    assert_eq!(u16::from_le_bytes([b[22], b[23]]), 300);
    assert!(b[24..40].iter().all(|&x| x == 0), "UNASSOC max_out_time and suspend_time are 0");
    assert_eq!(le32(&b, 40), 6);
    assert_eq!(&b[44..46], &[110, 0]);
    assert_eq!(&b[46..48], &[0, 0]);
    assert_eq!(b[48], 0x07); // channel.flags = EBS | EBS_ACCURATE | CACHE_ADD
    assert_eq!(b[49], 3); // channel.count
    assert_eq!(&b[50..52], &[0, 0]);
    // channel config array @52, 8 bytes each: flags(le32) channel_num iter_count iter_interval(le16)
    assert_eq!(&b[52..60], &[0, 0, 0, 0, 1, 1, 0, 0]);
    assert_eq!(&b[60..68], &[0, 0, 0, 0, 6, 1, 0, 0]);
    assert_eq!(&b[68..76], &[0, 0, 0, 0, 36, 1, 0, 0]);
    assert!(b[76..52 + 8 * 52].iter().all(|&x| x == 0));
    // tail v1 @468: schedule[0] {interval 0, iter_count 1, reserved}, schedule[1], delay, reserved
    let t = 52 + 8 * 52;
    assert_eq!(&b[t..t + 12], &[0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    // preq (iwl_scan_probe_req_v1) @t+12: mac_header{0,26} band_data[0]{26,3} band_data[1]{29,0} common{29,9} then buf[512]
    let p = t + 12;
    assert_eq!(&b[p..p + 16], &[0, 0, 26, 0, 26, 0, 3, 0, 29, 0, 0, 0, 29, 0, 9, 0]);
    let f = &b[p + 16..p + 16 + 38];
    assert_eq!(&f[0..2], &[0x40, 0x00], "IEEE80211_STYPE_PROBE_REQ");
    assert_eq!(&f[4..10], &[0xff; 6]);
    assert_eq!(&f[10..16], &[2, 0, 0, 0, 0, 1]);
    assert_eq!(&f[16..22], &[0xff; 6]);
    assert_eq!(&f[24..26], &[0, 0], "wildcard SSID element");
    assert_eq!(&f[26..29], &[3, 1, 0], "DS Parameter Set placeholder");
    assert_eq!(&f[29..38], &[221, 7, 0x00, 0x50, 0xf2, 8, 0, 0, 0], "WFA TPC report element");
    // direct_scan[20] after the preq: all zero (no SSIDs)
    let d = p + 16 + 512;
    assert!(b[d..d + 20 * 34].iter().all(|&x| x == 0));
    assert_eq!(d + 20 * 34, 1688);
}

#[test]
fn scan_req_without_ebs_leaves_the_channel_flags_clear_and_a_uid_is_carried() {
    let mut b = vec![0u8; 1688];
    let r = ScanRequest { uid: 3, channels: &[11], n_scan_channels: 52, sa: [0; 6], scan_start_mac_id: 0, use_ebs: false };
    encode_scan_req_umac_v8(&r, &mut b).unwrap();
    assert_eq!(le32(&b, 4), 3);
    assert_eq!(b[48], 0);
    assert_eq!(b[49], 1);
}

#[test]
fn scan_req_refuses_a_channel_list_over_the_firmware_capacity_and_a_short_buffer() {
    let many = [1u8; 53];
    let r = ScanRequest { uid: 0, channels: &many, n_scan_channels: 52, sa: [0; 6], scan_start_mac_id: 0, use_ebs: true };
    assert_eq!(encode_scan_req_umac_v8(&r, &mut vec![0u8; 2000]), Err(EncodeError::TooManyChannels { n: 53, max: 52 }));
    let r = ScanRequest { channels: &[1], ..r };
    assert_eq!(encode_scan_req_umac_v8(&r, &mut vec![0u8; 1687]), Err(EncodeError::BufferTooSmall { need: 1688, have: 1687 }));
}

#[test]
fn scan_flag_constants_are_pinned_by_value() {
    // fw/api/scan.h:626-656, :269-271, :452-474
    assert_eq!((IWL_UMAC_SCAN_GEN_FLAGS_PASS_ALL, IWL_UMAC_SCAN_GEN_FLAGS_PASSIVE, IWL_UMAC_SCAN_GEN_FLAGS_ADAPTIVE_DWELL), (4, 8, 0x2000));
    assert_eq!(IWL_UMAC_SCAN_GEN_FLAGS2_ALLOW_CHNL_REORDER, 2);
    assert_eq!((IWL_SCAN_CHANNEL_FLAG_EBS, IWL_SCAN_CHANNEL_FLAG_EBS_ACCURATE, IWL_SCAN_CHANNEL_FLAG_CACHE_ADD), (1, 2, 4));
    assert_eq!((IWL_CHANNEL_FLAG_EBS, IWL_CHANNEL_FLAG_ACCURATE_EBS, IWL_CHANNEL_FLAG_EBS_ADD, IWL_CHANNEL_FLAG_PRE_SCAN_PASSIVE2ACTIVE), (1, 2, 4, 8));
    assert_eq!((IWL_SCAN_OFFLOAD_COMPLETED, IWL_SCAN_OFFLOAD_ABORTED), (1, 2));
    assert_eq!(IWL_UCODE_TLV_FLAGS_EBS_SUPPORT, 1 << 25);
}
