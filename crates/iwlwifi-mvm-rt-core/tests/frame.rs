// SPDX-License-Identifier: GPL-2.0-only
//! Beacon / probe-response reduction, with frames assembled by hand from IEEE 802.11-2020 9.3.3.3 and 9.4.2.
use iwlwifi_mvm_rt_core::decode::RxMpdu;
use iwlwifi_mvm_rt_core::frame::*;

const BSSID: [u8; 6] = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0xee];

fn frame(subtype: u8, cap: u16, ies: &[u8]) -> Vec<u8> {
    let mut f = vec![0u8; 24];
    f[0] = subtype; // frame control byte 0: type mgmt (00), subtype in bits 4-7
    f[4..10].fill(0xff);
    f[10..16].copy_from_slice(&BSSID);
    f[16..22].copy_from_slice(&BSSID);
    f.extend_from_slice(&[0u8; 8]); // timestamp
    f.extend_from_slice(&100u16.to_le_bytes()); // beacon interval
    f.extend_from_slice(&cap.to_le_bytes());
    f.extend_from_slice(ies);
    f
}

fn ie(id: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![id, body.len() as u8];
    v.extend_from_slice(body);
    v
}

fn rx<'a>(mpdu: &'a [u8], ch: u8, rssi: i16) -> RxMpdu<'a> {
    RxMpdu { crc_ok: true, channel: ch, rssi, hdr_len: 24, mpdu, padded: false }
}

/// A WPA2-Personal RSN element: version 1, group CCMP, pairwise [CCMP], AKM [PSK], capabilities 0.
fn rsn(akms: &[u8], pairwise: &[u8]) -> Vec<u8> {
    let mut b = vec![1, 0, 0x00, 0x0f, 0xac, 4];
    b.extend_from_slice(&(pairwise.len() as u16).to_le_bytes());
    for p in pairwise {
        b.extend_from_slice(&[0x00, 0x0f, 0xac, *p]);
    }
    b.extend_from_slice(&(akms.len() as u16).to_le_bytes());
    for a in akms {
        b.extend_from_slice(&[0x00, 0x0f, 0xac, *a]);
    }
    b.extend_from_slice(&[0, 0]);
    ie(48, &b)
}

#[test]
fn an_open_beacon() {
    let mut ies = ie(0, b"CoralNet");
    ies.extend(ie(3, &[6]));
    let f = frame(0x80, 0x0401, &ies);
    let h = heard_from_rx(&rx(&f, 6, -42)).unwrap();
    assert_eq!(&h.ssid[..h.ssid_len as usize], b"CoralNet");
    assert_eq!((h.bssid, h.channel, h.rssi, h.security), (BSSID, 6, -42, Security::Open));
}

#[test]
fn a_wpa2_psk_probe_response() {
    let mut ies = ie(0, b"Home");
    ies.extend(ie(3, &[11]));
    ies.extend(rsn(&[2], &[4]));
    let f = frame(0x50, 0x0411, &ies); // privacy bit (bit 4) set
    let h = heard_from_rx(&rx(&f, 1, -61)).unwrap();
    assert_eq!((h.channel, h.security), (11, Security::Wpa2Psk), "the DS element wins over the receive channel");
}

#[test]
fn security_classification_family() {
    let cases: &[(&str, u16, Vec<u8>, Security)] = &[
        ("open, no privacy bit", 0x0001, vec![], Security::Open),
        ("wpa2 psk ccmp", 0x0011, rsn(&[2], &[4]), Security::Wpa2Psk),
        ("transition: sae then psk", 0x0011, rsn(&[8, 2], &[4]), Security::Wpa2Psk),
        ("sae only", 0x0011, rsn(&[8], &[4]), Security::Unsupported),
        ("enterprise 802.1x", 0x0011, rsn(&[1], &[4]), Security::Unsupported),
        ("psk but tkip only", 0x0011, rsn(&[2], &[2]), Security::Unsupported),
        ("wep: privacy bit, no rsn", 0x0011, vec![], Security::Unsupported),
        ("wpa1 vendor element only", 0x0011, ie(221, &[0x00, 0x50, 0xf2, 1, 1, 0]), Security::Unsupported),
    ];
    for (name, cap, extra, want) in cases {
        let mut ies = ie(0, b"x");
        ies.extend(extra);
        let f = frame(0x80, *cap, &ies);
        assert_eq!(heard_from_rx(&rx(&f, 1, -50)).unwrap().security, *want, "{name}");
    }
}

#[test]
fn a_lying_rsn_count_is_unsupported_not_a_panic() {
    let mut r = rsn(&[2], &[4]);
    r[2 + 8] = 9; // pairwise count claims 9 suites
    let mut ies = ie(0, b"x");
    ies.extend(r);
    let f = frame(0x80, 0x0011, &ies);
    assert_eq!(heard_from_rx(&rx(&f, 1, -50)).unwrap().security, Security::Unsupported);
}

#[test]
fn channel_falls_back_to_ht_operation_then_the_receive_channel() {
    let mut ies = ie(0, b"x");
    ies.extend(ie(61, &[36, 0, 0, 0, 0, 0]));
    let f = frame(0x80, 1, &ies);
    assert_eq!(heard_from_rx(&rx(&f, 40, -50)).unwrap().channel, 36);
    let f = frame(0x80, 1, &ie(0, b"x"));
    assert_eq!(heard_from_rx(&rx(&f, 40, -50)).unwrap().channel, 40);
}

#[test]
fn hidden_and_empty_ssids_are_reported_as_zero_length_for_the_manager_to_filter() {
    let f = frame(0x80, 1, &ie(0, &[]));
    assert_eq!(heard_from_rx(&rx(&f, 1, -50)).unwrap().ssid_len, 0);
    let f = frame(0x80, 1, &[]);
    assert_eq!(heard_from_rx(&rx(&f, 1, -50)).unwrap().ssid_len, 0, "no SSID element at all");
}

#[test]
fn frames_that_are_not_results_are_skipped_by_name() {
    let good = frame(0x80, 1, &ie(0, b"x"));
    assert_eq!(heard_from_rx(&RxMpdu { crc_ok: false, ..rx(&good, 1, -50) }), Err(Skip::BadCrc));
    assert_eq!(heard_from_rx(&rx(&frame(0x40, 1, &[]), 1, -50)), Err(Skip::NotBeaconOrProbeResp), "probe request");
    assert_eq!(heard_from_rx(&rx(&frame(0xb0, 1, &[]), 1, -50)), Err(Skip::NotBeaconOrProbeResp), "authentication");
    let mut data = good.clone();
    data[0] = 0x88; // data frame (type 10), QoS data
    assert_eq!(heard_from_rx(&rx(&data, 1, -50)), Err(Skip::NotBeaconOrProbeResp));
    assert_eq!(heard_from_rx(&rx(&good[..35], 1, -50)), Err(Skip::Truncated));
    let mut over = frame(0x80, 1, &ie(0, b"x"));
    over.extend_from_slice(&[7, 200, 1]); // an element claiming 200 bytes
    assert_eq!(heard_from_rx(&rx(&over, 1, -50)), Err(Skip::ElementOverrun));
    let long = frame(0x80, 1, &ie(0, &[b'a'; 33]));
    assert_eq!(heard_from_rx(&rx(&long, 1, -50)), Err(Skip::SsidTooLong));
}

#[test]
fn the_pad_flag_skips_two_bytes_after_the_header() {
    let mut f = frame(0x80, 0x0001, &ie(0, b"padded"));
    f.splice(24..24, [0xee, 0xee]); // the device inserted two pad bytes after the 24-byte header
    let h = heard_from_rx(&RxMpdu { padded: true, ..rx(&f, 1, -50) }).unwrap();
    assert_eq!(&h.ssid[..h.ssid_len as usize], b"padded");
    assert_eq!(h.security, Security::Open);
}
