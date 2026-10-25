// SPDX-License-Identifier: ISC
//! The RSN information element (IEEE Std 802.11-2020 9.4.2.24): parse for the 4-way handshake's
//! consistency checks, and build the one an association request and message 2 carry.
//! Counterpart of OpenBSD `ieee80211_parse_rsn` / `ieee80211_add_rsn_body`
//! (sys/net80211/ieee80211_proto.c / ieee80211_output.c).

use crate::kde::{ELEMID_RSN, OUI};
use crate::refusal::Refusal;

/// Cipher suite types under OUI 00-0F-AC (802.11-2020 Table 9-149).
pub const SUITE_TKIP: u8 = 2;
pub const SUITE_CCMP: u8 = 4;
/// AKM suite types under OUI 00-0F-AC (802.11-2020 Table 9-151).
pub const AKM_8021X: u8 = 1;
pub const AKM_PSK: u8 = 2;
pub const AKM_8021X_SHA256: u8 = 5;
pub const AKM_PSK_SHA256: u8 = 6;
pub const AKM_SAE: u8 = 8;

/// A cipher this station can run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cipher {
    Ccmp,
    Tkip,
}

impl Cipher {
    pub fn suite(self) -> u8 {
        match self {
            Cipher::Ccmp => SUITE_CCMP,
            Cipher::Tkip => SUITE_TKIP,
        }
    }
    /// Temporal key length in bytes (802.11-2020 12.7.1.3 Table 12-10 / ieee80211_cipher_keylen).
    pub fn key_len(self) -> usize {
        match self {
            Cipher::Ccmp => 16,
            Cipher::Tkip => 32,
        }
    }
}

/// A key management suite this station can complete the handshake for. The PMK comes from
/// elsewhere in every case (PBKDF2 for `Psk`/`PskSha256`, the SAE exchange for `Sae`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Akm {
    Psk,
    PskSha256,
    Sae,
}

impl Akm {
    pub fn suite(self) -> u8 {
        match self {
            Akm::Psk => AKM_PSK,
            Akm::PskSha256 => AKM_PSK_SHA256,
            Akm::Sae => AKM_SAE,
        }
    }
    /// Key descriptor version the AKM requires (802.11-2020 12.7.2 b.2).
    pub fn descriptor_version(self) -> u8 {
        match self {
            Akm::Psk => 2,
            Akm::PskSha256 | Akm::Sae => 3,
        }
    }
    pub fn kdf(self) -> wpa_crypto_core::kdf::KdfKind {
        match self {
            Akm::Psk => wpa_crypto_core::kdf::KdfKind::PrfSha1,
            Akm::PskSha256 | Akm::Sae => wpa_crypto_core::kdf::KdfKind::KdfSha256,
        }
    }
}

/// Longest information element: id + length + 255 bytes.
pub const IE_MAX: usize = 257;

/// A parsed view of an RSNE (all fields borrow the element).
#[derive(Debug, Clone, Copy)]
pub struct Rsne<'a> {
    pub group: [u8; 4],
    pairwise: &'a [u8],
    akms: &'a [u8],
    pub caps: u16,
}

fn suite_is(s: &[u8], ty: u8) -> bool {
    s.len() == 4 && s[..3] == OUI && s[3] == ty
}

impl<'a> Rsne<'a> {
    /// Parse a whole element (id, length, body).
    pub fn parse(ie: &'a [u8]) -> Result<Rsne<'a>, Refusal> {
        if ie.len() < 2 || ie[0] != ELEMID_RSN || ie[1] as usize + 2 != ie.len() {
            return Err(Refusal::RsneMalformed);
        }
        let b = &ie[2..];
        // version(2) group(4) pairwise-count(2)
        if b.len() < 8 || u16::from_le_bytes([b[0], b[1]]) != 1 {
            return Err(Refusal::RsneMalformed);
        }
        let mut group = [0u8; 4];
        group.copy_from_slice(&b[2..6]);
        let pc = u16::from_le_bytes([b[6], b[7]]) as usize;
        let p_end = 8 + 4 * pc;
        if b.len() < p_end + 2 {
            return Err(Refusal::RsneMalformed);
        }
        let pairwise = &b[8..p_end];
        let ac = u16::from_le_bytes([b[p_end], b[p_end + 1]]) as usize;
        let a_end = p_end + 2 + 4 * ac;
        if b.len() < a_end {
            return Err(Refusal::RsneMalformed);
        }
        let akms = &b[p_end + 2..a_end];
        let caps = if b.len() >= a_end + 2 { u16::from_le_bytes([b[a_end], b[a_end + 1]]) } else { 0 };
        Ok(Rsne { group, pairwise, akms, caps })
    }

    pub fn pairwise_count(&self) -> usize {
        self.pairwise.len() / 4
    }
    pub fn offers_pairwise(&self, c: Cipher) -> bool {
        self.pairwise.chunks_exact(4).any(|s| suite_is(s, c.suite()))
    }
    pub fn offers_akm(&self, a: Akm) -> bool {
        self.akms.chunks_exact(4).any(|s| suite_is(s, a.suite()))
    }
    pub fn akm_count(&self) -> usize {
        self.akms.len() / 4
    }
    pub fn group_is(&self, c: Cipher) -> bool {
        suite_is(&self.group, c.suite())
    }
}

/// Management Frame Protection capability bits are never offered: this station does not do 802.11w.
pub const CAPS_NONE: u16 = 0;

/// Build the RSNE of a station that offers exactly one pairwise cipher (CCMP) and one AKM, into
/// `out`; returns its length.
pub fn build(out: &mut [u8], group: Cipher, akm: Akm) -> Result<usize, Refusal> {
    const LEN: usize = 2 + 2 + 4 + 2 + 4 + 2 + 4 + 2;
    if out.len() < LEN {
        return Err(Refusal::TxBufferTooSmall { need: LEN, have: out.len() });
    }
    out[0] = ELEMID_RSN;
    out[1] = (LEN - 2) as u8;
    out[2..4].copy_from_slice(&1u16.to_le_bytes());
    out[4..7].copy_from_slice(&OUI);
    out[7] = group.suite();
    out[8..10].copy_from_slice(&1u16.to_le_bytes());
    out[10..13].copy_from_slice(&OUI);
    out[13] = SUITE_CCMP;
    out[14..16].copy_from_slice(&1u16.to_le_bytes());
    out[16..19].copy_from_slice(&OUI);
    out[19] = akm.suite();
    out[20..22].copy_from_slice(&CAPS_NONE.to_le_bytes());
    Ok(LEN)
}
