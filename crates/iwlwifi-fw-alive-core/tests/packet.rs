// SPDX-License-Identifier: GPL-2.0-only
//! Vectors for the RX packet header.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/iwl-trans.h:63-102` and
//! `drivers/net/wireless/intel/iwlwifi/fw/api/cmdhdr.h`. Copyright (C) 2005-2014, 2018-2025 Intel
//! Corporation, and the Linux iwlwifi authors.

use iwlwifi_fw_alive_core::packet::*;

#[test]
fn constants_are_linux() {
    assert_eq!(FH_RSCSR_FRAME_SIZE_MSK, 0x00003FFF); // iwl-trans.h:63
    assert_eq!(FH_RSCSR_FRAME_INVALID, 0x55550000); // iwl-trans.h:64
    assert_eq!(FH_RSCSR_RXQ_POS, 16); // iwl-trans.h:68
    assert_eq!(FH_RSCSR_RXQ_MASK, 0x3F0000); // iwl-trans.h:69
    assert_eq!(SEQ_RX_FRAME, 0x8000); // cmdhdr.h:22
    assert_eq!(RX_PACKET_HDR_LEN, 8);
}

#[test]
fn a_packet_splits_into_header_and_a_payload_sized_by_bits_0_13() {
    // len_n_flags: queue 5 (bits 21:16), flag bit 31, size 4 + 3 = 7.
    let mut rb = (0x8005_0007u32).to_le_bytes().to_vec();
    rb.extend_from_slice(&[0x01, 0x00, 0x34, 0x92]); // cmd 1, group 0, seq 0x9234
    rb.extend_from_slice(&[0xA, 0xB, 0xC, 0xEE, 0xEE]); // 3 payload bytes + trailing slack
    let p = parse_rx_packet(&rb).unwrap();
    assert_eq!((p.cmd, p.group_id, p.sequence), (1, 0, 0x9234));
    assert_eq!(p.payload, &[0xA, 0xB, 0xC]);
    assert!(p.is_notification());
    assert_eq!(p.rx_queue(), 5);
    let mut resp = rb.clone();
    resp[7] = 0x12; // sequence 0x1234: a command response
    assert!(!parse_rx_packet(&resp).unwrap().is_notification());
}

#[test]
fn bad_buffers_are_refused_by_name() {
    assert_eq!(parse_rx_packet(&[0; 7]), Err(PacketError::TooShort { len: 7 }));
    let mut inv = 0x5555_0010u32.to_le_bytes().to_vec();
    inv.extend_from_slice(&[0; 20]);
    assert_eq!(parse_rx_packet(&inv), Err(PacketError::FrameInvalid { len_n_flags: 0x5555_0010 }));
    let mut tiny = 3u32.to_le_bytes().to_vec();
    tiny.extend_from_slice(&[0; 4]);
    assert_eq!(parse_rx_packet(&tiny), Err(PacketError::SizeBelowHeader { size: 3 }));
    let mut big = 0x3FFFu32.to_le_bytes().to_vec();
    big.extend_from_slice(&[0; 8]);
    assert_eq!(parse_rx_packet(&big), Err(PacketError::SizeOverrun { size: 0x3FFF, avail: 8 }));
    // A size above bit 13 is masked off, not trusted. // iwl-trans.h:96
    let mut hi = 0x0000_4004u32.to_le_bytes().to_vec();
    hi.extend_from_slice(&[1, 0, 0, 0]);
    assert_eq!(parse_rx_packet(&hi).unwrap().payload, &[] as &[u8]);
}
