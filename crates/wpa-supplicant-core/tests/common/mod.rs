// SPDX-License-Identifier: ISC
//! A test authenticator and a recording installer. The authenticator builds frames with LITERAL
//! offsets and bit values, not the crate's `off` / `info` constants, so a wrong constant in the
//! crate cannot cancel against the test.
#![allow(dead_code)]

pub use wpa_supplicant_core::testkit::*;
use wpa_supplicant_core::{Cipher, Entropy, GroupKey, InstallError, KeyInstaller, PairwiseKey, Supplicant};


#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ev {
    Ptk { tk: Vec<u8>, rsc: u64 },
    Gtk { id: u8, key: Vec<u8>, tx: bool, rsc: u64 },
    Open,
}

#[derive(Default)]
pub struct Rec {
    pub log: Vec<Ev>,
    pub refuse_ptk: bool,
    pub refuse_gtk: bool,
}

impl KeyInstaller for Rec {
    fn install_ptk(&mut self, k: &PairwiseKey<'_>) -> Result<(), InstallError> {
        if self.refuse_ptk {
            return Err(InstallError::Refused);
        }
        assert_eq!(k.cipher, Cipher::Ccmp);
        self.log.push(Ev::Ptk { tk: k.tk.to_vec(), rsc: k.rsc });
        Ok(())
    }
    fn install_gtk(&mut self, k: &GroupKey<'_>) -> Result<(), InstallError> {
        if self.refuse_gtk {
            return Err(InstallError::Refused);
        }
        self.log.push(Ev::Gtk { id: k.key_id, key: k.key.to_vec(), tx: k.tx, rsc: k.rsc });
        Ok(())
    }
    fn open_port(&mut self) {
        self.log.push(Ev::Open);
    }
}

/// Counts fills and hands out a distinct pattern each time.
#[derive(Default)]
pub struct Seq {
    pub fills: u8,
}

impl Entropy for Seq {
    fn fill(&mut self, out: &mut [u8]) {
        self.fills += 1;
        for (i, b) in out.iter_mut().enumerate() {
            *b = self.fills.wrapping_mul(16).wrapping_add(i as u8);
        }
    }
}

/// Run message 1 -> 2 and return the supplicant ready for message 3.
pub fn through_m2(a: &mut Auth, s: &mut Supplicant, rng: &mut Seq, inst: &mut Rec) -> Vec<u8> {
    let m1 = a.m1();
    let mut tx = [0u8; 512];
    let own = a.ap_rsne.clone();
    let r = s.on_eapol(&m1, &mut tx, 10, rng, inst).unwrap();
    let n = match r {
        wpa_supplicant_core::Reply::Send(n) => n,
        _ => panic!("no m2"),
    };
    a.check_m2(&tx[..n], &own).unwrap();
    tx[..n].to_vec()
}

pub fn send(s: &mut Supplicant, rx: &[u8], now: u64, rng: &mut Seq, inst: &mut Rec) -> Result<Vec<u8>, wpa_supplicant_core::Refusal> {
    let mut tx = [0u8; 512];
    match s.on_eapol(rx, &mut tx, now, rng, inst)? {
        wpa_supplicant_core::Reply::Send(n) => Ok(tx[..n].to_vec()),
        wpa_supplicant_core::Reply::None => Ok(Vec::new()),
    }
}

/// Full 4-way: returns after tx_done with the supplicant Established.
pub fn establish(a: &mut Auth, s: &mut Supplicant, rng: &mut Seq, inst: &mut Rec) {
    through_m2(a, s, rng, inst);
    let m3 = a.m3();
    let m4 = send(s, &m3, 20, rng, inst).unwrap();
    a.check_m4(&m4, a.replay).unwrap();
    s.tx_done(inst).unwrap();
    assert!(s.is_established());
}
