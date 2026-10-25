// SPDX-License-Identifier: ISC
//! PBKDF2-HMAC-SHA1 (RFC 8018) and the 802.11 passphrase mapping (802.11-2020 J.4.1).

use crate::hash::hmac_sha1;
use crate::{Pmk, PMK_LEN};

/// 802.11-2020 J.4.1: 4096 iterations.
pub const PSK_ITERATIONS: u32 = 4096;
/// Passphrase length bounds in bytes (802.11-2020 J.4.1: 8..=63).
pub const PASSPHRASE_MIN: usize = 8;
pub const PASSPHRASE_MAX: usize = 63;
/// SSID length bound (802.11-2020 9.4.2.2: 1..=32 for a PSK network).
pub const SSID_MAX: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PskRefusal {
    /// Passphrase shorter than 8 or longer than 63 bytes; carries the length seen.
    PassphraseLength(usize),
    /// A passphrase byte outside printable ASCII 32..=126 (J.4.1); carries its index, never the byte.
    PassphraseNotAscii(usize),
    /// SSID empty or longer than 32 bytes; carries the length seen.
    SsidLength(usize),
}

/// PBKDF2 with HMAC-SHA1 as the PRF, filling `out` (any length).
pub fn pbkdf2_hmac_sha1(password: &[u8], salt: &[u8], iterations: u32, out: &mut [u8]) {
    let mut block_no = 1u32;
    for chunk in out.chunks_mut(20) {
        let bn = block_no.to_be_bytes();
        let mut u = hmac_sha1(password, &[salt, &bn]);
        let mut t = u;
        for _ in 1..iterations {
            u = hmac_sha1(password, &[&u]);
            for i in 0..20 {
                t[i] ^= u[i];
            }
        }
        chunk.copy_from_slice(&t[..chunk.len()]);
        block_no += 1;
    }
}

/// PMK = PBKDF2(passphrase, ssid, 4096, 256 bits). The only function in this repository that sees
/// a passphrase; run it where the credential lives and pass the [`Pmk`] on.
pub fn passphrase_to_pmk(passphrase: &[u8], ssid: &[u8]) -> Result<Pmk, PskRefusal> {
    if passphrase.len() < PASSPHRASE_MIN || passphrase.len() > PASSPHRASE_MAX {
        return Err(PskRefusal::PassphraseLength(passphrase.len()));
    }
    if let Some(i) = passphrase.iter().position(|&b| !(32..=126).contains(&b)) {
        return Err(PskRefusal::PassphraseNotAscii(i));
    }
    if ssid.is_empty() || ssid.len() > SSID_MAX {
        return Err(PskRefusal::SsidLength(ssid.len()));
    }
    let mut pmk = [0u8; PMK_LEN];
    pbkdf2_hmac_sha1(passphrase, ssid, PSK_ITERATIONS, &mut pmk);
    Ok(Pmk::from_bytes(pmk))
}
