// SPDX-License-Identifier: ISC
//! The EAPOL-Key frame (IEEE Std 802.11-2020 12.7.2): zero-copy parse of a received PDU and a
//! builder for the frames a station sends. Layout is OpenBSD `struct ieee80211_eapol_key`
//! (sys/net80211/ieee80211.h:1122-1167).

use crate::refusal::Refusal;
use wpa_crypto_core::mic;

pub const EAPOL_KEY: u8 = 3; // ieee80211.h:1131
pub const DESC_IEEE80211: u8 = 2; // ieee80211.h:1138
pub const NONCE_LEN: usize = 32; // ieee80211.h:1118
pub const IV_LEN: usize = 16; // ieee80211.h:1119
pub const MIC_LEN: usize = 16; // ieee80211.h:1120

/// Bytes of the EAPOL header (version, type, body length) that the body length does not count.
pub const HDR_LEN: usize = 4;
/// `sizeof(struct ieee80211_eapol_key)`: the fixed part, up to and including the key-data length.
pub const FIXED_LEN: usize = 99;

/// Byte offsets inside an EAPOL-Key PDU.
pub mod off {
    pub const VERSION: usize = 0;
    pub const TYPE: usize = 1;
    pub const BODY_LEN: usize = 2;
    pub const DESC: usize = 4;
    pub const INFO: usize = 5;
    pub const KEY_LEN: usize = 7;
    pub const REPLAY: usize = 9;
    pub const NONCE: usize = 17;
    pub const IV: usize = 49;
    pub const RSC: usize = 65;
    pub const RESERVED: usize = 73;
    pub const MIC: usize = 81;
    pub const DATA_LEN: usize = 97;
    pub const DATA: usize = 99;
}

/// Key Information bits (ieee80211.h:1142-1154).
pub mod info {
    pub const VERSION_MASK: u16 = 0x7; // ieee80211.h:1142
    pub const PAIRWISE: u16 = 1 << 3; // ieee80211.h:1146
    pub const INSTALL: u16 = 1 << 6; // ieee80211.h:1147
    pub const ACK: u16 = 1 << 7; // ieee80211.h:1148
    pub const MIC: u16 = 1 << 8; // ieee80211.h:1149
    pub const SECURE: u16 = 1 << 9; // ieee80211.h:1150
    pub const ERROR: u16 = 1 << 10; // ieee80211.h:1151
    pub const REQUEST: u16 = 1 << 11; // ieee80211.h:1152
    pub const ENCRYPTED: u16 = 1 << 12; // ieee80211.h:1153
    pub const SMK: u16 = 1 << 13; // ieee80211.h:1154
}

/// A received EAPOL-Key PDU, trimmed to the length its header claims.
#[derive(Clone, Copy)]
pub struct KeyFrame<'a> {
    pdu: &'a [u8],
}

impl<'a> KeyFrame<'a> {
    /// Check the framing of `rx` (Ethernet padding after the PDU is allowed and ignored). Mirrors
    /// the length checks of `ieee80211_eapol_key_input` (ieee80211_pae_input.c:75-150).
    pub fn parse(rx: &'a [u8]) -> Result<KeyFrame<'a>, Refusal> {
        if rx.len() < FIXED_LEN {
            return Err(Refusal::Truncated { got: rx.len(), need: FIXED_LEN });
        }
        if rx[off::TYPE] != EAPOL_KEY {
            return Err(Refusal::NotEapolKey(rx[off::TYPE]));
        }
        if rx[off::DESC] != DESC_IEEE80211 {
            return Err(Refusal::NotRsnDescriptor(rx[off::DESC]));
        }
        let body = u16::from_be_bytes([rx[off::BODY_LEN], rx[off::BODY_LEN + 1]]) as usize;
        let total = HDR_LEN + body;
        if total > rx.len() || total < FIXED_LEN {
            return Err(Refusal::BodyLength { claimed: total, have: rx.len() });
        }
        let pdu = &rx[..total];
        let kd = u16::from_be_bytes([pdu[off::DATA_LEN], pdu[off::DATA_LEN + 1]]) as usize;
        if kd > total - FIXED_LEN {
            return Err(Refusal::KeyDataLength { claimed: kd, have: total - FIXED_LEN });
        }
        Ok(KeyFrame { pdu })
    }

    pub fn pdu(&self) -> &'a [u8] {
        self.pdu
    }
    pub fn eapol_version(&self) -> u8 {
        self.pdu[off::VERSION]
    }
    pub fn info(&self) -> u16 {
        u16::from_be_bytes([self.pdu[off::INFO], self.pdu[off::INFO + 1]])
    }
    pub fn descriptor_version(&self) -> u8 {
        (self.info() & info::VERSION_MASK) as u8
    }
    pub fn key_len(&self) -> u16 {
        u16::from_be_bytes([self.pdu[off::KEY_LEN], self.pdu[off::KEY_LEN + 1]])
    }
    pub fn replay(&self) -> u64 {
        let mut b = [0u8; 8];
        b.copy_from_slice(&self.pdu[off::REPLAY..off::REPLAY + 8]);
        u64::from_be_bytes(b)
    }
    pub fn nonce(&self) -> [u8; NONCE_LEN] {
        let mut n = [0u8; NONCE_LEN];
        n.copy_from_slice(&self.pdu[off::NONCE..off::NONCE + NONCE_LEN]);
        n
    }
    /// The Key RSC field as a 48-bit little-endian packet number (the low six bytes).
    pub fn rsc(&self) -> u64 {
        let mut b = [0u8; 8];
        b[..6].copy_from_slice(&self.pdu[off::RSC..off::RSC + 6]);
        u64::from_le_bytes(b)
    }
    pub fn key_data(&self) -> &'a [u8] {
        let kd = u16::from_be_bytes([self.pdu[off::DATA_LEN], self.pdu[off::DATA_LEN + 1]]) as usize;
        &self.pdu[off::DATA..off::DATA + kd]
    }

    /// Verify the MIC over the received bytes (field zeroed during the check).
    pub fn check_mic(&self, kck: &[u8; 16]) -> Result<(), Refusal> {
        match mic::verify(self.descriptor_version(), kck, self.pdu, off::MIC) {
            Ok(()) => Ok(()),
            Err(mic::MicRefusal::Mismatch) => Err(Refusal::MicFailure),
            Err(mic::MicRefusal::UnsupportedVersion(v)) => Err(Refusal::MicUnsupported(v)),
        }
    }
}

/// What a built frame says. The MIC field is left zero for [`stamp_mic`].
pub struct Build<'a> {
    pub eapol_version: u8,
    pub info: u16,
    pub key_len: u16,
    pub replay: u64,
    pub nonce: &'a [u8; NONCE_LEN],
    pub key_data: &'a [u8],
}

/// Serialise into `out`, returning the PDU length.
pub fn build(out: &mut [u8], b: &Build<'_>) -> Result<usize, Refusal> {
    let total = FIXED_LEN + b.key_data.len();
    if out.len() < total {
        return Err(Refusal::TxBufferTooSmall { need: total, have: out.len() });
    }
    out[..total].fill(0);
    out[off::VERSION] = b.eapol_version;
    out[off::TYPE] = EAPOL_KEY;
    out[off::BODY_LEN..off::BODY_LEN + 2].copy_from_slice(&((total - HDR_LEN) as u16).to_be_bytes());
    out[off::DESC] = DESC_IEEE80211;
    out[off::INFO..off::INFO + 2].copy_from_slice(&b.info.to_be_bytes());
    out[off::KEY_LEN..off::KEY_LEN + 2].copy_from_slice(&b.key_len.to_be_bytes());
    out[off::REPLAY..off::REPLAY + 8].copy_from_slice(&b.replay.to_be_bytes());
    out[off::NONCE..off::NONCE + NONCE_LEN].copy_from_slice(b.nonce);
    out[off::DATA_LEN..off::DATA_LEN + 2].copy_from_slice(&(b.key_data.len() as u16).to_be_bytes());
    out[off::DATA..total].copy_from_slice(b.key_data);
    Ok(total)
}

/// Compute the MIC of the built PDU `out[..len]` (MIC field zero) and write it into place.
pub fn stamp_mic(out: &mut [u8], len: usize, kck: &[u8; 16]) -> Result<(), Refusal> {
    let version = (u16::from_be_bytes([out[off::INFO], out[off::INFO + 1]]) & info::VERSION_MASK) as u8;
    match mic::compute(version, kck, &out[..len]) {
        Ok(m) => {
            out[off::MIC..off::MIC + MIC_LEN].copy_from_slice(&m);
            Ok(())
        }
        Err(mic::MicRefusal::UnsupportedVersion(v)) => Err(Refusal::MicUnsupported(v)),
        Err(mic::MicRefusal::Mismatch) => Err(Refusal::MicFailure),
    }
}
