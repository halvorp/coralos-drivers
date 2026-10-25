// SPDX-License-Identifier: ISC
//! Every bad frame is refused by name, leaves the keys alone, and is fatal only where it should be.

mod common;
use common::*;
use wpa_crypto_core::keywrap::WrapRefusal;
use wpa_supplicant_core::{Akm, Refusal, Severity, State};

fn fresh() -> (Auth, wpa_supplicant_core::Supplicant, Seq, Rec) {
    let a = Auth::new(Akm::Psk);
    let s = a.supplicant(0);
    (a, s, Seq::default(), Rec::default())
}

fn after_m2() -> (Auth, wpa_supplicant_core::Supplicant, Seq, Rec) {
    let (mut a, mut s, mut r, mut i) = fresh();
    through_m2(&mut a, &mut s, &mut r, &mut i);
    (a, s, r, i)
}

/// A refused frame sends nothing, installs nothing and (when droppable) leaves the state alone.
fn refused(
    s: &mut wpa_supplicant_core::Supplicant,
    rx: &[u8],
    r: &mut Seq,
    i: &mut Rec,
    want: Refusal,
) {
    let before = s.state();
    let e = send(s, rx, 20, r, i).unwrap_err();
    assert_eq!(e, want);
    assert!(i.log.is_empty(), "a refused frame installed {:?}", i.log);
    if e.severity() == Severity::Drop {
        assert_eq!(s.state(), before, "a droppable refusal must not move the state");
    } else {
        assert_eq!(s.state(), State::Failed);
    }
}

#[test]
fn truncated_and_wrong_framing() {
    let (mut a, mut s, mut r, mut i) = fresh();
    let m1 = a.m1();
    refused(&mut s, &m1[..98], &mut r, &mut i, Refusal::Truncated { got: 98, need: 99 });
    refused(&mut s, &[], &mut r, &mut i, Refusal::Truncated { got: 0, need: 99 });
    let mut f = m1.clone();
    f[1] = 0; // EAP packet, not EAPOL-Key
    refused(&mut s, &f, &mut r, &mut i, Refusal::NotEapolKey(0));
    let mut f = m1.clone();
    f[4] = 254; // the WPA1 descriptor
    refused(&mut s, &f, &mut r, &mut i, Refusal::NotRsnDescriptor(254));
    let mut f = m1.clone();
    f[2..4].copy_from_slice(&200u16.to_be_bytes()); // body longer than the frame
    refused(&mut s, &f, &mut r, &mut i, Refusal::BodyLength { claimed: 204, have: 99 });
    let mut f = m1.clone();
    f[2..4].copy_from_slice(&90u16.to_be_bytes()); // body shorter than the fixed header
    refused(&mut s, &f, &mut r, &mut i, Refusal::BodyLength { claimed: 94, have: 99 });
    let mut f = m1.clone();
    f[97..99].copy_from_slice(&5u16.to_be_bytes()); // key data longer than the body
    refused(&mut s, &f, &mut r, &mut i, Refusal::KeyDataLength { claimed: 5, have: 0 });
}

#[test]
fn descriptor_versions() {
    let (mut a, mut s, mut r, mut i) = fresh();
    let m1 = a.m1();
    for (v, want) in [
        (0u16, Refusal::UnknownDescriptorVersion(0)),
        (4, Refusal::UnknownDescriptorVersion(4)),
        (7, Refusal::UnknownDescriptorVersion(7)),
        (1, Refusal::WrongDescriptorVersion { got: 1, want: 2 }), // HMAC-MD5 / RC4: refused
        (3, Refusal::WrongDescriptorVersion { got: 3, want: 2 }), // CMAC under a SHA-1 AKM
    ] {
        let mut f = m1.clone();
        f[6] = (f[6] & !7) | v as u8;
        refused(&mut s, &f, &mut r, &mut i, want);
    }
    // And the reverse under a SHA-256 AKM.
    let mut a = Auth::new(Akm::Sae);
    let mut s = a.supplicant(0);
    let mut m1 = a.m1();
    m1[6] = (m1[6] & !7) | 2;
    refused(&mut s, &m1, &mut r, &mut i, Refusal::WrongDescriptorVersion { got: 2, want: 3 });
}

#[test]
fn a_request_frame_and_misdirected_messages() {
    let (mut a, mut s, mut r, mut i) = fresh();
    let mut m1 = a.m1();
    m1[5] |= 0x08; // REQUEST
    refused(&mut s, &m1, &mut r, &mut i, Refusal::RequestFrame);

    // Message 2/4 shape (pairwise + MIC, no ACK) is never received by a station.
    let mut f = a.m1();
    f[5] |= 0x01; // MIC bit 0x0100 -> high byte
    f[5] = 0x01;
    f[6] = 0x08 | 2;
    refused(&mut s, &f, &mut r, &mut i, Refusal::Unexpected { state: 0, msg: 2 });
    // Pairwise with neither MIC nor ACK.
    let mut f = a.m1();
    f[6] = 0x08 | 2;
    refused(&mut s, &f, &mut r, &mut i, Refusal::Unexpected { state: 0, msg: 2 });
    // Group frame without ACK.
    let mut f = a.m1();
    f[5] = 0x03; // MIC | SECURE
    f[6] = 2;
    refused(&mut s, &f, &mut r, &mut i, Refusal::Unexpected { state: 0, msg: 6 });
}

#[test]
fn message_3_before_message_1_and_group_before_established() {
    let (mut a, mut s, mut r, mut i) = fresh();
    // Without a message 1 the authenticator has no PTK to protect message 3 with: forge one.
    let mut b = Auth::new(Akm::Psk);
    b.ptk = Some(wpa_crypto_core::kdf::derive_ptk(
        b.kind, &b.pmk, &AA, &SPA, &b.anonce, &[9; 32],
    ));
    let m3 = b.m3();
    refused(&mut s, &m3, &mut r, &mut i, Refusal::Unexpected { state: 0, msg: 3 });
    let g1 = b.g1(&[1; 16], 1);
    refused(&mut s, &g1, &mut r, &mut i, Refusal::Unexpected { state: 0, msg: 5 });
    let _ = &mut a;
}

#[test]
fn replay_counters() {
    let (mut a, mut s, mut r, mut i) = fresh();
    establish(&mut a, &mut s, &mut r, &mut i);
    i.log.clear();
    let last = a.replay;
    // A replayed message 3 (same counter) and an older one are both refused.
    let mut old = a.m3();
    old[9..17].copy_from_slice(&last.to_be_bytes()); // counter == last (MIC now wrong, but replay is checked first)
    refused(&mut s, &old, &mut r, &mut i, Refusal::ReplayCounter { got: last, last });
    let mut older = old.clone();
    older[9..17].copy_from_slice(&(last - 1).to_be_bytes());
    refused(&mut s, &older, &mut r, &mut i, Refusal::ReplayCounter { got: last - 1, last });
    // Likewise for a group message 1 and a message 1.
    let mut g1 = a.g1(&[3; 16], 2);
    g1[9..17].copy_from_slice(&last.to_be_bytes());
    refused(&mut s, &g1, &mut r, &mut i, Refusal::ReplayCounter { got: last, last });
    let mut m1 = a.m1();
    m1[9..17].copy_from_slice(&last.to_be_bytes());
    refused(&mut s, &m1, &mut r, &mut i, Refusal::ReplayCounter { got: last, last });
    assert_eq!(s.state(), State::Established);
}

#[test]
fn a_bad_mic_is_dropped_and_the_handshake_survives() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    let m3 = a.m3();
    let mut bad = m3.clone();
    bad[85] ^= 0x01; // inside the MIC
    refused(&mut s, &bad, &mut r, &mut i, Refusal::MicFailure);
    let mut bad = m3.clone();
    let n = bad.len();
    bad[n - 1] ^= 0x80; // inside the wrapped key data (MIC covers it)
    refused(&mut s, &bad, &mut r, &mut i, Refusal::MicFailure);
    // The genuine message 3 still completes.
    let m4 = send(&mut s, &m3, 21, &mut r, &mut i).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
}

#[test]
fn an_unsupported_mic_version_cannot_slip_through() {
    // Descriptor version 2 AKM, frame says version 2, but a CMAC-protected frame must not verify.
    let (mut a, mut s, mut r, mut i) = after_m2();
    a.version = 3;
    let m3 = a.m3(); // built and MIC'd with CMAC, labelled version 3
    refused(&mut s, &m3, &mut r, &mut i, Refusal::WrongDescriptorVersion { got: 3, want: 2 });
}

#[test]
fn anonce_changed_between_m1_and_m3() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    a.anonce = [0x11; 32];
    let m3 = a.m3();
    refused(&mut s, &m3, &mut r, &mut i, Refusal::AnonceChanged);
}

#[test]
fn rsne_downgrade_is_fatal_with_reason_17() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    // The AP's beacon said CCMP+PSK; message 3 claims a different group cipher.
    let other = rsne_for(Akm::Psk, wpa_supplicant_core::Cipher::Tkip);
    let mut plain = other;
    plain.extend_from_slice(&a.gtk_kde(&[0xc0; 16], 1, true));
    let m3 = a.m3_with(plain, Auth::M3_INFO, 16);
    let e = send(&mut s, &m3, 20, &mut r, &mut i).unwrap_err();
    assert_eq!(e, Refusal::RsneMismatch);
    assert_eq!(e.deauth_reason(), Some(17));
    assert_eq!(s.state(), State::Failed);
    assert!(i.log.is_empty());
}

#[test]
fn a_second_rsne_may_not_change_the_akm_or_offer_two_pairwise_ciphers() {
    for second in [
        rsne_for(Akm::PskSha256, wpa_supplicant_core::Cipher::Ccmp), // different AKM
        rsne_for(Akm::Psk, wpa_supplicant_core::Cipher::Tkip),       // different group cipher
    ] {
        let (mut a, mut s, mut r, mut i) = after_m2();
        let mut plain = a.ap_rsne.clone();
        plain.extend_from_slice(&second);
        plain.extend_from_slice(&a.gtk_kde(&[0xc0; 16], 1, true));
        let m3 = a.m3_with(plain, Auth::M3_INFO, 16);
        let e = send(&mut s, &m3, 20, &mut r, &mut i).unwrap_err();
        assert_eq!(e, Refusal::RsneDowngrade);
        assert_eq!(e.deauth_reason(), Some(19));
        assert_eq!(s.state(), State::Failed);
    }
    // A second RSNE identical to the first is fine.
    let (mut a, mut s, mut r, mut i) = after_m2();
    let mut plain = a.ap_rsne.clone();
    plain.extend_from_slice(&a.ap_rsne.clone());
    plain.extend_from_slice(&a.gtk_kde(&[0xc0; 16], 1, true));
    let m3 = a.m3_with(plain, Auth::M3_INFO, 16);
    assert!(send(&mut s, &m3, 20, &mut r, &mut i).is_ok());
}

#[test]
fn message_3_key_data_requirements() {
    // No RSNE.
    let (mut a, mut s, mut r, mut i) = after_m2();
    let m3 = a.m3_with(a.gtk_kde(&[0xc0; 16], 1, true), Auth::M3_INFO, 16);
    refused(&mut s, &m3, &mut r, &mut i, Refusal::MissingRsne);

    // No Install bit.
    let (mut a, mut s, mut r, mut i) = after_m2();
    let m3 = a.m3_with(a.ap_rsne.clone(), Auth::M3_INFO & !0x0040, 16);
    refused(&mut s, &m3, &mut r, &mut i, Refusal::MissingInstall);

    // Wrong key length field.
    let (mut a, mut s, mut r, mut i) = after_m2();
    let m3 = a.m3_with(a.ap_rsne.clone(), Auth::M3_INFO, 32);
    let e = send(&mut s, &m3, 20, &mut r, &mut i).unwrap_err();
    assert_eq!(e, Refusal::KeyLength { got: 32, want: 16 });
    assert_eq!(e.deauth_reason(), Some(3));
    assert_eq!(s.state(), State::Failed);

    // A GTK of the wrong size for the group cipher.
    let (mut a, mut s, mut r, mut i) = after_m2();
    let mut plain = a.ap_rsne.clone();
    plain.extend_from_slice(&a.gtk_kde(&[0xc0; 32], 1, true));
    let m3 = a.m3_with(plain, Auth::M3_INFO, 16);
    let e = send(&mut s, &m3, 20, &mut r, &mut i).unwrap_err();
    assert_eq!(e, Refusal::GtkLength { got: 32, want: 16 });
    assert_eq!(s.state(), State::Failed);
}

#[test]
fn a_gtk_in_unencrypted_key_data_is_refused() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    let mut plain = a.ap_rsne.clone();
    plain.extend_from_slice(&a.gtk_kde(&[0xc0; 16], 1, true));
    a.replay += 1;
    let kck = a.ptk.as_ref().unwrap().kck;
    let rsc = a.rsc;
    let mut f = a.raw((Auth::M3_INFO & !0x1000) | 2, 16, a.replay, &a.anonce.clone(), &rsc, &plain);
    a.stamp(&mut f, &kck);
    refused(&mut s, &f, &mut r, &mut i, Refusal::GtkNotEncrypted);
}

#[test]
fn key_data_that_does_not_unwrap_is_refused() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    // Genuine MIC over key data that was wrapped under the wrong KEK.
    a.replay += 1;
    let kck = a.ptk.as_ref().unwrap().kck;
    let mut buf = vec![0u8; 8];
    buf.extend_from_slice(&a.ap_rsne.clone());
    buf.extend_from_slice(&[0xdd, 0]); // pad to a multiple of 8
    while buf.len() % 8 != 0 {
        buf.push(0);
    }
    wpa_crypto_core::keywrap::wrap_in_place(&[0x66; 16], &mut buf).unwrap();
    let rsc = a.rsc;
    let mut f = a.raw(Auth::M3_INFO | 2, 16, a.replay, &a.anonce.clone(), &rsc, &buf);
    a.stamp(&mut f, &kck);
    refused(&mut s, &f, &mut r, &mut i, Refusal::KeyDataDecrypt(WrapRefusal::Integrity));

    // And key data too short to be a wrapped blob.
    a.replay += 1;
    let mut f = a.raw(Auth::M3_INFO | 2, 16, a.replay, &a.anonce.clone(), &rsc, &[0u8; 16]);
    a.stamp(&mut f, &kck);
    refused(&mut s, &f, &mut r, &mut i, Refusal::KeyDataDecrypt(WrapRefusal::Length(16)));
}

#[test]
fn oversized_key_data_is_refused_by_name() {
    let (mut a, mut s, mut r, mut i) = after_m2();
    a.replay += 1;
    let kck = a.ptk.as_ref().unwrap().kck;
    let rsc = a.rsc;
    let big = vec![0u8; 600];
    let mut f = a.raw(Auth::M3_INFO | 2, 16, a.replay, &a.anonce.clone(), &rsc, &big);
    a.stamp(&mut f, &kck);
    refused(&mut s, &f, &mut r, &mut i, Refusal::KeyDataTooLong { len: 600, max: 512 });
}

#[test]
fn a_malformed_pmkid_kde_in_message_1_is_refused() {
    let (mut a, mut s, mut r, mut i) = fresh();
    let kde = [vec![0xdd, 4 + 8, 0x00, 0x0f, 0xac, 0x04], vec![7u8; 8]].concat();
    let m1 = a.m1_with_data(&kde);
    refused(&mut s, &m1, &mut r, &mut i, Refusal::PmkidKde { len: 8 });
    // A well-formed one is accepted.
    let kde = [vec![0xdd, 4 + 16, 0x00, 0x0f, 0xac, 0x04], vec![7u8; 16]].concat();
    let m1 = a.m1_with_data(&kde);
    assert!(send(&mut s, &m1, 21, &mut r, &mut i).is_ok());
}

#[test]
fn group_message_requirements() {
    let (mut a, mut s, mut r, mut i) = fresh();
    establish(&mut a, &mut s, &mut r, &mut i);
    i.log.clear();

    // Not encrypted.
    let kde = a.gtk_kde(&[5; 16], 2, true);
    let g1 = a.g1_with(kde, false);
    refused(&mut s, &g1, &mut r, &mut i, Refusal::GtkMissing);
    // Encrypted but no GTK KDE.
    let g1 = a.g1_with(a.ap_rsne.clone(), true);
    refused(&mut s, &g1, &mut r, &mut i, Refusal::GtkMissing);
    // Bad MIC.
    let mut g1 = a.g1(&[5; 16], 2);
    g1[90] ^= 1;
    refused(&mut s, &g1, &mut r, &mut i, Refusal::MicFailure);
    // Wrong GTK size is fatal.
    let kde = a.gtk_kde(&[5; 32], 2, true);
    let g1 = a.g1_with(kde, true);
    let e = send(&mut s, &g1, 30, &mut r, &mut i).unwrap_err();
    assert_eq!(e, Refusal::GtkLength { got: 32, want: 16 });
    assert_eq!(s.state(), State::Failed);
}

#[test]
fn a_refused_group_install_is_fatal() {
    let (mut a, mut s, mut r, mut i) = fresh();
    establish(&mut a, &mut s, &mut r, &mut i);
    i.refuse_gtk = true;
    let g1 = a.g1(&[7; 16], 2);
    let e = send(&mut s, &g1, 30, &mut r, &mut i).unwrap_err();
    assert_eq!(e, Refusal::InstallFailed);
    assert_eq!(s.state(), State::Failed);
}

#[test]
fn a_small_transmit_buffer_is_refused_not_overrun() {
    let (mut a, mut s, mut r, mut i) = fresh();
    let m1 = a.m1();
    let mut tx = [0u8; 100];
    let e = s.on_eapol(&m1, &mut tx, 1, &mut r, &mut i).unwrap_err();
    assert!(matches!(e, Refusal::TxBufferTooSmall { need: 121, have: 100 }), "{e:?}");
}

#[test]
fn severity_and_reason_table() {
    use Refusal::*;
    let fatal = [
        (TooManyMessage1 { answered: 5 }, 15),
        (Timeout { waited_ms: 1, limit_ms: 0 }, 15),
        (RsneMismatch, 17),
        (RsneDowngrade, 19),
        (KeyLength { got: 1, want: 2 }, 3),
        (GtkLength { got: 1, want: 2 }, 3),
        (InstallFailed, 3),
    ];
    for (r, reason) in fatal {
        assert_eq!(r.severity(), Severity::Fatal, "{r:?}");
        assert_eq!(r.deauth_reason(), Some(reason), "{r:?}");
    }
    for r in [MicFailure, ReplayCounter { got: 0, last: 0 }, MissingRsne, AnonceChanged, RequestFrame] {
        assert_eq!(r.severity(), Severity::Drop, "{r:?}");
        assert_eq!(r.deauth_reason(), None, "{r:?}");
    }
}

#[test]
fn config_must_be_consistent() {
    use wpa_supplicant_core::{Cipher, Config, Supplicant};
    let a = Auth::new(Akm::Psk);
    let ok = a.ap_rsne.clone();
    let wrong_akm = rsne_for(Akm::Sae, Cipher::Ccmp);
    let mk = |own: &[u8], ap: &[u8], akm| {
        let c = Config { aa: AA, spa: SPA, akm, group: Cipher::Ccmp, own_rsne: own, ap_rsne: ap };
        Supplicant::new(&c, pmk(), 0).err()
    };
    assert_eq!(mk(&ok, &ok, Akm::Psk), None);
    assert_eq!(mk(&wrong_akm, &ok, Akm::Psk), Some(Refusal::BadConfig));
    assert_eq!(mk(&ok, &wrong_akm, Akm::Psk), Some(Refusal::BadConfig));
    assert_eq!(mk(&ok[..10], &ok, Akm::Psk), Some(Refusal::BadConfig)); // length byte disagrees
    assert_eq!(mk(&[], &ok, Akm::Psk), Some(Refusal::BadConfig));
    let mut long = vec![0x30, 255];
    long.extend_from_slice(&[0u8; 255]);
    long.push(0);
    assert_eq!(mk(&long, &ok, Akm::Psk), Some(Refusal::BadConfig));
}
