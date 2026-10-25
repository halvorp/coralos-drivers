// SPDX-License-Identifier: ISC
//! The cryptography an IEEE 802.11 RSN station needs to run the 4-way and group-key handshakes,
//! with no dependency, no allocation and no I/O: SHA-1, SHA-256, HMAC, PBKDF2-HMAC-SHA1 (passphrase
//! to PMK), the 802.11 PRF-SHA1 and KDF-SHA256 (PTK), AES-128, RFC 3394 key wrap, AES-128-CMAC,
//! and the EAPOL-Key MIC for key descriptor versions 2 and 3.
//!
//! Function split follows OpenBSD `sys/net80211/ieee80211_crypto.c` (ISC, Damien Bergamini; see
//! LICENSE-OpenBSD-ISC). Algorithms are FIPS 180-4 (SHA), RFC 2104 (HMAC), RFC 8018 (PBKDF2),
//! RFC 3394 (key wrap), RFC 4493 (CMAC) and IEEE Std 802.11-2020 12.7.1.2 / 12.7.1.6 (PRF, KDF).
//! Table lookups are not constant-time; the callers run in a ring-3 device process that no other
//! tenant shares.
//!
//! Where the secret lives: [`pbkdf2::passphrase_to_pmk`] is the ONLY function that sees a
//! passphrase. The caller that owns the credential runs it and hands the 32-byte [`Pmk`] on; the
//! supplicant crates take a `Pmk`, never a passphrase.

#![no_std]
#![forbid(unsafe_code)]

pub mod aes;
pub mod cmac;
pub mod hash;
pub mod kdf;
pub mod keywrap;
pub mod mic;
pub mod pbkdf2;

/// Length of a pairwise master key (802.11-2020 12.7.1.3: 256 bits).
pub const PMK_LEN: usize = 32;

/// A pairwise master key. Not `Debug` and not `Clone`: it is zeroed on drop and cannot be printed,
/// so it never reaches a log line or a refusal.
pub struct Pmk([u8; PMK_LEN]);

impl Pmk {
    pub fn from_bytes(b: [u8; PMK_LEN]) -> Pmk {
        Pmk(b)
    }
    pub fn as_bytes(&self) -> &[u8; PMK_LEN] {
        &self.0
    }
}

impl Drop for Pmk {
    fn drop(&mut self) {
        wipe(&mut self.0);
    }
}

/// Overwrite `b` with zeros in a way the optimiser may not elide.
pub fn wipe(b: &mut [u8]) {
    for x in b.iter_mut() {
        *x = core::hint::black_box(0);
    }
    core::hint::black_box(&*b);
}

/// Constant-time equality of two byte strings; unequal lengths compare false.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut d = 0u8;
    for i in 0..a.len() {
        d |= a[i] ^ b[i];
    }
    d == 0
}
