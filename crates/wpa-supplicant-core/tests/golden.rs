// SPDX-License-Identifier: ISC
//! Byte-exact pins: the frame layout, the Key Information bits, the RSNE, and one message 2 that
//! an independent implementation (Python hashlib/hmac over `struct`-packed fields) produced for
//! a given message 1, PMK, addresses and SNonce.

mod common;
use common::*;
use wpa_supplicant_core::frame::{self, info, off};
use wpa_supplicant_core::rsne::{self, Rsne};
use wpa_supplicant_core::{Akm, Cipher, Config, Entropy, Reply, Supplicant};

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

#[test]
fn layout_constants_are_the_ieee80211_literals() {
    // struct ieee80211_eapol_key, sys/net80211/ieee80211.h:1122-1167, summed by hand.
    assert_eq!(frame::HDR_LEN, 4);
    assert_eq!(frame::FIXED_LEN, 1 + 1 + 2 + 1 + 2 + 2 + 8 + 32 + 16 + 8 + 8 + 16 + 2);
    assert_eq!(frame::FIXED_LEN, 99);
    assert_eq!(off::VERSION, 0);
    assert_eq!(off::TYPE, 1);
    assert_eq!(off::BODY_LEN, 2);
    assert_eq!(off::DESC, 4);
    assert_eq!(off::INFO, 5);
    assert_eq!(off::KEY_LEN, 7);
    assert_eq!(off::REPLAY, 9);
    assert_eq!(off::NONCE, 17);
    assert_eq!(off::IV, 49);
    assert_eq!(off::RSC, 65);
    assert_eq!(off::RESERVED, 73);
    assert_eq!(off::MIC, 81);
    assert_eq!(off::DATA_LEN, 97);
    assert_eq!(off::DATA, 99);
    assert_eq!(frame::EAPOL_KEY, 3);
    assert_eq!(frame::DESC_IEEE80211, 2);
    assert_eq!((frame::NONCE_LEN, frame::IV_LEN, frame::MIC_LEN), (32, 16, 16));
}

#[test]
fn key_information_bits_are_the_ieee80211_literals() {
    assert_eq!(info::VERSION_MASK, 0x0007);
    assert_eq!(info::PAIRWISE, 0x0008);
    assert_eq!(info::INSTALL, 0x0040);
    assert_eq!(info::ACK, 0x0080);
    assert_eq!(info::MIC, 0x0100);
    assert_eq!(info::SECURE, 0x0200);
    assert_eq!(info::ERROR, 0x0400);
    assert_eq!(info::REQUEST, 0x0800);
    assert_eq!(info::ENCRYPTED, 0x1000);
    assert_eq!(info::SMK, 0x2000);
}

#[test]
fn rsne_bytes_for_wpa2_psk_ccmp() {
    let mut b = [0u8; 64];
    let n = rsne::build(&mut b, Cipher::Ccmp, Akm::Psk).unwrap();
    assert_eq!(&b[..n], &hex("30140100000fac040100000fac040100000fac020000")[..]);
    let n = rsne::build(&mut b, Cipher::Tkip, Akm::PskSha256).unwrap();
    assert_eq!(&b[..n], &hex("30140100000fac020100000fac040100000fac060000")[..]);
    let n = rsne::build(&mut b, Cipher::Ccmp, Akm::Sae).unwrap();
    assert_eq!(&b[..n], &hex("30140100000fac040100000fac040100000fac080000")[..]);
    assert!(matches!(rsne::build(&mut b[..21], Cipher::Ccmp, Akm::Psk), Err(wpa_supplicant_core::Refusal::TxBufferTooSmall { need: 22, have: 21 })));
}

#[test]
fn rsne_parse() {
    let ie = hex("30140100000fac040100000fac040100000fac020000");
    let r = Rsne::parse(&ie).unwrap();
    assert!(r.offers_akm(Akm::Psk) && !r.offers_akm(Akm::Sae) && !r.offers_akm(Akm::PskSha256));
    assert!(r.offers_pairwise(Cipher::Ccmp) && !r.offers_pairwise(Cipher::Tkip));
    assert!(r.group_is(Cipher::Ccmp) && !r.group_is(Cipher::Tkip));
    assert_eq!((r.pairwise_count(), r.akm_count(), r.caps), (1, 1, 0));
    // A mixed-mode AP: TKIP+CCMP pairwise, TKIP group, PSK and 802.1X, with capabilities.
    let mixed = hex("301c0100000fac020200000fac02000fac040200000fac01000fac02ac00");
    let m = Rsne::parse(&mixed).unwrap();
    assert!(m.offers_pairwise(Cipher::Ccmp) && m.offers_pairwise(Cipher::Tkip));
    assert!(m.group_is(Cipher::Tkip));
    assert!(m.offers_akm(Akm::Psk));
    assert_eq!((m.pairwise_count(), m.akm_count(), m.caps), (2, 2, 0x00ac));
    // Malformed forms.
    for bad in [
        &ie[..21],                                       // length byte disagrees
        &hex("3100")[..],                                // too short for a body
        &hex("2f140100000fac040100000fac040100000fac020000")[..], // wrong element id
        &hex("30140200000fac040100000fac040100000fac020000")[..], // version 2
        &hex("30140100000fac04ff00000fac040100000fac020000")[..], // pairwise count overruns
    ] {
        assert!(Rsne::parse(bad).is_err(), "{bad:02x?}");
    }
}

struct Fixed;
impl Entropy for Fixed {
    fn fill(&mut self, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = 0x80 + i as u8;
        }
    }
}

/// Message 1 and the message 2 expected for it were produced by a
/// from-scratch Python model of 802.11-2020 12.7.6 using hashlib / hmac and struct.pack.
#[test]
fn message_2_matches_an_independent_model_byte_for_byte() {
    let m1 = hex(
        "0103005f02008a00000000000000000001404142434445464748494a4b4c4d4e4f505152535455565758595a5b5c5d5e5f\
         0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
    );
    let want_m2 = hex(
        "0103007502010a00000000000000000001808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f\
         000000000000000000000000000000000000000000000000000000000000000006163cc0ec995d8e2db752924e97137000\
         1630140100000fac040100000fac040100000fac020000",
    );
    let rsne = hex("30140100000fac040100000fac040100000fac020000");
    let cfg = Config { aa: AA, spa: SPA, akm: Akm::Psk, group: Cipher::Ccmp, own_rsne: &rsne, ap_rsne: &rsne };
    let mut s = Supplicant::new(&cfg, pmk(), 0).unwrap();
    let mut tx = [0u8; 512];
    let r = s.on_eapol(&m1, &mut tx, 1, &mut Fixed, &mut Rec::default()).unwrap();
    let Reply::Send(n) = r else { panic!() };
    assert_eq!(hex_of(&tx[..n]), hex_of(&want_m2));
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[test]
fn builder_and_parser_roundtrip_every_field() {
    let mut out = [0u8; 200];
    let nonce = [0x33u8; 32];
    let n = frame::build(
        &mut out,
        &frame::Build { eapol_version: 2, info: 0x13ca, key_len: 16, replay: 0x0102030405060708, nonce: &nonce, key_data: &[1, 2, 3] },
    )
    .unwrap();
    assert_eq!(n, 102);
    let f = frame::KeyFrame::parse(&out[..n]).unwrap();
    assert_eq!(f.eapol_version(), 2);
    assert_eq!(f.info(), 0x13ca);
    assert_eq!(f.descriptor_version(), 2);
    assert_eq!(f.key_len(), 16);
    assert_eq!(f.replay(), 0x0102030405060708);
    assert_eq!(f.nonce(), nonce);
    assert_eq!(f.key_data(), &[1, 2, 3]);
    assert_eq!(f.rsc(), 0);
    assert_eq!(&out[2..4], &(98u16).to_be_bytes()); // body length excludes the 4-byte header
    // The RSC is read as six little-endian bytes.
    out[65..71].copy_from_slice(&[1, 2, 3, 4, 5, 6]);
    assert_eq!(frame::KeyFrame::parse(&out[..n]).unwrap().rsc(), 0x0605_0403_0201);
    out[71] = 0xff; // bytes 6 and 7 of the 8-byte RSC field are not part of the PN
    assert_eq!(frame::KeyFrame::parse(&out[..n]).unwrap().rsc(), 0x0605_0403_0201);
}
