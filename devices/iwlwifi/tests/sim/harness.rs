// SPDX-License-Identifier: GPL-2.0-only
//! Shared set-up of the join tests: a card with the RT image up and one scan done, an access point on the air, and the
//! checks every test ends with.
#![allow(dead_code)]
use super::ap::{self, JoinAp};
use super::blob::{build, Opts};
use super::rtfw::{ApSec, SimAp};
use super::{Sim, SimHost};
use iwlwifi_device::driver::Driver;
use iwlwifi_device::event::{JoinRequest, Security, Ssid};
use iwlwifi_device::sha256::sha256;
use wpa_crypto_core::pbkdf2::passphrase_to_pmk;

pub const STA: [u8; 6] = [0x12, 0x22, 0x33, 0x44, 0x55, 0x66];

pub fn pmk(pass: &str, ssid: &str) -> [u8; 32] {
    *passphrase_to_pmk(pass.as_bytes(), ssid.as_bytes()).unwrap().as_bytes()
}

pub fn scan_aps() -> Vec<SimAp> {
    vec![
        SimAp::new("CoralNet", 0x01, 6, -42, ApSec::Open),
        SimAp::new("Home", 0x02, 11, -61, ApSec::Wpa2Psk),
        SimAp::new("FiveG", 0x03, 36, -50, ApSec::Wpa2Psk),
        SimAp::new("Sae", 0x06, 44, -66, ApSec::Wpa2Sae),
        SimAp::new("Tkip", 0x08, 48, -77, ApSec::Wpa2Tkip),
    ]
}

/// A card with the RT image up and one scan done, so the beacons are known.
pub fn ready() -> (Sim, Driver<SimHost>) {
    let b = build(&Opts { rt: true, ..Opts::default() });
    let sim = Sim::new();
    {
        let mut w = sim.w();
        w.fw.init = Some(b.init.clone());
        w.fw.regular = Some(b.regular.clone());
        w.fw.rt.n_scan = b.n_scan;
        w.fw.rt.aps = scan_aps();
        w.fw.rt.junk = false;
        w.fw.rt.chatter = false;
    }
    let mut d = Driver::new(sim.host(), b.bytes.clone(), &sha256(&b.bytes)).expect("blob");
    d.bring_up().expect("INIT");
    d.start_rt().expect("RT");
    d.scan(1).expect("scan");
    (sim, d)
}

pub fn put_ap(sim: &Sim, ap: JoinAp) {
    let mut w = sim.w();
    let psk = ap.pmk.is_some();
    w.fw.rt.join.ap = Some(ap);
    if psk {
        ap::start_authenticator(&mut w);
    }
}

pub fn req(ssid: &str, security: Security, pmk: Option<[u8; 32]>) -> JoinRequest {
    JoinRequest { ssid: Ssid::new(ssid.as_bytes()).unwrap(), bssid: None, security, pmk }
}

pub fn no_violations(sim: &Sim) {
    assert_eq!(sim.w().violations, Vec::<String>::new());
}

pub fn log_has(sim: &Sim, needle: &str) -> bool {
    sim.w().log.iter().any(|l| l.contains(needle))
}

/// Every firmware context the join made is gone.
pub fn contexts_gone(sim: &Sim) {
    let w = sim.w();
    let j = &w.fw.rt.join;
    assert!(!j.phy && !j.binding && !j.sta && !j.te && !j.q_on[ap::QUEUE] && !j.mac_assoc);
    assert_eq!(j.mac_bssid, [0; 6]);
    assert!(j.keys.iter().all(|k| k.is_none()));
}
