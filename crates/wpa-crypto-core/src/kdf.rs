// SPDX-License-Identifier: ISC
//! The 802.11 key-derivation functions and the pairwise transient key (PTK) split.
//! Port of OpenBSD `ieee80211_prf`, `ieee80211_kdf` and `ieee80211_derive_ptk`
//! (sys/net80211/ieee80211_crypto.c), which implement IEEE Std 802.11-2020 12.7.1.2 (PRF-SHA1),
//! 12.7.1.7.2 (KDF-SHA256) and 12.7.1.3 (PTK = PRF/KDF(PMK, "Pairwise key expansion",
//! Min(AA,SPA) || Max(AA,SPA) || Min(ANonce,SNonce) || Max(ANonce,SNonce))).

use crate::hash::{hmac_sha1, hmac_sha256};
use crate::Pmk;

pub const PTK_LABEL: &[u8] = b"Pairwise key expansion"; // 802.11-2020 12.7.1.3

/// Largest output either function is asked for in this crate (CCMP PTK = 48 bytes).
pub const KDF_OUT_MAX: usize = 64;

/// PRF-N (802.11-2020 12.7.1.2): HMAC-SHA1(K, label || 0x00 || data || i) for i = 0, 1, ...,
/// truncated to `out.len()` bytes.
pub fn prf_sha1(key: &[u8], label: &[u8], data: &[u8], out: &mut [u8]) {
    for (i, chunk) in out.chunks_mut(20).enumerate() {
        let ctr = [i as u8];
        let h = hmac_sha1(key, &[label, &[0u8], data, &ctr]);
        chunk.copy_from_slice(&h[..chunk.len()]);
    }
}

/// KDF-SHA256 (802.11-2020 12.7.1.7.2): HMAC-SHA256(K, i_le16 || label || context || bits_le16)
/// for i = 1, 2, ..., truncated to `out.len()` bytes. The output length in BITS is mixed into
/// every block, so two lengths of the same key and context share no output.
pub fn kdf_sha256(key: &[u8], label: &[u8], context: &[u8], out: &mut [u8]) {
    let bits = ((out.len() * 8) as u16).to_le_bytes();
    for (i, chunk) in out.chunks_mut(32).enumerate() {
        let ctr = ((i + 1) as u16).to_le_bytes();
        let h = hmac_sha256(key, &[&ctr, label, context, &bits]);
        chunk.copy_from_slice(&h[..chunk.len()]);
    }
}

/// The three keys a CCMP PTK is split into (802.11-2020 12.7.1.3 Table 12-11).
#[derive(Clone)]
pub struct Ptk {
    pub kck: [u8; 16],
    pub kek: [u8; 16],
    pub tk: [u8; 16],
}

impl Drop for Ptk {
    fn drop(&mut self) {
        crate::wipe(&mut self.kck);
        crate::wipe(&mut self.kek);
        crate::wipe(&mut self.tk);
    }
}

/// Which hash the AKM's key derivation uses (802.11-2020 12.7.1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KdfKind {
    /// AKM 00-0F-AC:2 (PSK) and :1: PRF-SHA1, key descriptor version 2.
    PrfSha1,
    /// AKM 00-0F-AC:6 (PSK-SHA256) and :8 (SAE): KDF-SHA256, key descriptor version 3.
    KdfSha256,
}

/// Derive the CCMP PTK. `aa` is the authenticator (AP) address, `spa` the supplicant's.
pub fn derive_ptk(
    kind: KdfKind,
    pmk: &Pmk,
    aa: &[u8; 6],
    spa: &[u8; 6],
    anonce: &[u8; 32],
    snonce: &[u8; 32],
) -> Ptk {
    let mut buf = [0u8; 2 * 6 + 2 * 32];
    let (lo_a, hi_a) = if aa < spa { (aa, spa) } else { (spa, aa) };
    buf[0..6].copy_from_slice(lo_a);
    buf[6..12].copy_from_slice(hi_a);
    let (lo_n, hi_n) = if anonce < snonce { (anonce, snonce) } else { (snonce, anonce) };
    buf[12..44].copy_from_slice(lo_n);
    buf[44..76].copy_from_slice(hi_n);
    let mut raw = [0u8; 48];
    match kind {
        KdfKind::PrfSha1 => prf_sha1(pmk.as_bytes(), PTK_LABEL, &buf, &mut raw),
        KdfKind::KdfSha256 => kdf_sha256(pmk.as_bytes(), PTK_LABEL, &buf, &mut raw),
    }
    let mut p = Ptk { kck: [0; 16], kek: [0; 16], tk: [0; 16] };
    p.kck.copy_from_slice(&raw[0..16]);
    p.kek.copy_from_slice(&raw[16..32]);
    p.tk.copy_from_slice(&raw[32..48]);
    crate::wipe(&mut raw);
    p
}

/// PMKID (802.11-2020 12.7.1.3): first 128 bits of HMAC-H(PMK, "PMK Name" || AA || SPA).
pub fn pmkid(kind: KdfKind, pmk: &Pmk, aa: &[u8; 6], spa: &[u8; 6]) -> [u8; 16] {
    let mut o = [0u8; 16];
    match kind {
        KdfKind::PrfSha1 => o.copy_from_slice(&hmac_sha1(pmk.as_bytes(), &[b"PMK Name", aa, spa])[..16]),
        KdfKind::KdfSha256 => {
            o.copy_from_slice(&hmac_sha256(pmk.as_bytes(), &[b"PMK Name", aa, spa])[..16])
        }
    }
    o
}
