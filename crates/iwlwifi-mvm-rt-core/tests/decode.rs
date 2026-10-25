// SPDX-License-Identifier: GPL-2.0-only
//! Decoder vectors written byte by byte from fw/api/scan.h, fw/api/nvm-reg.h, fw/api/rx.h, fw/api/sta.h.
use iwlwifi_mvm_rt_core::decode::*;

#[test]
fn add_sta_status() {
    assert_eq!(decode_add_sta_status(&[1, 0, 0, 0]), Ok(1));
    assert!(add_sta_ok(1));
    assert!(add_sta_ok(0x0000_0101), "only the low byte is the status (IWL_ADD_STA_STATUS_MASK 0xFF)");
    assert!(!add_sta_ok(2)); // ADD_STA_STATIONS_OVERLOAD
    assert!(!add_sta_ok(8)); // ADD_STA_MODIFY_NON_EXISTING_STA
    assert_eq!(decode_add_sta_status(&[1, 0]), Err(DecodeError::Short { have: 2, need: 4 }));
}

#[test]
fn mcc_resp_v3_is_length_checked_exactly_and_maps_zero_to_the_world_domain() {
    // iwl_mcc_update_resp_v3 { le32 status; le16 mcc; u8 cap; u8 source_id; le16 time; le16 geo_info; le32 n_channels; le32 channels[] }
    let mut p = vec![0u8; 16 + 3 * 4];
    p[0] = 1; // status
    p[4] = 0x53; // mcc 'U' 'S' = 0x5553
    p[5] = 0x55;
    p[6] = 2; // cap
    p[7] = 0x10; // source_id
    p[8] = 7;
    p[10] = 1;
    p[12] = 3; // n_channels
    p[16..20].copy_from_slice(&0x0000_0071u32.to_le_bytes());
    p[20..24].copy_from_slice(&0xffff_0000u32.to_le_bytes());
    p[24..28].copy_from_slice(&0x0000_0001u32.to_le_bytes());
    let r = decode_mcc_resp_v3(&p).unwrap();
    assert_eq!((r.status, r.mcc, r.cap, r.source_id, r.time, r.geo_info, r.n_channels), (1, 0x5553, 2, 0x10, 7, 1, 3));
    assert_eq!(r.channel_flags(0), Some(0x71));
    assert_eq!(r.channel_flags(1), Some(0), "only the low 16 bits of each dword are flags");
    assert_eq!(r.channel_flags(2), Some(1));
    assert_eq!(r.channel_flags(3), None);
    // one byte too long / short is refused
    let mut long = p.clone();
    long.push(0);
    assert_eq!(decode_mcc_resp_v3(&long), Err(DecodeError::McCLengthMismatch { have: 29, want: 28 }));
    assert_eq!(decode_mcc_resp_v3(&p[..27]), Err(DecodeError::McCLengthMismatch { have: 27, want: 28 }));
    assert_eq!(decode_mcc_resp_v3(&p[..15]), Err(DecodeError::Short { have: 15, need: 16 }));
    // mcc 0 -> "00" (0x3030) (mvm/nvm.c W/A)
    let mut z = vec![0u8; 16];
    z[4] = 0;
    z[5] = 0;
    assert_eq!(decode_mcc_resp_v3(&z).unwrap().mcc, 0x3030);
    // a lying n_channels that would overflow the length arithmetic is a mismatch, not a panic
    let mut lie = vec![0u8; 16];
    lie[12..16].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    assert!(matches!(decode_mcc_resp_v3(&lie), Err(DecodeError::McCLengthMismatch { .. })));
}

#[test]
fn scan_complete_and_iteration_complete() {
    // iwl_umac_scan_complete { le32 uid; u8 last_schedule; u8 last_iter; u8 status; u8 ebs_status; le32 time_from_last_iter; le32 reserved }
    let p = [3, 0, 0, 0, 0, 1, 1, 0, 0x10, 0x27, 0, 0, 0, 0, 0, 0];
    let c = decode_scan_complete(&p).unwrap();
    assert_eq!((c.uid, c.last_schedule, c.last_iter, c.status, c.ebs_status, c.time_from_last_iter), (3, 0, 1, 1, 0, 10_000));
    assert_eq!(decode_scan_complete(&p[..15]), Err(DecodeError::Short { have: 15, need: 16 }));
    // iwl_umac_scan_iter_complete_notif { le32 uid; u8 scanned_channels; u8 status; u8 bt_status; u8 last_channel; le64 start_tsf; results[] }
    let q = [3, 0, 0, 0, 3, 1, 0, 36, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11];
    let i = decode_scan_iter_complete(&q).unwrap();
    assert_eq!((i.uid, i.scanned_channels, i.status, i.bt_status, i.last_channel, i.start_tsf), (3, 3, 1, 0, 36, 0x1122_3344_5566_7788));
    assert_eq!(decode_scan_iter_complete(&q[..8]), Err(DecodeError::Short { have: 8, need: 16 }));
}

fn rx_payload(mpdu: &[u8], flags1: u8, flags2: u8, status: u32, ea: u8, eb: u8, ch: u8) -> Vec<u8> {
    // iwl_rx_mpdu_desc (v1 tail), fw/api/rx.h: mpdu_len@0 mac_flags1@2 mac_flags2@3 amsdu_info@4 phy_info@5 mac_phy_band@7
    // raw_csum@8 l3l4_flags@10 status@12 reorder_data@16 | v1: rss_hash@20 filter_match@24 rate_n_flags@28
    // energy_a@32 energy_b@33 channel@34 mac_context@35 gp2_on_air_rise@36 tsf_on_air_rise@40
    let mut p = vec![0u8; 48];
    p[0..2].copy_from_slice(&(mpdu.len() as u16).to_le_bytes());
    p[2] = flags1;
    p[3] = flags2;
    p[12..16].copy_from_slice(&status.to_le_bytes());
    p[32] = ea;
    p[33] = eb;
    p[34] = ch;
    p.extend_from_slice(mpdu);
    p
}

#[test]
fn rx_mpdu_descriptor_fields() {
    let mpdu = [0x80u8; 40];
    let p = rx_payload(&mpdu, 0, 12, 3, 60, 50, 6); // hdr_len words 12 = 24 bytes
    let r = decode_rx_mpdu_v1(&p).unwrap();
    assert!(r.crc_ok);
    assert_eq!(r.channel, 6);
    assert_eq!(r.rssi, -50, "max(-60, -50)");
    assert_eq!(r.hdr_len, 24);
    assert_eq!(r.mpdu.len(), 40);
    assert!(!r.padded);
    // a missing chain (energy 0) counts as -128 dBm, so the other wins
    assert_eq!(decode_rx_mpdu_v1(&rx_payload(&mpdu, 0, 12, 3, 0, 70, 1)).unwrap().rssi, -70);
    assert_eq!(decode_rx_mpdu_v1(&rx_payload(&mpdu, 0, 12, 3, 0, 0, 1)).unwrap().rssi, -128);
    // CRC_OK (bit0) and OVERRUN_OK (bit1) must both be set
    assert!(!decode_rx_mpdu_v1(&rx_payload(&mpdu, 0, 12, 1, 60, 50, 6)).unwrap().crc_ok);
    assert!(!decode_rx_mpdu_v1(&rx_payload(&mpdu, 0, 12, 2, 60, 50, 6)).unwrap().crc_ok);
    // mic_crc_len: mac_flags1 high nibble, in 2-byte words: 0x20 -> nibble 2 -> 4 bytes stripped
    assert_eq!(decode_rx_mpdu_v1(&rx_payload(&mpdu, 0x20, 12, 3, 60, 50, 6)).unwrap().mpdu.len(), 36);
    // the PAD bit (0x20 of mac_flags2) is reported
    assert!(decode_rx_mpdu_v1(&rx_payload(&mpdu, 0, 12 | 0x20, 3, 60, 50, 6)).unwrap().padded);
}

#[test]
fn rx_mpdu_refuses_a_short_or_lying_packet() {
    assert_eq!(decode_rx_mpdu_v1(&[0u8; 47]), Err(DecodeError::Short { have: 47, need: 48 }));
    let mut p = rx_payload(&[0u8; 10], 0, 12, 3, 1, 1, 1);
    p[0] = 11; // mpdu_len says 11, 10 follow
    assert_eq!(decode_rx_mpdu_v1(&p), Err(DecodeError::RxLenLie { mpdu_len: 11, desc_size: 48, pkt_len: 58 }));
}
