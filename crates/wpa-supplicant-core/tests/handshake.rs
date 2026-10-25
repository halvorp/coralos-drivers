// SPDX-License-Identifier: ISC
//! The full exchange between the supplicant and a test authenticator, under each AKM, plus the
//! rekey cases.

mod common;
use common::*;
use wpa_supplicant_core::{Akm, Reply, Refusal, State};

fn happy(akm: Akm) {
    let mut a = Auth::new(akm);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    assert_eq!(s.state(), State::AwaitM1);

    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    assert_eq!(s.state(), State::AwaitM3);
    assert!(inst.log.is_empty(), "no key may be installed before message 3 is verified");

    let m3 = a.m3();
    let m4 = send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
    assert_eq!(s.state(), State::M4Sent);
    assert!(inst.log.is_empty(), "no key may be installed before message 4 has left the air");
    assert!(!s.port_open());

    s.tx_done(&mut inst).unwrap();
    assert_eq!(s.state(), State::Established);
    assert!(s.port_open());
    let ptk = a.ptk.as_ref().unwrap();
    assert_eq!(
        inst.log,
        vec![
            // A GTK rides along, so the pairwise RSC is 0 (ieee80211_pae_input.c:569).
            Ev::Ptk { tk: ptk.tk.to_vec(), rsc: 0 },
            Ev::Gtk { id: 1, key: vec![0xc0; 16], tx: true, rsc: 9 },
            Ev::Open,
        ]
    );
}

#[test]
fn four_way_wpa2_psk_descriptor_v2() {
    happy(Akm::Psk);
}

#[test]
fn four_way_psk_sha256_descriptor_v3() {
    happy(Akm::PskSha256);
}

#[test]
fn four_way_after_sae_descriptor_v3() {
    happy(Akm::Sae);
}

#[test]
fn pairwise_rsc_is_used_when_no_gtk_rides_along() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    a.rsc = [7, 0, 0, 0, 0, 0];
    let m3 = a.m3_with(a.ap_rsne.clone(), Auth::M3_INFO, 16);
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    s.tx_done(&mut inst).unwrap();
    assert_eq!(inst.log[0], Ev::Ptk { tk: a.ptk.as_ref().unwrap().tk.to_vec(), rsc: 7 });
    assert_eq!(inst.log.len(), 2); // PTK, Open
}

#[test]
fn group_key_handshake_installs_only_a_changed_key() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    establish(&mut a, &mut s, &mut rng, &mut inst);
    inst.log.clear();

    // A new GTK in slot 2.
    let g1 = a.g1(&[0xd1; 16], 2);
    let g2 = send(&mut s, &g1, 30, &mut rng, &mut inst).unwrap();
    a.check_g2(&g2, a.replay).unwrap();
    assert_eq!(inst.log, vec![Ev::Gtk { id: 2, key: vec![0xd1; 16], tx: true, rsc: 9 }]);

    // The same key again (an AP retransmission with a fresh counter): answered, not reinstalled.
    let g1 = a.g1(&[0xd1; 16], 2);
    let g2 = send(&mut s, &g1, 31, &mut rng, &mut inst).unwrap();
    a.check_g2(&g2, a.replay).unwrap();
    assert_eq!(inst.log.len(), 1);

    // The same bytes under another key id are a different key and are installed.
    let g1 = a.g1(&[0xd1; 16], 3);
    send(&mut s, &g1, 32, &mut rng, &mut inst).unwrap();
    assert_eq!(inst.log.len(), 2);
}

#[test]
fn ptk_rekey_installs_the_new_ptk_and_keeps_the_port_and_gtk() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    establish(&mut a, &mut s, &mut rng, &mut inst);
    let old_tk = a.ptk.as_ref().unwrap().tk;
    inst.log.clear();

    a.anonce = [0x6b; 32];
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    assert_eq!(s.state(), State::AwaitM3);
    assert!(s.port_open(), "a rekey does not close the port");
    let m3 = a.m3();
    let m4 = send(&mut s, &m3, 40, &mut rng, &mut inst).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
    s.tx_done(&mut inst).unwrap();
    assert_eq!(inst.log.len(), 1, "the GTK did not change and the port is already open: {:?}", inst.log);
    match &inst.log[0] {
        Ev::Ptk { tk, .. } => assert_ne!(tk[..], old_tk[..]),
        e => panic!("{e:?}"),
    }
}

#[test]
fn retransmitted_m3_after_established_does_not_reinstall_the_ptk() {
    // The key reinstallation attack (KRACK): a replayed message 3 must be answered, never
    // allowed to reset the packet-number state by installing the same key again.
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    establish(&mut a, &mut s, &mut rng, &mut inst);
    let installed = inst.log.len();
    assert_eq!(installed, 3);

    let m3 = a.m3(); // the AP retransmits with the next counter
    let m4 = send(&mut s, &m3, 50, &mut rng, &mut inst).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
    assert_eq!(s.state(), State::Established, "no new transition for a retransmission");
    s.tx_done(&mut inst).unwrap();
    assert_eq!(inst.log.len(), installed, "no key may be reinstalled");
}

#[test]
fn retransmitted_m3_before_tx_done_is_answered_and_installs_once() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let m3 = a.m3();
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    let m3b = a.m3();
    let m4 = send(&mut s, &m3b, 21, &mut rng, &mut inst).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
    s.tx_done(&mut inst).unwrap();
    let ptks = inst.log.iter().filter(|e| matches!(e, Ev::Ptk { .. })).count();
    assert_eq!(ptks, 1);
}

#[test]
fn a_failed_m4_transmission_waits_for_the_ap_to_retry() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let m3 = a.m3();
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    s.tx_failed();
    assert_eq!(s.state(), State::AwaitM3);
    s.tx_done(&mut inst).unwrap(); // a late tx_done after the failure is a no-op
    assert!(inst.log.is_empty());
    let m3b = a.m3();
    send(&mut s, &m3b, 21, &mut rng, &mut inst).unwrap();
    s.tx_done(&mut inst).unwrap();
    assert!(s.is_established());
}

#[test]
fn same_anonce_reuses_the_snonce_and_a_new_anonce_does_not() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    let m2a = through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let m2b = through_m2(&mut a, &mut s, &mut rng, &mut inst); // retransmitted M1, same ANonce
    assert_eq!(m2a[17..49], m2b[17..49]);
    assert_eq!(rng.fills, 1);
    a.anonce = [0x77; 32];
    let m2c = through_m2(&mut a, &mut s, &mut rng, &mut inst);
    assert_ne!(m2a[17..49], m2c[17..49]);
    assert_eq!(rng.fills, 2);
    // The M3 for the newest M1 completes.
    let m3 = a.m3();
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    s.tx_done(&mut inst).unwrap();
    assert!(s.is_established());
}

#[test]
fn message_one_retries_are_bounded() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    for _ in 0..wpa_supplicant_core::supplicant::MAX_M1_ANSWERS {
        through_m2(&mut a, &mut s, &mut rng, &mut inst);
    }
    let m1 = a.m1();
    let e = send(&mut s, &m1, 11, &mut rng, &mut inst).unwrap_err();
    assert_eq!(e, Refusal::TooManyMessage1 { answered: 5 });
    assert_eq!(e.deauth_reason(), Some(15));
    assert_eq!(s.state(), State::Failed);
    // Once failed, everything is refused.
    let m1 = a.m1();
    assert!(matches!(send(&mut s, &m1, 12, &mut rng, &mut inst), Err(Refusal::Unexpected { .. })));
}

#[test]
fn the_four_way_times_out() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(1_000);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    assert_eq!(s.tick(11_000), Ok(())); // exactly at the limit is still in time
    let e = s.tick(11_001).unwrap_err();
    assert_eq!(e, Refusal::Timeout { waited_ms: 10_001, limit_ms: 10_000 });
    assert_eq!(e.deauth_reason(), Some(15));
    assert_eq!(s.state(), State::Failed);

    // A frame that arrives after the deadline is the timeout too, and installs nothing.
    let mut s = a.supplicant(0);
    let m1 = a.m1();
    let e = send(&mut s, &m1, 20_000, &mut rng, &mut inst).unwrap_err();
    assert!(matches!(e, Refusal::Timeout { .. }));
    assert!(inst.log.is_empty());
}

#[test]
fn an_established_link_does_not_time_out() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    establish(&mut a, &mut s, &mut rng, &mut inst);
    assert_eq!(s.tick(10_000_000), Ok(()));
}

#[test]
fn a_refused_install_fails_the_handshake_and_never_opens_the_port() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec { refuse_ptk: true, ..Default::default() });
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let m3 = a.m3();
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    let e = s.tx_done(&mut inst).unwrap_err();
    assert_eq!(e, Refusal::InstallFailed);
    assert_eq!(e.deauth_reason(), Some(3));
    assert_eq!(s.state(), State::Failed);
    assert!(!s.port_open());
    assert!(inst.log.is_empty());

    // A refused GTK install is just as fatal.
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let mut inst = Rec { refuse_gtk: true, ..Default::default() };
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let m3 = a.m3();
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    assert_eq!(s.tx_done(&mut inst), Err(Refusal::InstallFailed));
    assert!(!s.port_open());
}

#[test]
fn a_late_install_failure_is_reported_by_the_driver() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    establish(&mut a, &mut s, &mut rng, &mut inst);
    assert_eq!(s.fail_install(), Refusal::InstallFailed);
    assert_eq!(s.state(), State::Failed);
}

#[test]
fn ethernet_padding_after_the_pdu_is_ignored() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    let mut m1 = a.m1();
    m1.extend_from_slice(&[0u8; 17]);
    assert!(send(&mut s, &m1, 1, &mut rng, &mut inst).is_ok());
}

#[test]
fn reply_none_is_not_used_for_accepted_frames() {
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    let m1 = a.m1();
    let mut tx = [0u8; 512];
    assert!(matches!(s.on_eapol(&m1, &mut tx, 1, &mut rng, &mut inst), Ok(Reply::Send(_))));
}

#[test]
fn the_port_stays_closed_until_a_message_3_with_the_secure_bit() {
    // ieee80211_pae_input.c:655 opens the port only on EAPOL_KEY_SECURE.
    let mut a = Auth::new(Akm::Psk);
    let mut s = a.supplicant(0);
    let (mut rng, mut inst) = (Seq::default(), Rec::default());
    through_m2(&mut a, &mut s, &mut rng, &mut inst);
    let mut plain = a.ap_rsne.clone();
    plain.extend_from_slice(&a.gtk_kde(&[0xc0; 16], 1, true));
    let m3 = a.m3_with(plain, Auth::M3_INFO & !0x0200, 16);
    send(&mut s, &m3, 20, &mut rng, &mut inst).unwrap();
    s.tx_done(&mut inst).unwrap();
    assert!(!s.port_open());
    assert!(!inst.log.contains(&Ev::Open));
}
