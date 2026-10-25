// SPDX-License-Identifier: ISC
//! A test authenticator for the supplicant, shared by this crate's tests and by device crates that
//! run a simulated access point. It builds frames from LITERAL offsets and bit values (not the
//! crate's `off` / `info` constants), so a wrong constant in the crate cannot cancel against the
//! test. Compiled only with the `testkit` feature; it uses `std`.
#![allow(dead_code)]

use crate::rsne;
use crate::{Akm, Cipher, Config, Supplicant};
use std::format;
use std::string::String;
use std::vec;
use std::vec::Vec;
use wpa_crypto_core::kdf::{derive_ptk, KdfKind, Ptk};
use wpa_crypto_core::keywrap::wrap_in_place;
use wpa_crypto_core::mic;
use wpa_crypto_core::pbkdf2::passphrase_to_pmk;
use wpa_crypto_core::Pmk;

pub const AA: [u8; 6] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
pub const SPA: [u8; 6] = [0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb];

/// The PMK of passphrase "password" on SSID "IEEE" (802.11-2020 J.4.1).
pub fn pmk() -> Pmk {
    passphrase_to_pmk(b"password", b"IEEE").unwrap()
}

pub fn rsne_for(akm: Akm, group: Cipher) -> Vec<u8> {
    let mut b = [0u8; 64];
    let n = rsne::build(&mut b, group, akm).unwrap();
    b[..n].to_vec()
}

pub struct Auth {
    pub aa: [u8; 6],
    pub spa: [u8; 6],
    pub akm: Akm,
    pub kind: KdfKind,
    pub version: u16,
    pub anonce: [u8; 32],
    pub replay: u64,
    pub ap_rsne: Vec<u8>,
    pub ptk: Option<Ptk>,
    pub pmk: Pmk,
    pub gtk: [u8; 16],
    pub gtk_id: u8,
    pub rsc: [u8; 6],
}

impl Auth {
    pub fn new(akm: Akm) -> Auth {
        let (kind, version) = match akm {
            Akm::Psk => (KdfKind::PrfSha1, 2),
            _ => (KdfKind::KdfSha256, 3),
        };
        Auth {
            aa: AA,
            spa: SPA,
            akm,
            kind,
            version,
            anonce: [0x5a; 32],
            replay: 0,
            ap_rsne: rsne_for(akm, Cipher::Ccmp),
            ptk: None,
            pmk: pmk(),
            gtk: [0xc0; 16],
            gtk_id: 1,
            rsc: [9, 0, 0, 0, 0, 0],
        }
    }

    pub fn config<'a>(&'a self, own: &'a [u8]) -> Config<'a> {
        Config { aa: self.aa, spa: self.spa, akm: self.akm, group: Cipher::Ccmp, own_rsne: own, ap_rsne: &self.ap_rsne }
    }

    pub fn supplicant(&self, now: u64) -> Supplicant {
        let own = self.ap_rsne.clone();
        Supplicant::new(&self.config(&own), Pmk::from_bytes(*self.pmk.as_bytes()), now).unwrap()
    }

    /// Raw EAPOL-Key frame, MIC zero.
    pub fn raw(&self, info: u16, keylen: u16, replay: u64, nonce: &[u8; 32], rsc: &[u8; 6], data: &[u8]) -> Vec<u8> {
        let mut f = vec![0u8; 99];
        f[0] = 2;
        f[1] = 3;
        f[4] = 2;
        f[5..7].copy_from_slice(&info.to_be_bytes());
        f[7..9].copy_from_slice(&keylen.to_be_bytes());
        f[9..17].copy_from_slice(&replay.to_be_bytes());
        f[17..49].copy_from_slice(nonce);
        f[65..71].copy_from_slice(rsc);
        f[97..99].copy_from_slice(&(data.len() as u16).to_be_bytes());
        f.extend_from_slice(data);
        let body = (f.len() - 4) as u16;
        f[2..4].copy_from_slice(&body.to_be_bytes());
        f
    }

    pub fn stamp(&self, f: &mut [u8], kck: &[u8; 16]) {
        let m = mic::compute(self.version as u8, kck, f).unwrap();
        f[81..97].copy_from_slice(&m);
    }

    pub fn m1(&mut self) -> Vec<u8> {
        self.replay += 1;
        // ACK | PAIRWISE | version
        self.raw(0x0088 | self.version, 0, self.replay, &self.anonce.clone(), &[0; 6], &[])
    }

    pub fn m1_with_data(&mut self, data: &[u8]) -> Vec<u8> {
        self.replay += 1;
        self.raw(0x0088 | self.version, 0, self.replay, &self.anonce.clone(), &[0; 6], data)
    }

    /// Learn the PTK from the supplicant's message 2 and verify it.
    pub fn check_m2(&mut self, m2: &[u8], sta_rsne: &[u8]) -> Result<(), String> {
        if m2.len() != 99 + sta_rsne.len() {
            return Err(format!("m2 length {}", m2.len()));
        }
        let info = u16::from_be_bytes([m2[5], m2[6]]);
        // PAIRWISE | MIC | version, and neither ACK, INSTALL, SECURE, ENCRYPTED
        if info != 0x0108 | self.version {
            return Err(format!("m2 info {info:#06x}"));
        }
        let mut snonce = [0u8; 32];
        snonce.copy_from_slice(&m2[17..49]);
        let ptk = derive_ptk(self.kind, &self.pmk, &self.aa, &self.spa, &self.anonce, &snonce);
        let mut copy = m2.to_vec();
        copy[81..97].fill(0);
        let want = mic::compute(self.version as u8, &ptk.kck, &copy).unwrap();
        if want[..] != m2[81..97] {
            return Err("m2 MIC".into());
        }
        if m2[9..17] != self.replay.to_be_bytes() {
            return Err("m2 replay counter is not m1's".into());
        }
        if &m2[99..] != sta_rsne {
            return Err("m2 RSNE".into());
        }
        if m2[7..9] != [0, 0] {
            return Err("m2 key length".into());
        }
        self.ptk = Some(ptk);
        Ok(())
    }

    pub fn gtk_kde(&self, key: &[u8], id: u8, tx: bool) -> Vec<u8> {
        let mut k = vec![0xdd, (6 + key.len()) as u8, 0x00, 0x0f, 0xac, 0x01, id | if tx { 4 } else { 0 }, 0];
        k.extend_from_slice(key);
        k
    }

    /// Pad (0xdd, 0...) to a multiple of 8, at least 16, then wrap under the KEK.
    pub fn wrap(&self, mut data: Vec<u8>) -> Vec<u8> {
        if data.len() < 16 || data.len() % 8 != 0 {
            data.push(0xdd);
            while data.len() < 16 || data.len() % 8 != 0 {
                data.push(0);
            }
        }
        let mut buf = vec![0u8; 8];
        buf.extend_from_slice(&data);
        wrap_in_place(&self.ptk.as_ref().unwrap().kek, &mut buf).unwrap();
        buf
    }

    pub const M3_INFO: u16 = 0x1000 | 0x0200 | 0x0100 | 0x0080 | 0x0040 | 0x0008; // E S M A I P

    pub fn m3_with(&mut self, plain: Vec<u8>, info: u16, keylen: u16) -> Vec<u8> {
        self.replay += 1;
        let enc = self.wrap(plain);
        let kck = self.ptk.as_ref().unwrap().kck;
        let rsc = self.rsc;
        let mut f = self.raw(info | self.version, keylen, self.replay, &self.anonce.clone(), &rsc, &enc);
        self.stamp(&mut f, &kck);
        f
    }

    pub fn m3(&mut self) -> Vec<u8> {
        let mut plain = self.ap_rsne.clone();
        plain.extend_from_slice(&self.gtk_kde(&self.gtk.clone(), self.gtk_id, true));
        self.m3_with(plain, Self::M3_INFO, 16)
    }

    pub fn check_m4(&self, m4: &[u8], want_replay: u64) -> Result<(), String> {
        if m4.len() != 99 {
            return Err(format!("m4 length {}", m4.len()));
        }
        let info = u16::from_be_bytes([m4[5], m4[6]]);
        // S | M | P | version
        if info != 0x0200 | 0x0100 | 0x0008 | self.version {
            return Err(format!("m4 info {info:#06x}"));
        }
        if m4[9..17] != want_replay.to_be_bytes() {
            return Err("m4 replay".into());
        }
        let mut copy = m4.to_vec();
        copy[81..97].fill(0);
        let kck = self.ptk.as_ref().unwrap().kck;
        let want = mic::compute(self.version as u8, &kck, &copy).unwrap();
        if want[..] != m4[81..97] {
            return Err("m4 MIC".into());
        }
        if m4[17..49] != [0u8; 32] {
            return Err("m4 nonce not zero".into());
        }
        Ok(())
    }

    pub fn g1_with(&mut self, plain: Vec<u8>, encrypted: bool) -> Vec<u8> {
        self.replay += 1;
        // E | S | M | A (no PAIRWISE) | version
        let mut info = 0x0200 | 0x0100 | 0x0080 | self.version;
        let data = if encrypted {
            info |= 0x1000;
            self.wrap(plain)
        } else {
            plain
        };
        let kck = self.ptk.as_ref().unwrap().kck;
        let rsc = self.rsc;
        let mut f = self.raw(info, 16, self.replay, &[0; 32], &rsc, &data);
        self.stamp(&mut f, &kck);
        f
    }

    pub fn g1(&mut self, key: &[u8; 16], id: u8) -> Vec<u8> {
        let kde = self.gtk_kde(key, id, true);
        self.g1_with(kde, true)
    }

    pub fn check_g2(&self, g2: &[u8], want_replay: u64) -> Result<(), String> {
        if g2.len() != 99 {
            return Err(format!("g2 length {}", g2.len()));
        }
        let info = u16::from_be_bytes([g2[5], g2[6]]);
        // S | M | version, no PAIRWISE
        if info != 0x0200 | 0x0100 | self.version {
            return Err(format!("g2 info {info:#06x}"));
        }
        if g2[9..17] != want_replay.to_be_bytes() {
            return Err("g2 replay".into());
        }
        let mut copy = g2.to_vec();
        copy[81..97].fill(0);
        let kck = self.ptk.as_ref().unwrap().kck;
        let want = mic::compute(self.version as u8, &kck, &copy).unwrap();
        if want[..] != g2[81..97] {
            return Err("g2 MIC".into());
        }
        Ok(())
    }
}

