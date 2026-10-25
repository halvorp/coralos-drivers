// SPDX-License-Identifier: GPL-2.0-only
//! Every way a join or a link can fail, against the simulated card and access point: each ends in a NAMED refusal, leaves
//! no firmware context behind, and the card records no violation.
mod sim;
use iwlwifi_device::driver::Driver;
use iwlwifi_device::event::{Event, JoinRequest, LinkDown, Request, Security};
use iwlwifi_device::link::{JoinError, JoinWhy};
use iwlwifi_device::sha256::sha256;
use iwlwifi_device::transport::TransportError;
use sim::ap::{self, JoinAp};
use sim::blob::{build, Opts};
use sim::harness::*;
use sim::Sim;
use wpa_supplicant_core::Refusal;

fn fails(d: &mut Driver<sim::SimHost>, r: &JoinRequest) -> JoinError {
    d.join(r).expect_err("the join must fail")
}

#[test]
fn authentication_refused_leaves_nothing_behind() {
    let (sim, mut d) = ready();
    let mut a = JoinAp::new("CoralNet", 0x01, 6, None);
    a.auth_status = 1;
    put_ap(&sim, a);
    let e = fails(&mut d, &req("CoralNet", Security::Open, None));
    assert_eq!((e.stage, e.why.clone()), ("auth", JoinWhy::AuthRefused(1)));
    assert_eq!(e.code(), 1);
    assert!(d.link().is_none());
    contexts_gone(&sim);
    no_violations(&sim);
}

#[test]
fn association_refused_names_the_status() {
    let (sim, mut d) = ready();
    let mut a = JoinAp::new("CoralNet", 0x01, 6, None);
    a.assoc_status = 18; // unsupported rates
    put_ap(&sim, a);
    let e = fails(&mut d, &req("CoralNet", Security::Open, None));
    assert_eq!((e.stage, e.why.clone()), ("assoc", JoinWhy::AssocRefused(18)));
    contexts_gone(&sim);
    assert_eq!(sim.w().fw.rt.join.deauths, Vec::<u16>::new());
    no_violations(&sim);
}

#[test]
fn a_lost_request_is_retried() {
    let (sim, mut d) = ready();
    let mut a = JoinAp::new("CoralNet", 0x01, 6, None);
    a.ignore_auth = 2;
    a.ignore_assoc = 1;
    put_ap(&sim, a);
    d.join(&req("CoralNet", Security::Open, None)).expect("join after retries");
    let w = sim.w();
    assert_eq!((w.fw.rt.join.auth_seen, w.fw.rt.join.assoc_seen), (3, 2));
    assert_eq!(w.log.iter().filter(|l| l.contains("[IWL-AUTH retry]")).count(), 2);
    assert!(w.violations.is_empty(), "{:?}", w.violations);
}

#[test]
fn a_silent_access_point_ends_in_a_named_timeout_after_three_tries() {
    let (sim, mut d) = ready();
    let mut a = JoinAp::new("CoralNet", 0x01, 6, None);
    a.ignore_auth = 99;
    put_ap(&sim, a);
    let e = fails(&mut d, &req("CoralNet", Security::Open, None));
    assert_eq!((e.stage, e.why.clone()), ("auth", JoinWhy::Timeout("authentication response")));
    assert_eq!(sim.w().fw.rt.join.auth_seen, 3);
    contexts_gone(&sim);
    no_violations(&sim);

    let mut a = JoinAp::new("CoralNet", 0x01, 6, None);
    a.ignore_assoc = 99;
    put_ap(&sim, a);
    sim.w().fw.rt.join.auth_seen = 0;
    sim.w().fw.rt.join.assoc_seen = 0;
    let e = fails(&mut d, &req("CoralNet", Security::Open, None));
    assert_eq!((e.stage, e.why.clone()), ("assoc", JoinWhy::Timeout("association response")));
    assert_eq!(sim.w().fw.rt.join.assoc_seen, 3);
    contexts_gone(&sim);
}

#[test]
fn a_wrong_passphrase_times_out_the_handshake_and_deauthenticates() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(pmk("correct horse", "Home"))));
    let e = fails(&mut d, &req("Home", Security::Wpa2Psk, Some(pmk("wrong horse", "Home"))));
    assert_eq!(e.stage, "4way");
    assert!(matches!(e.why, JoinWhy::Handshake(Refusal::Timeout { .. })), "{:?}", e.why);
    assert_eq!(e.code(), 15);
    assert!(sim.w().fw.rt.join.log.iter().any(|l| l.contains("M2 refused")), "the AP saw a bad MIC");
    assert_eq!(sim.w().fw.rt.join.deauths, vec![15]);
    contexts_gone(&sim);
    no_violations(&sim);
}

#[test]
fn an_rsne_that_differs_from_the_beacon_aborts_with_reason_17() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    {
        let mut w = sim.w();
        let a = w.fw.rt.join.auth.as_mut().unwrap();
        a.ap_rsne = wpa_supplicant_core::testkit::rsne_for(wpa_supplicant_core::Akm::Psk, wpa_supplicant_core::Cipher::Tkip);
    }
    let e = fails(&mut d, &req("Home", Security::Wpa2Psk, Some(k)));
    assert!(matches!(e.why, JoinWhy::Handshake(Refusal::RsneMismatch)), "{:?}", e.why);
    assert_eq!(sim.w().fw.rt.join.deauths, vec![17]);
    assert!(sim.w().fw.rt.join.keys.iter().all(|k| k.is_none()), "no key may be installed for a refused message 3");
    contexts_gone(&sim);
    // the link never came up, so there is no LinkDown to report: the failure is the join's
    assert!(!d.take_events().iter().any(|e| matches!(e, Event::LinkDown { .. })));
    no_violations(&sim);
}

#[test]
fn no_key_is_installed_when_message_4_never_leaves() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    let mut a = JoinAp::new("Home", 0x02, 11, Some(k));
    a.fail_tx_from = Some(3); // auth, assoc, M2 go; M4 is lost on the air
    put_ap(&sim, a);
    let e = fails(&mut d, &req("Home", Security::Wpa2Psk, Some(k)));
    assert!(matches!(e.why, JoinWhy::Handshake(Refusal::Timeout { .. })), "{:?}", e.why);
    assert!(!log_has(&sim, "[IWL-KEYS"), "the PTK must not be installed before message 4 has been transmitted");
    contexts_gone(&sim);
}

#[test]
fn a_retransmitted_message_3_is_answered_and_installs_nothing() {
    // The key reinstallation attack, end to end: the AP resends message 3, the station sends message 4 again, and no key
    // is installed a second time.
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    d.take_events();
    let installs = |sim: &Sim| sim.w().log.iter().filter(|l| l.contains("[IWL-KEYS")).count();
    assert_eq!(installs(&sim), 2);
    let frames_before = sim.w().fw.rt.join.frames.len();
    {
        let mut w = sim.w();
        ap::resend_m3(&mut w, 100);
    }
    d.link_poll(50).expect("poll");
    assert_eq!(installs(&sim), 2, "a replayed message 3 must not reinstall any key");
    assert_eq!(sim.w().fw.rt.join.frames.len(), frames_before + 1, "message 4 is sent again");
    assert!(d.link().unwrap().up);
    no_violations(&sim);
}

#[test]
fn a_deauthentication_from_the_access_point_ends_the_link() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    d.take_events();
    {
        let mut w = sim.w();
        ap::ap_drop(&mut w, 100, 7, false);
    }
    let ev = d.link_poll(50).expect("poll");
    assert_eq!(ev, vec![Event::LinkDown { reason: LinkDown::Deauthenticated(7) }]);
    assert!(d.link().is_none());
    contexts_gone(&sim);

    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("CoralNet", 0x01, 6, None));
    d.join(&req("CoralNet", Security::Open, None)).expect("join");
    d.take_events();
    {
        let mut w = sim.w();
        ap::ap_drop(&mut w, 100, 8, true);
    }
    assert_eq!(d.link_poll(50).expect("poll"), vec![Event::LinkDown { reason: LinkDown::Disassociated(8) }]);
    contexts_gone(&sim);
    no_violations(&sim);
}

#[test]
fn requests_that_cannot_be_served_are_refused_by_name() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(pmk("passphrase1", "Home"))));
    // not in the scan
    let e = fails(&mut d, &req("Nowhere", Security::Open, None));
    assert_eq!((e.stage, e.why), ("request", JoinWhy::NoScanResult));
    // a different BSSID than the one heard
    let mut r = req("Home", Security::Wpa2Psk, Some(pmk("passphrase1", "Home")));
    r.bssid = Some([1, 2, 3, 4, 5, 6]);
    assert_eq!(fails(&mut d, &r).why, JoinWhy::NoScanResult);
    // security and credential disagree
    assert_eq!(fails(&mut d, &req("Home", Security::Wpa2Psk, None)).why, JoinWhy::Unsupported("security and credential disagree"));
    assert_eq!(fails(&mut d, &req("Home", Security::Open, Some([0; 32]))).why, JoinWhy::Unsupported("security and credential disagree"));
    // an open request for a protected network
    assert_eq!(fails(&mut d, &req("Home", Security::Open, None)).why, JoinWhy::Unsupported("network is protected"));
    // networks whose RSN element this station cannot use
    assert_eq!(fails(&mut d, &req("Sae", Security::Wpa2Psk, Some([0; 32]))).why, JoinWhy::Unsupported("AP does not offer WPA2-PSK with CCMP"));
    assert_eq!(fails(&mut d, &req("Tkip", Security::Wpa2Psk, Some([0; 32]))).why, JoinWhy::Unsupported("AP does not offer WPA2-PSK with CCMP"));
    // nothing was sent to the firmware for any of them
    assert!(sim.w().fw.rt.join.frames.is_empty());
    contexts_gone(&sim);
    assert!(d.link().is_none());
    no_violations(&sim);
}

#[test]
fn no_entropy_no_handshake() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    sim.w().rng_refuse = true;
    let e = fails(&mut d, &req("Home", Security::Wpa2Psk, Some(k)));
    assert_eq!((e.stage, e.why), ("request", JoinWhy::NoEntropy));
    contexts_gone(&sim);
    assert!(sim.w().fw.rt.join.frames.is_empty(), "nothing is sent before the entropy check");
}

fn snonce_of_m2(sim: &Sim, from: usize) -> Vec<u8> {
    let w = sim.w();
    // the M2 is the data frame (To-DS data, 0x08) whose EAPOL key information says PAIRWISE | MIC and nothing else
    w.fw.rt.join.frames[from..].iter().find(|f| f.hdr[0] == 0x08 && f.body.len() > 8 + 99).map(|f| f.body[8 + 4 + 13..8 + 4 + 13 + 32].to_vec()).expect("an M2 frame")
}

#[test]
fn the_snonce_comes_from_the_host_entropy_and_differs_per_join() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    assert!(sim.w().rng_calls >= 2, "one probe and one SNonce");
    let s1 = snonce_of_m2(&sim, 0);
    assert_ne!(s1, vec![0u8; 32]);
    d.leave();
    let n = sim.w().fw.rt.join.frames.len();
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("second join");
    let s2 = snonce_of_m2(&sim, n);
    assert_ne!(s1, s2);
}

#[test]
fn the_transmit_queue_is_bounded_and_drains() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("CoralNet", 0x01, 6, None));
    d.join(&req("CoralNet", Security::Open, None)).expect("join");
    d.take_events();
    let mut eth = vec![0xffu8; 6];
    eth.extend_from_slice(&STA);
    eth.extend_from_slice(&0x0806u16.to_be_bytes());
    eth.extend_from_slice(&[0u8; 28]);
    let mut sent = 0;
    let err = loop {
        match d.link_send(&eth) {
            Ok(()) => sent += 1,
            Err(e) => break e,
        }
        assert!(sent < 100, "the queue never filled");
    };
    assert_eq!(err.why, JoinWhy::Transport(TransportError::QueueFull));
    assert!((29..=33).contains(&sent), "{sent} frames fit");
    d.link_poll(20).expect("poll"); // the TX responses reclaim the ring
    d.link_send(&eth).expect("room again");
    no_violations(&sim);
}

#[test]
fn bad_frames_to_send_are_refused() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("CoralNet", 0x01, 6, None));
    assert_eq!(d.link_send(&[0u8; 20]).unwrap_err().why, JoinWhy::NoLink);
    d.join(&req("CoralNet", Security::Open, None)).expect("join");
    assert_eq!(d.link_send(&[0u8; 10]).unwrap_err().why, JoinWhy::BadFrame("shorter than an Ethernet header"));
    assert_eq!(d.link_send(&[0u8; 20]).unwrap_err().why, JoinWhy::BadFrame("source is not this station"));
    let mut big = vec![0xffu8; 6];
    big.extend_from_slice(&STA);
    big.extend_from_slice(&[8, 0]);
    big.extend_from_slice(&vec![0u8; 2100]);
    assert!(matches!(d.link_send(&big).unwrap_err().why, JoinWhy::Transport(TransportError::TxFrameTooLong { .. })));
    // joining while joined
    assert_eq!(d.join(&req("CoralNet", Security::Open, None)).unwrap_err().why, JoinWhy::Busy);
    assert!(d.link().is_some(), "a refused second join leaves the first link alone");
}

#[test]
fn the_request_interface_turns_a_join_into_events() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("CoralNet", 0x01, 6, None));
    let ev = d.handle(Request::Join(req("CoralNet", Security::Open, None))).unwrap();
    assert!(matches!(ev.as_slice(), [Event::LinkUp { channel: 6, aid: 3, .. }]), "{ev:?}");
    let ev = d.handle(Request::Leave).unwrap();
    assert_eq!(ev, vec![Event::LinkDown { reason: LinkDown::Left }]);
    let ev = d.handle(Request::Join(req("Missing", Security::Open, None))).unwrap();
    assert_eq!(ev, vec![Event::JoinFailed { stage: "request", code: 0 }]);
    // the PMK never appears in a debug print of the request
    let s = format!("{:?}", req("Home", Security::Wpa2Psk, Some([0xab; 32])));
    assert!(!s.contains("171") && !s.contains("ab"), "{s}");
    assert!(s.contains("<present>"));
    no_violations(&sim);
}

#[test]
fn a_join_before_the_rt_image_is_up_is_refused() {
    let b = build(&Opts { rt: true, ..Opts::default() });
    let sim = Sim::new();
    {
        let mut w = sim.w();
        w.fw.init = Some(b.init.clone());
        w.fw.regular = Some(b.regular.clone());
    }
    let mut d = Driver::new(sim.host(), b.bytes.clone(), &sha256(&b.bytes)).expect("blob");
    d.bring_up().expect("INIT");
    assert_eq!(fails(&mut d, &req("x", Security::Open, None)).why, JoinWhy::NotUp);
}
