// SPDX-License-Identifier: ISC
//! The EAPOL-Key MIC. Port of OpenBSD `ieee80211_eapol_key_mic` / `_check_mic`
//! (sys/net80211/ieee80211_crypto.c). Descriptor version 1 (HMAC-MD5/RC4) is deliberately not
//! implemented: it belongs to WEP/TKIP-era networks this station refuses.

use crate::cmac::aes_cmac;
use crate::hash::hmac_sha1;

pub const MIC_LEN: usize = 16;
/// Key Descriptor Version, EAPOL-Key Key Information bits 0..2 (802.11-2020 12.7.2).
pub const VERSION_HMAC_SHA1_AES: u8 = 2;
pub const VERSION_AES_CMAC: u8 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicRefusal {
    /// A descriptor version this station has no MIC for (1 = HMAC-MD5, 0 and 4..7 are reserved).
    UnsupportedVersion(u8),
    /// The recomputed MIC differs from the frame's.
    Mismatch,
}

/// MIC over `frame`, the whole EAPOL PDU (from the EAPOL version byte through the key data) with
/// its MIC field already zero.
pub fn compute(version: u8, kck: &[u8; 16], frame: &[u8]) -> Result<[u8; MIC_LEN], MicRefusal> {
    match version {
        VERSION_HMAC_SHA1_AES => {
            let mut o = [0u8; MIC_LEN];
            o.copy_from_slice(&hmac_sha1(kck, &[frame])[..MIC_LEN]); // truncated to 128 bits
            Ok(o)
        }
        VERSION_AES_CMAC => Ok(aes_cmac(kck, &[frame])),
        v => Err(MicRefusal::UnsupportedVersion(v)),
    }
}

/// Check the MIC of a received frame. `frame` is the received bytes as they came off the air;
/// the MIC field is `frame[mic_off..mic_off + 16]`. The check recomputes over the received bytes
/// with that field zeroed, so a frame is never re-encoded before it is authenticated.
pub fn verify(version: u8, kck: &[u8; 16], frame: &[u8], mic_off: usize) -> Result<(), MicRefusal> {
    if mic_off + MIC_LEN > frame.len() {
        return Err(MicRefusal::Mismatch);
    }
    let want = match version {
        VERSION_HMAC_SHA1_AES => {
            let z = [0u8; MIC_LEN];
            let h = hmac_sha1(kck, &[&frame[..mic_off], &z, &frame[mic_off + MIC_LEN..]]);
            let mut o = [0u8; MIC_LEN];
            o.copy_from_slice(&h[..MIC_LEN]);
            o
        }
        VERSION_AES_CMAC => {
            let z = [0u8; MIC_LEN];
            aes_cmac(kck, &[&frame[..mic_off], &z, &frame[mic_off + MIC_LEN..]])
        }
        v => return Err(MicRefusal::UnsupportedVersion(v)),
    };
    if crate::ct_eq(&want, &frame[mic_off..mic_off + MIC_LEN]) {
        Ok(())
    } else {
        Err(MicRefusal::Mismatch)
    }
}
