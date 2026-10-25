// SPDX-License-Identifier: GPL-2.0-only
//! Rates, TX command, 802.11 frames and the beacon reduction, against literals.

use iwlwifi_mvm_join_core::bss::*;
use iwlwifi_mvm_join_core::dot11::*;
use iwlwifi_mvm_join_core::ids::*;
use iwlwifi_mvm_join_core::rates::*;
use iwlwifi_mvm_join_core::txcmd::*;
use iwlwifi_mvm_join_core::EncodeError;

// ---------------------------------------------------------------------------------------------- rates

#[test]
fn rate_tables_are_the_linux_literals() {
    assert_eq!(PLCP, [10, 20, 55, 110, 13, 15, 5, 7, 9, 11, 1, 3]); // fw/rs.c fw_rate_idx_to_plcp via rs.h PLCP codes
    assert_eq!(UNITS_500K, [2, 4, 11, 22, 12, 18, 24, 36, 48, 72, 96, 108]);
    assert_eq!((RATE_COUNT, FIRST_CCK, LAST_CCK, FIRST_OFDM), (12, 0, 3, 4));
    let idx = [RATE_1M, RATE_2M, RATE_5M, RATE_11M, RATE_6M, RATE_9M, RATE_12M, RATE_18M, RATE_24M, RATE_36M, RATE_48M, RATE_54M];
    assert_eq!(idx, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]);
}

#[test]
fn rate_octets() {
    assert_eq!(index_of(0x82), Some(RATE_1M)); // 1 Mbit/s, basic
    assert_eq!(index_of(0x0c), Some(RATE_6M));
    assert_eq!(index_of(0x6c), Some(RATE_54M));
    assert_eq!(index_of(0xff), None); // the HT PHY membership selector
    assert_eq!(index_of(0x00), None);
    assert!(is_basic(0x82) && !is_basic(0x02));
    assert!(is_cck(RATE_11M) && !is_cck(RATE_6M));
}

#[test]
fn rate_n_flags_version_1() {
    assert_eq!(RATE_MCS_ANT_POS, 14);
    assert_eq!(RATE_MCS_CCK_MSK_V1, 0x200);
    assert_eq!(rate_n_flags_v1(RATE_6M, 1), 0x400d); // PLCP 13, antenna A
    assert_eq!(rate_n_flags_v1(RATE_24M, 2), 0x8009); // PLCP 9, antenna B
    assert_eq!(rate_n_flags_v1(RATE_1M, 3), 0xc20a); // CCK flag | PLCP 10 | antennas A+B
    assert_eq!(rate_n_flags_v1(RATE_11M, 1), 0x4200 | 110);
    assert_eq!(rate_n_flags_v1(RATE_54M, 1), 0x4003);
}

#[test]
fn ack_rate_bitmaps() {
    let bit = |i: usize| 1u16 << i;
    // No basic rates at all: every CCK rate and 6/12/24 Mbit/s OFDM may acknowledge.
    assert_eq!(ack_rates(0), (0xf, 0x15));
    // A 5 GHz AP: 6, 12, 24 basic.
    assert_eq!(ack_rates(bit(RATE_6M) | bit(RATE_12M) | bit(RATE_24M)), (0xf, 0x15));
    // Only 6 Mbit/s basic: 12 and 24 are not below it, so they are not added.
    assert_eq!(ack_rates(bit(RATE_6M)), (0xf, 0x01));
    // Only 24 Mbit/s basic: 12 and 6 are mandatory below it.
    assert_eq!(ack_rates(bit(RATE_24M)), (0xf, 0x15));
    // 1 and 2 Mbit/s basic: 5.5 and 11 are above the lowest, so not added.
    assert_eq!(ack_rates(bit(RATE_1M) | bit(RATE_2M)).0, 0x3);
    // 2 Mbit/s basic: 1 Mbit/s is mandatory below it.
    assert_eq!(ack_rates(bit(RATE_2M)).0, 0x3);
    // 5.5 Mbit/s basic: 1 and 2 mandatory, 11 above.
    assert_eq!(ack_rates(bit(RATE_5M)).0, 0x7);
    // 11 Mbit/s basic: everything below.
    assert_eq!(ack_rates(bit(RATE_11M)).0, 0xf);
    // The rt-core no-basic-rate constants are these.
    assert_eq!(ack_rates(0), (iwlwifi_mvm_rt_core::ids::ACK_RATES_CCK_NO_BASIC, iwlwifi_mvm_rt_core::ids::ACK_RATES_OFDM_NO_BASIC));
}

#[test]
fn lowest_rate_for_management_frames() {
    let bit = |i: usize| 1u16 << i;
    assert_eq!(lowest_rate(bit(RATE_6M) | bit(RATE_12M), false), RATE_6M);
    assert_eq!(lowest_rate(bit(RATE_12M) | bit(RATE_24M), false), RATE_12M);
    assert_eq!(lowest_rate(0, false), RATE_6M);
    assert_eq!(lowest_rate(bit(RATE_1M) | bit(RATE_6M), true), RATE_1M);
    assert_eq!(lowest_rate(bit(RATE_6M), true), RATE_6M); // 2.4 GHz with only OFDM basic rates
    assert_eq!(lowest_rate(0, true), RATE_1M);
    assert_eq!(lowest_rate(bit(RATE_1M), false), RATE_6M); // 5 GHz ignores CCK
}

// ------------------------------------------------------------------------------------------- TX command

fn tx(hdr: u8) -> TxParams {
    TxParams { sta_id: 0, tid: IWL_TID_NON_QOS, frame_len: 153, hdr_len: hdr, rate_n_flags: 0x400d, key: None, ack: true, fw_seq: true, pm_frame_timeout: PM_FRAME_NONE, bt_prio: 0 }
}

#[test]
fn tx_constants_are_the_linux_literals() {
    assert_eq!((TX_CMD_FLG_ACK, TX_CMD_FLG_BT_DIS, TX_CMD_FLG_SEQ_CTL, TX_CMD_FLG_MH_PAD), (8, 0x1000, 0x2000, 0x10_0000));
    assert_eq!((TX_CMD_SEC_CCM, TX_CMD_SEC_MSK), (2, 7));
    assert_eq!(TX_CMD_LIFE_TIME_INFINITE, 0xffff_ffff);
    assert_eq!((IWL_DEFAULT_TX_RETRY, IWL_MGMT_DFAULT_RETRY_LIMIT, IWL_RTS_DFAULT_RETRY_LIMIT), (15, 3, 60));
    assert_eq!((PM_FRAME_NONE, PM_FRAME_MGMT, PM_FRAME_ASSOC), (0, 2, 3));
    assert_eq!((IWL_TID_NON_QOS, IWL_MAX_TID_COUNT), (0, 8));
    assert_eq!((TX_CMD_OFFLD_MH_SIZE, TX_CMD_OFFLD_PAD), (8, 13));
    assert_eq!(TX_CMD_PARAMS_LEN, 2 + 2 + 4 + 4 + 4 + 1 + 1 + 1 + 1 + 16 + 4 + 4 + 4 + 1 + 1 + 1 + 1 + 2 + 2);
    assert_eq!((CMD_HEADER_LEN, IWL_FIRST_TB_SIZE, IWL_TX_CRC_SIZE, IWL_TX_DELIMITER_SIZE), (4, 20, 4, 4));
    assert_eq!((TX_STATUS_MSK, TX_STATUS_SUCCESS, TX_STATUS_DIRECT_DONE), (0xff, 1, 2));
    assert_eq!((IWL_MVM_DQA_MIN_MGMT_QUEUE, IWL_MVM_TX_FIFO_BK, IWL_MVM_TX_FIFO_BE, IWL_MVM_TX_FIFO_VI, IWL_MVM_TX_FIFO_VO), (5, 0, 1, 2, 3));
}

#[test]
fn tx_cmd_for_an_unprotected_data_frame() {
    let mut b = [0u8; 64];
    assert_eq!(tx_cmd_params(&tx(24), 0x2_1234_5678, &mut b), Ok(56));
    let mut w = [0u8; 56];
    w[0..2].copy_from_slice(&153u16.to_le_bytes()); // len
    w[2..4].copy_from_slice(&0x0c00u16.to_le_bytes()); // offload_assist: mac header 24 bytes = 12 words << 8
    w[4..8].copy_from_slice(&0x2008u32.to_le_bytes()); // ACK | SEQ_CTL
    w[12..16].copy_from_slice(&0x400du32.to_le_bytes()); // rate_n_flags: 6 Mbit/s, antenna A
    w[16] = 0; // sta_id
    w[17] = 0; // sec_ctl
    w[40..44].copy_from_slice(&0xffff_ffffu32.to_le_bytes()); // life_time
    w[44..48].copy_from_slice(&0x1234_5678u32.to_le_bytes()); // dram_lsb_ptr
    w[48] = 2; // dram_msb_ptr
    w[49] = 60; // rts_retry_limit
    w[50] = 15; // data_retry_limit
    w[51] = 0; // tid_tspec
    w[52..54].copy_from_slice(&0u16.to_le_bytes()); // pm_frame_timeout
    assert_eq!(&b[..56], &w[..]);
}

#[test]
fn tx_cmd_for_an_authentication_frame_on_2_4_ghz() {
    let mut p = tx(24);
    p.tid = IWL_MAX_TID_COUNT;
    p.frame_len = 30;
    p.rate_n_flags = 0x420a; // 1 Mbit/s CCK, antenna A
    p.pm_frame_timeout = PM_FRAME_MGMT;
    p.bt_prio = 3;
    let mut b = [0u8; 56];
    tx_cmd_params(&p, 0x1000, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 0x1800 | 0x2008); // BT priority 3 << 11
    assert_eq!(b[51], 8);
    assert_eq!(u16::from_le_bytes([b[52], b[53]]), 2);
    assert_eq!(u32::from_le_bytes(b[12..16].try_into().unwrap()), 0x420a);
    assert_eq!(u16::from_le_bytes([b[0], b[1]]), 30);
}

#[test]
fn tx_cmd_for_a_ccmp_protected_frame() {
    let mut p = tx(24);
    p.key = Some([0xa0; 16]);
    let mut b = [0u8; 56];
    tx_cmd_params(&p, 0, &mut b).unwrap();
    assert_eq!(b[17], 2); // TX_CMD_SEC_CCM
    assert_eq!(&b[20..36], &[0xa0u8; 16]);
    // The mac header now counts the 8-byte IV: 32 bytes = 16 words.
    assert_eq!(u16::from_le_bytes([b[2], b[3]]), 0x1000);
    // Unprotected frames carry no key bytes.
    tx_cmd_params(&tx(24), 0, &mut b).unwrap();
    assert_eq!(&b[20..36], &[0u8; 16]);
}

#[test]
fn tx_cmd_pads_a_header_that_is_not_dword_sized() {
    let mut b = [0u8; 56];
    tx_cmd_params(&tx(26), 0, &mut b).unwrap();
    assert_eq!(u16::from_le_bytes([b[2], b[3]]), (13 << 8) | (1 << 13));
}

#[test]
fn tx_cmd_buffer() {
    let mut b = [0u8; 55];
    assert_eq!(tx_cmd_params(&tx(24), 0, &mut b), Err(EncodeError::BufferTooSmall { need: 56, have: 55 }));
}

#[test]
fn transfer_buffer_plan() {
    // cmd header 4 + params 56 + header 24 = 84 bytes: TB0 the first 20, TB1 the other 64.
    assert_eq!(tb_plan(24), TbPlan { tb0_len: 20, tb1_len: 64, mh_pad: false });
    // A 26-byte QoS header makes 86: TB1 is 66 bytes, rounded to 68, and the command says so.
    assert_eq!(tb_plan(26), TbPlan { tb0_len: 20, tb1_len: 68, mh_pad: true });
    assert_eq!(tb_plan(30), TbPlan { tb0_len: 20, tb1_len: 72, mh_pad: true });
}

#[test]
fn scheduler_byte_count() {
    // (100 + 4 CRC + 4 delimiter) / 4
    assert_eq!(byte_count_entry(100, 0, 0), Ok(27));
    // CCMP adds the 8-byte MIC: (100 + 8 + 8) / 4
    assert_eq!(byte_count_entry(100, TX_CMD_SEC_CCM, 0), Ok(29));
    // rounded up
    assert_eq!(byte_count_entry(101, 0, 0), Ok(28));
    // the station id sits in bits 12..15
    assert_eq!(byte_count_entry(100, 0, 1), Ok(27 | 0x1000));
    assert_eq!(byte_count_entry(100, 0, 0xf), Ok(27 | 0xf000));
    assert_eq!(byte_count_entry(0x4000, 0, 0), Err(ByteCountRefusal::TooLong(4098)));
    assert_eq!(byte_count_entry(0x3ff0, 0, 0), Ok((0x3ff0 + 8u32).div_ceil(4) as u16));
}

fn resp(frame_count: u8, status: u16, ssn: u32) -> Vec<u8> {
    let mut p = vec![0u8; 36 + 4 * frame_count as usize + 4];
    p[0] = frame_count;
    p[2] = 1; // failure_rts
    p[3] = 2; // failure_frame
    p[4..8].copy_from_slice(&0x400du32.to_le_bytes());
    p[33] = 0x05; // ra_tid: RA 0, TID 5
    p[36..38].copy_from_slice(&status.to_le_bytes());
    let o = 36 + 4 * frame_count as usize;
    p[o..o + 4].copy_from_slice(&ssn.to_le_bytes());
    p
}

#[test]
fn tx_response() {
    let r = decode_tx_resp(&resp(1, 0x0001, 0xf007)).unwrap();
    assert_eq!(r, TxResp { frame_count: 1, failure_rts: 1, failure_frame: 2, initial_rate: 0x400d, status: 1, ssn: 7, ra: 0, tid: 5 });
    assert!(r.success());
    assert!(decode_tx_resp(&resp(1, 0x0002, 0)).unwrap().success()); // DIRECT_DONE
    let f = decode_tx_resp(&resp(1, 0x0083, 0)).unwrap();
    assert_eq!(f.status, 0x83);
    assert!(!f.success());
    // The status word is masked to its low byte; upper bits are other fields.
    assert!(decode_tx_resp(&resp(1, 0x4101, 0)).unwrap().success());
    // The SSN follows the status array, whatever its length.
    assert_eq!(decode_tx_resp(&resp(3, 1, 0x123)).unwrap().ssn, 0x123);
    let mut ra = resp(1, 1, 0);
    ra[33] = 0x31;
    assert_eq!((decode_tx_resp(&ra).unwrap().ra, decode_tx_resp(&ra).unwrap().tid), (3, 1));
}

#[test]
fn tx_response_refusals() {
    assert!(matches!(decode_tx_resp(&[0u8; 39]), Err(TxRespRefusal::Short { have: 39, .. })));
    assert!(matches!(decode_tx_resp(&resp(0, 1, 0)), Err(TxRespRefusal::Short { .. })));
    let mut p = resp(2, 1, 0);
    p.truncate(36 + 8);
    assert!(matches!(decode_tx_resp(&p), Err(TxRespRefusal::Short { have: 44, need: 48 })));
}

// ------------------------------------------------------------------------------------------- 802.11

const BSSID: Mac = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x01];
const STA: Mac = [0x12, 0x22, 0x33, 0x44, 0x55, 0x66];

#[test]
fn frame_constants() {
    assert_eq!((HDR_LEN, QOS_HDR_LEN, CCMP_HDR_LEN, LLC_SNAP_LEN), (24, 26, 8, 8));
    assert_eq!((SUBTYPE_ASSOC_REQ, SUBTYPE_ASSOC_RESP, SUBTYPE_PROBE_RESP, SUBTYPE_BEACON), (0x00, 0x10, 0x50, 0x80));
    assert_eq!((SUBTYPE_DISASSOC, SUBTYPE_AUTH, SUBTYPE_DEAUTH), (0xa0, 0xb0, 0xc0));
    assert_eq!((FC1_TO_DS, FC1_FROM_DS, FC1_PROTECTED), (1, 2, 0x40));
    assert_eq!(ETHERTYPE_EAPOL, 0x888e);
}

#[test]
fn authentication_request() {
    let mut b = [0u8; 40];
    assert_eq!(auth_request(&mut b, &BSSID, &STA), Ok(30));
    let mut w = vec![0xb0, 0x00, 0, 0];
    w.extend_from_slice(&BSSID);
    w.extend_from_slice(&STA);
    w.extend_from_slice(&BSSID);
    w.extend_from_slice(&[0, 0]); // sequence control
    w.extend_from_slice(&[0, 0, 1, 0, 0, 0]); // open system, transaction 1, success
    assert_eq!(&b[..30], &w[..]);
}

#[test]
fn deauthentication() {
    let mut b = [0u8; 32];
    assert_eq!(deauth(&mut b, &BSSID, &STA, 15), Ok(26));
    assert_eq!(b[0], 0xc0);
    assert_eq!(&b[4..10], &BSSID);
    assert_eq!(&b[24..26], &[15, 0]);
}

#[test]
fn association_request() {
    let rsne = [0x30, 0x14, 1, 0, 0, 0x0f, 0xac, 4, 1, 0, 0, 0x0f, 0xac, 4, 1, 0, 0, 0x0f, 0xac, 2, 0, 0];
    let rates = [2u8, 4, 11, 22, 12, 18, 24, 36, 48, 72, 96, 108];
    let a = AssocReq { sa: STA, bssid: BSSID, capability: 0x0431, listen_interval: 10, ssid: b"Net", rates: &rates, rsne: Some(&rsne) };
    let mut b = [0u8; 128];
    let n = assoc_request(&mut b, &a).unwrap();
    let mut w = vec![0x00, 0x00, 0, 0];
    w.extend_from_slice(&BSSID);
    w.extend_from_slice(&STA);
    w.extend_from_slice(&BSSID);
    w.extend_from_slice(&[0, 0]);
    w.extend_from_slice(&[0x31, 0x04, 10, 0]); // capability, listen interval
    w.extend_from_slice(&[0, 3, b'N', b'e', b't']);
    w.extend_from_slice(&[1, 8, 2, 4, 11, 22, 12, 18, 24, 36]); // Supported Rates: first eight
    w.extend_from_slice(&[50, 4, 48, 72, 96, 108]); // Extended Supported Rates: the rest
    w.extend_from_slice(&rsne);
    assert_eq!(&b[..n], &w[..]);
    // Eight or fewer rates carry no extended element; an open network no RSNE.
    let a = AssocReq { sa: STA, bssid: BSSID, capability: 0x0401, listen_interval: 10, ssid: b"", rates: &rates[..8], rsne: None };
    let n = assoc_request(&mut b, &a).unwrap();
    assert_eq!(n, 24 + 4 + 2 + 10);
    assert!(!b[28..n].contains(&50));
}

#[test]
fn association_request_refusals() {
    let long = [b'x'; 33];
    let a = AssocReq { sa: STA, bssid: BSSID, capability: 0, listen_interval: 0, ssid: &long, rates: &[2], rsne: None };
    assert_eq!(assoc_request(&mut [0u8; 200], &a), Err(EncodeError::OutOfRange { what: "ssid", value: 33, max: 32 }));
    let a = AssocReq { ssid: b"a", ..a };
    assert!(matches!(assoc_request(&mut [0u8; 10], &a), Err(EncodeError::BufferTooSmall { .. })));
}

#[test]
fn data_frame_pieces() {
    let mut h = [0u8; 24];
    assert_eq!(data_header_to_ds(&mut h, &BSSID, &STA, &[0xff; 6], false), Ok(24));
    assert_eq!(&h[..4], &[0x08, 0x01, 0, 0]); // data, To-DS
    assert_eq!(&h[4..10], &BSSID); // addr1 = BSSID
    assert_eq!(&h[10..16], &STA); // addr2 = SA
    assert_eq!(&h[16..22], &[0xff; 6]); // addr3 = DA
    data_header_to_ds(&mut h, &BSSID, &STA, &BSSID, true).unwrap();
    assert_eq!(h[1], 0x41); // To-DS | Protected
    let mut l = [0u8; 8];
    llc_snap(&mut l, ETHERTYPE_EAPOL).unwrap();
    assert_eq!(l, [0xaa, 0xaa, 0x03, 0, 0, 0, 0x88, 0x8e]);
    let mut c = [0u8; 8];
    ccmp_header(&mut c, 0x0605_0403_0201, 1).unwrap();
    assert_eq!(c, [0x01, 0x02, 0x00, 0x60, 0x03, 0x04, 0x05, 0x06]);
    ccmp_header(&mut c, 1, 0).unwrap();
    assert_eq!(c, [1, 0, 0, 0x20, 0, 0, 0, 0]);
}

#[test]
fn parse_management() {
    let mut f = vec![0xb0, 0, 0, 0];
    f.extend_from_slice(&STA);
    f.extend_from_slice(&BSSID);
    f.extend_from_slice(&BSSID);
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(&[0, 0, 2, 0, 0, 0]); // authentication, transaction 2, success
    let m = parse_mgmt(&f, false).unwrap();
    assert_eq!((m.subtype, m.da, m.sa, m.bssid), (SUBTYPE_AUTH, STA, BSSID, BSSID));
    assert_eq!(parse_auth(m.body), Ok((0, 2, 0)));
    // With the device's two pad bytes after the header.
    let mut p = f[..24].to_vec();
    p.extend_from_slice(&[0, 0]);
    p.extend_from_slice(&f[24..]);
    assert_eq!(parse_mgmt(&p, true).unwrap().body, m.body);
    assert_eq!(parse_mgmt(&f[..20], false), Err(RxRefusal::Short { have: 20, need: 24 }));
    let mut d = f.clone();
    d[0] = 0x08;
    assert_eq!(parse_mgmt(&d, false), Err(RxRefusal::WrongType(0x08)));
    assert_eq!(parse_auth(&[0; 5]), Err(RxRefusal::Short { have: 5, need: 6 }));
}

#[test]
fn parse_association_response() {
    assert_eq!(parse_assoc_resp(&[0x31, 0x04, 0, 0, 0x05, 0xc0, 1, 2]), Ok((0x0431, 0, 5))); // the top two AID bits are masked
    assert_eq!(parse_assoc_resp(&[0x31, 0x04, 17, 0, 0x01, 0xc0]), Ok((0x0431, 17, 1)));
    assert_eq!(parse_assoc_resp(&[0; 5]), Err(RxRefusal::Short { have: 5, need: 6 }));
    assert_eq!(parse_reason(&[15, 0]), Ok(15));
    assert_eq!(parse_reason(&[15]), Err(RxRefusal::Short { have: 1, need: 2 }));
}

fn data_from_ds(fc1: u8, qos: bool, pad: bool, ccmp: Option<[u8; 8]>, ethertype: [u8; 2]) -> Vec<u8> {
    let mut f = vec![if qos { 0x88 } else { 0x08 }, fc1, 0, 0];
    f.extend_from_slice(&STA); // addr1 = DA (us)
    f.extend_from_slice(&BSSID); // addr2 = BSSID
    f.extend_from_slice(&[0x02, 0x11, 0x22, 0x33, 0x44, 0x55]); // addr3 = SA
    f.extend_from_slice(&[0, 0]);
    if qos {
        f.extend_from_slice(&[0, 0]);
    }
    if pad {
        f.extend_from_slice(&[0, 0]);
    }
    if let Some(c) = ccmp {
        f.extend_from_slice(&c);
    }
    f.extend_from_slice(&[0xaa, 0xaa, 0x03, 0, 0, 0, ethertype[0], ethertype[1]]);
    f.extend_from_slice(&[1, 2, 3, 4]);
    f
}

#[test]
fn parse_from_ds_data() {
    let f = data_from_ds(0x02, false, false, None, [0x88, 0x8e]);
    let d = parse_data(&f, false).unwrap();
    assert_eq!((d.da, d.bssid, d.ethertype, d.payload, d.protected, d.pn), (STA, BSSID, 0x888e, &[1, 2, 3, 4][..], false, None));
    assert_eq!(d.sa, [0x02, 0x11, 0x22, 0x33, 0x44, 0x55]);
    // Protected: the CCMP header is read and skipped.
    let c = [0x01, 0x02, 0, 0x60, 0x03, 0x04, 0x05, 0x06];
    let f = data_from_ds(0x42, false, false, Some(c), [0x08, 0x00]);
    let d = parse_data(&f, false).unwrap();
    assert_eq!((d.protected, d.pn, d.ethertype, d.payload), (true, Some((0x0605_0403_0201, 1)), 0x0800, &[1, 2, 3, 4][..]));
    // QoS data with the device's pad.
    let f = data_from_ds(0x02, true, true, None, [0x86, 0xdd]);
    let d = parse_data(&f, true).unwrap();
    assert_eq!((d.ethertype, d.payload), (0x86dd, &[1, 2, 3, 4][..]));
}

#[test]
fn parse_data_refusals() {
    assert_eq!(parse_data(&[0u8; 10], false), Err(RxRefusal::Short { have: 10, need: 24 }));
    assert_eq!(parse_data(&data_from_ds(0x01, false, false, None, [8, 0]), false), Err(RxRefusal::NotFromDs(0x01))); // To-DS
    assert_eq!(parse_data(&data_from_ds(0x03, false, false, None, [8, 0]), false), Err(RxRefusal::NotFromDs(0x03))); // WDS
    let mut m = data_from_ds(0x02, false, false, None, [8, 0]);
    m[0] = 0x80;
    assert_eq!(parse_data(&m, false), Err(RxRefusal::WrongType(0x80)));
    let mut n = data_from_ds(0x02, false, false, None, [8, 0]);
    n[0] = 0x48; // null function
    assert_eq!(parse_data(&n, false), Err(RxRefusal::NoPayload));
    let tkip = [0x01, 0x02, 0, 0x40, 0x03, 0x04, 0x05, 0x06]; // no ExtIV bit
    assert_eq!(parse_data(&data_from_ds(0x42, false, false, Some(tkip), [8, 0]), false), Err(RxRefusal::NotCcmp(0x40)));
    let mut bad = data_from_ds(0x02, false, false, None, [8, 0]);
    bad[24] = 0x00; // not SNAP
    assert_eq!(parse_data(&bad, false), Err(RxRefusal::NotSnap));
    let cut = data_from_ds(0x42, false, false, Some([0, 0, 0, 0x20, 0, 0, 0, 0]), [8, 0]);
    assert!(matches!(parse_data(&cut[..28], false), Err(RxRefusal::Short { .. })));
    assert!(matches!(parse_data(&cut[..36], false), Err(RxRefusal::Short { .. })));
}

// ------------------------------------------------------------------------------------------------ beacon

fn beacon(ies: &[u8]) -> Vec<u8> {
    let mut f = vec![0x80, 0, 0, 0];
    f.extend_from_slice(&[0xff; 6]);
    f.extend_from_slice(&BSSID);
    f.extend_from_slice(&BSSID);
    f.extend_from_slice(&[0, 0]);
    f.extend_from_slice(&0x0102_0304_0506_0708u64.to_le_bytes());
    f.extend_from_slice(&[100, 0]); // beacon interval
    f.extend_from_slice(&[0x31, 0x14]); // capability: ESS, privacy, short preamble, short slot
    f.extend_from_slice(ies);
    f
}

fn full_ies() -> Vec<u8> {
    let mut v = vec![0, 3, b'N', b'e', b't'];
    v.extend_from_slice(&[1, 8, 0x82, 0x84, 0x8b, 0x96, 0x0c, 0x12, 0x18, 0x24]);
    v.extend_from_slice(&[3, 1, 6]);
    v.extend_from_slice(&[5, 4, 1, 3, 0, 0]);
    v.extend_from_slice(&[42, 1, 0x06]);
    v.extend_from_slice(&[50, 4, 0x30, 0x48, 0x60, 0x6c]);
    v.extend_from_slice(&[48, 20, 1, 0, 0, 0x0f, 0xac, 4, 1, 0, 0, 0x0f, 0xac, 4, 1, 0, 0, 0x0f, 0xac, 2, 0, 0]);
    v.extend_from_slice(&[45, 2, 0, 0]);
    v
}

#[test]
fn beacon_reduction() {
    let ies = full_ies();
    let f = beacon(&ies);
    let b = parse_beacon(&f, false).unwrap();
    assert_eq!(b.bssid, BSSID);
    assert_eq!(b.ssid, b"Net");
    assert_eq!(b.tsf, 0x0102_0304_0506_0708);
    assert_eq!(b.beacon_interval, 100);
    assert_eq!(b.capability, 0x1431);
    assert_eq!(b.channel, Some(6));
    assert_eq!(b.supported_rates, 0xfff);
    assert_eq!(b.basic_rates, 0x00f);
    assert_eq!(b.dtim, Some((1, 3)));
    assert!(b.use_protection && b.barker_preamble);
    assert_eq!(b.rsne.unwrap(), &ies[ies.len() - 4 - 22..ies.len() - 4]);
    assert!(b.has_ht);
}

#[test]
fn beacon_channel_falls_back_to_the_ht_operation_element() {
    let mut ies = vec![0, 1, b'x'];
    ies.extend_from_slice(&[61, 3, 36, 0, 0]);
    let f = beacon(&ies);
    let b = parse_beacon(&f, false).unwrap();
    assert_eq!(b.channel, Some(36));
    assert_eq!(parse_beacon(&beacon(&[0, 1, b'x']), false).unwrap().channel, None);
    assert_eq!(parse_beacon(&beacon(&[0, 1, b'x']), false).unwrap().dtim, None);
}

#[test]
fn probe_response_is_a_beacon() {
    let mut f = beacon(&[0, 1, b'x']);
    f[0] = 0x50;
    assert_eq!(parse_beacon(&f, false).unwrap().ssid, b"x");
}

#[test]
fn beacon_refusals() {
    let ies = full_ies();
    let mut f = beacon(&ies);
    f[0] = 0xb0;
    assert_eq!(parse_beacon(&f, false), Err(BssRefusal::NotBeacon(0xb0)));
    let f = beacon(&ies);
    assert!(matches!(parse_beacon(&f[..30], false), Err(BssRefusal::Short { .. })));
    assert_eq!(parse_beacon(&f[..f.len() - 1], false), Err(BssRefusal::ElementOverrun));
    assert_eq!(parse_beacon(&beacon(&[0, 33, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]), false), Err(BssRefusal::BadSsid));
    assert_eq!(parse_beacon(&beacon(&[1, 1, 0x82]), false), Err(BssRefusal::BadSsid)); // no SSID element
}

#[test]
fn unknown_rate_octets_are_ignored() {
    let ies = [0, 1, b'x', 1, 3, 0xff, 0x82, 0x0c];
    let f = beacon(&ies);
    let b = parse_beacon(&f, false).unwrap();
    assert_eq!(b.supported_rates, 0b1_0001);
    assert_eq!(b.basic_rates, 0b1);
}

#[test]
fn offered_rates_ascending() {
    let mut o = [0u8; 12];
    assert_eq!(offered_rates(0xfff, &mut o), 12);
    assert_eq!(o, UNITS_500K);
    let n = offered_rates(0b1_0001, &mut o);
    assert_eq!(&o[..n], &[2, 12]);
    assert_eq!(offered_rates(0, &mut o), 0);
}

// ------------------------------------------------------------------------------------------------ RX security

use iwlwifi_mvm_join_core::rx::*;

#[test]
fn rx_status_constants() {
    assert_eq!((IWL_RX_MPDU_STATUS_CRC_OK, IWL_RX_MPDU_STATUS_OVERRUN_OK, IWL_RX_MPDU_STATUS_KEY_VALID), (1, 2, 8));
    assert_eq!((IWL_RX_MPDU_STATUS_MIC_OK, IWL_RX_MPDU_STATUS_REPLAY_ERROR), (0x40, 0x80));
    assert_eq!((IWL_RX_MPDU_STATUS_SEC_MASK, IWL_RX_MPDU_STATUS_SEC_NONE, IWL_RX_MPDU_STATUS_SEC_CCM), (0x700, 0, 0x200));
    assert_eq!(IWL_RX_MPDU_STATUS_DECRYPTED, 0x800);
    assert_eq!(RX_DESC_STATUS_OFF, 12);
}

#[test]
fn rx_security_decisions() {
    let ok = IWL_RX_MPDU_STATUS_CRC_OK | IWL_RX_MPDU_STATUS_OVERRUN_OK;
    assert_eq!(rx_security(ok, false), Ok(RxSec::Clear));
    assert_eq!(rx_security(ok | IWL_RX_MPDU_STATUS_SEC_CCM, false), Ok(RxSec::Clear));
    assert_eq!(rx_security(ok | IWL_RX_MPDU_STATUS_SEC_CCM | IWL_RX_MPDU_STATUS_MIC_OK, true), Ok(RxSec::CcmpDecrypted));
    assert_eq!(rx_security(ok | IWL_RX_MPDU_STATUS_SEC_CCM, true), Err(RxSecRefusal::BadMic));
    assert_eq!(rx_security(ok, true), Err(RxSecRefusal::NotDecrypted));
    assert_eq!(rx_security(ok | (3 << 8), true), Err(RxSecRefusal::Algorithm(3)));
    assert_eq!(rx_security(ok | (7 << 8), true), Err(RxSecRefusal::Algorithm(7)));
}

#[test]
fn pn_replay_window() {
    let mut w = PnReplay::default();
    assert!(w.accept(0));
    assert!(w.accept(1));
    assert!(!w.accept(1));
    assert!(!w.accept(0));
    assert!(w.accept(10));
    assert!(w.accept(11));
    let mut k = PnReplay::from_rsc(5);
    assert!(!k.accept(5));
    assert!(!k.accept(3));
    assert!(k.accept(6));
}
