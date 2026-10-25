// SPDX-License-Identifier: GPL-2.0-only
//! The join against the simulated card's strict RT firmware and an access point on the air (tests/sim/ap.rs): the
//! firmware contexts in Linux's order, authentication, association, the 4-way handshake through the supplicant crate,
//! the keys, data both ways and group rekeys. Failures are in join_fail.rs. Each test asserts the card recorded NO violation.
mod sim;
use iwlwifi_device::event::{Event, LinkDown, Security};
use sim::ap::{self, JoinAp};
use sim::harness::*;

#[test]
fn an_open_network_joins_and_leaves() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("CoralNet", 0x01, 6, None));
    let rep = d.join(&req("CoralNet", Security::Open, None)).expect("join");
    assert_eq!((rep.bssid, rep.channel, rep.aid, rep.secure), ([0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x01], 6, 3, false));
    let ev = d.take_events();
    assert!(ev.iter().any(|e| matches!(e, Event::LinkUp { channel: 6, aid: 3, .. })));
    {
        let w = sim.w();
        let j = &w.fw.rt.join;
        assert!(j.phy && j.binding && j.sta && j.q_on[ap::QUEUE] && j.mcast && j.mac_assoc);
        assert!(!j.te, "the session protection is stopped once the link is up");
        assert_eq!((j.auth_seen, j.assoc_seen), (1, 1));
        assert_eq!(j.frames.len(), 2);
        assert_eq!(j.mac_bssid, [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x01]);
    }
    for m in ["[IWL-JOIN-REQ", "[IWL-JOIN-PHY ok", "[IWL-JOIN-MAC ok", "[IWL-JOIN-BIND ok]", "[IWL-JOIN-TXQ ok", "[IWL-JOIN-STA ok", "[IWL-JOIN-TE ok", "[IWL-AUTH ok", "[IWL-ASSOC aid=3", "[IWL-JOIN-MAC assoc ok", "[IWL-LINK-UP]"] {
        assert!(log_has(&sim, m), "missing marker {m}");
    }
    d.leave();
    assert!(d.link().is_none());
    contexts_gone(&sim);
    assert_eq!(sim.w().fw.rt.join.deauths, vec![3]);
    assert!(d.take_events().iter().any(|e| matches!(e, Event::LinkDown { reason: LinkDown::Left })));
    no_violations(&sim);
}

#[test]
fn a_five_gigahertz_network_uses_the_6_mbit_management_rate() {
    let (sim, mut d) = ready();
    put_ap(&sim, JoinAp::new("FiveG", 0x03, 36, Some(pmk("correct horse", "FiveG"))));
    d.join(&req("FiveG", Security::Wpa2Psk, Some(pmk("correct horse", "FiveG")))).expect("join");
    no_violations(&sim);
    assert!(sim.w().fw.rt.join.m4_seen);
    d.leave();
    contexts_gone(&sim);
    no_violations(&sim);
}

#[test]
fn wpa2_psk_runs_the_four_way_handshake_and_installs_the_keys() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    let rep = d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    assert!(rep.secure);
    {
        let w = sim.w();
        let j = &w.fw.rt.join;
        assert_eq!((j.m2_seen, j.m4_seen), (1, true));
        // the PTK in slot and the GTK in another; one frame each of auth, assoc, M2, M4
        let held: Vec<_> = j.keys.iter().enumerate().filter_map(|(i, k)| k.as_ref().map(|k| (i, k.1))).collect();
        assert_eq!(held.len(), 2, "{held:?}");
        assert!(held.iter().any(|h| !h.1) && held.iter().any(|h| h.1));
        // message 4 was queued unprotected and the PTK went in only afterwards
        let m4 = j.frames.last().unwrap();
        assert_eq!(m4.sec_ctl, 0);
    }
    for m in ["[IWL-4WAY m2 sent]", "[IWL-4WAY m4 queued", "[IWL-KEYS ptk", "[IWL-KEYS gtk id=1", "[IWL-4WAY OK]", "[IWL-LINK-UP]"] {
        assert!(log_has(&sim, m), "missing marker {m}");
    }
    assert!(d.link().unwrap().up);
    no_violations(&sim);
    d.leave();
    contexts_gone(&sim);
    no_violations(&sim);
}

#[test]
fn data_flows_both_ways_after_the_handshake() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    d.take_events();
    let bssid = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x02];
    // station -> AP: three frames, each protected with the next packet number
    for i in 0..3u8 {
        let mut eth = Vec::new();
        eth.extend_from_slice(&bssid); // dst
        eth.extend_from_slice(&STA); // src
        eth.extend_from_slice(&0x0800u16.to_be_bytes());
        eth.extend_from_slice(&[i; 40]);
        d.link_send(&eth).expect("send");
        d.link_poll(5).expect("poll");
    }
    {
        let w = sim.w();
        let data = &w.fw.rt.join.data;
        assert_eq!(data.len(), 3);
        for (i, (pn, et, payload, prot)) in data.iter().enumerate() {
            assert!(*prot);
            assert_eq!(*et, 0x0800);
            assert_eq!(payload, &vec![i as u8; 40]);
            assert_eq!(*pn, i as u64 + 1);
        }
    }
    // AP -> station: a unicast and a group-addressed frame, decrypted by the firmware
    {
        let mut w = sim.w();
        ap::deliver(&mut w, 100, STA, bssid, 0x0806, &[7; 28], true, None);
        ap::deliver(&mut w, 400, [0xff; 6], bssid, 0x0806, &[8; 28], true, Some(1));
    }
    let mut ev = d.link_poll(50).expect("poll");
    ev.extend(d.link_poll(50).expect("poll"));
    let rx: Vec<&Vec<u8>> = ev.iter().filter_map(|e| if let Event::Rx(f) = e { Some(f) } else { None }).collect();
    assert_eq!(rx.len(), 2, "{ev:?}");
    assert_eq!(&rx[0][..6], &STA);
    assert_eq!(&rx[0][6..12], &bssid);
    assert_eq!(&rx[0][12..14], &[0x08, 0x06]);
    assert_eq!(&rx[0][14..], &[7u8; 28]);
    assert_eq!(&rx[1][..6], &[0xff; 6]);
    assert_eq!(&rx[1][14..], &[8u8; 28]);
    assert_eq!(d.link().unwrap().stats.tx_ok, 7, "auth, assoc, M2, M4 and the three data frames");
    no_violations(&sim);
}

#[test]
fn a_replayed_frame_and_an_unprotected_one_are_dropped() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    d.take_events();
    let bssid = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x02];
    {
        let mut w = sim.w();
        ap::deliver(&mut w, 100, STA, bssid, 0x0800, &[1; 20], true, None); // PN 1: accepted
        ap::deliver(&mut w, 300, STA, bssid, 0x0800, &[2; 20], false, None); // cleartext once keyed: dropped
        // the same PN again
        w.fw.rt.join.ap_pn -= 1;
        ap::deliver(&mut w, 500, STA, bssid, 0x0800, &[3; 20], true, None);
    }
    let mut ev = d.link_poll(50).expect("poll");
    ev.extend(d.link_poll(50).expect("poll"));
    ev.extend(d.link_poll(50).expect("poll"));
    let n = ev.iter().filter(|e| matches!(e, Event::Rx(_))).count();
    assert_eq!(n, 1);
    let st = d.link().unwrap().stats;
    assert_eq!((st.rx_replay, st.rx_dropped_port), (1, 1));
    no_violations(&sim);
}

#[test]
fn a_group_rekey_installs_a_new_gtk_and_is_answered() {
    let (sim, mut d) = ready();
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join");
    d.take_events();
    {
        let mut w = sim.w();
        ap::group_rekey(&mut w, 100, [0xd7; 16], 2);
    }
    d.link_poll(50).expect("poll");
    {
        let w = sim.w();
        assert_eq!(w.fw.rt.join.g2_seen, 1);
        let held = w.fw.rt.join.keys.iter().flatten().filter(|k| k.1).count();
        assert_eq!(held, 2, "the old GTK stays until replaced by key id; the new one is in a second slot");
    }
    assert!(log_has(&sim, "[IWL-KEYS gtk id=2"));
    assert!(log_has(&sim, "[IWL-GROUP-REKEY replied]"));
    no_violations(&sim);
}

/// The REAL -46 file, when pointed at: its capability and command-version TLVs select the layouts the join sends
/// (the 8-byte channel info, ADD_STA_KEY v1), and a full WPA2 join runs against the strict firmware model.
#[test]
fn the_real_9260_46_file_joins_a_wpa2_network() {
    let Ok(path) = std::env::var("CORAL_IWL_UCODE") else {
        eprintln!("the_real_9260_46_file_joins_a_wpa2_network: SKIPPED (set CORAL_IWL_UCODE=<decompressed -46 blob>)");
        return;
    };
    let b = sim::blob::from_file(std::fs::read(path).unwrap());
    let sim = sim::Sim::new();
    {
        let mut w = sim.w();
        w.fw.init = Some(b.init.clone());
        w.fw.regular = Some(b.regular.clone());
        w.fw.rt.n_scan = b.n_scan;
        w.fw.rt.aps = scan_aps();
        w.fw.rt.junk = false;
        w.fw.rt.chatter = false;
        w.dma_quota_bytes = 1024 * 4096;
    }
    let mut d = iwlwifi_device::driver::Driver::new(sim.host(), b.bytes.clone(), &iwlwifi_device::sha256::sha256(&b.bytes)).expect("file");
    d.bring_up().expect("INIT");
    d.start_rt().expect("RT");
    d.scan(1).expect("scan");
    let k = pmk("correct horse", "Home");
    put_ap(&sim, JoinAp::new("Home", 0x02, 11, Some(k)));
    d.join(&req("Home", Security::Wpa2Psk, Some(k))).expect("join on the real file");
    no_violations(&sim);
    d.leave();
    contexts_gone(&sim);
    no_violations(&sim);
    eprintln!("DMA-PAGES real file after a join: {}", sim.w().dma_used / 4096);
}
