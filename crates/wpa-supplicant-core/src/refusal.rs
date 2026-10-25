// SPDX-License-Identifier: ISC
//! Every way a frame or a step is refused, by name, with the numbers that decided it.

use wpa_crypto_core::keywrap::WrapRefusal;

/// What the caller does about a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// The frame is dropped; the handshake carries on and the authenticator retransmits.
    Drop,
    /// The handshake is over; the station deauthenticates with [`Refusal::deauth_reason`].
    Fatal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    // ---- framing ----
    /// Fewer bytes than the fixed EAPOL-Key header.
    Truncated { got: usize, need: usize },
    /// EAPOL packet type other than EAPOL-Key (3).
    NotEapolKey(u8),
    /// Key descriptor type other than RSN (2).
    NotRsnDescriptor(u8),
    /// The EAPOL body length claims more bytes than arrived.
    BodyLength { claimed: usize, have: usize },
    /// The key-data length claims more bytes than the body holds.
    KeyDataLength { claimed: usize, have: usize },
    /// Key data longer than this station buffers.
    KeyDataTooLong { len: usize, max: usize },
    /// Descriptor version outside 1..=3.
    UnknownDescriptorVersion(u8),
    /// Descriptor version not the one the negotiated AKM/cipher requires.
    WrongDescriptorVersion { got: u8, want: u8 },
    /// An EAPOL-Key Request frame: only a station sends those.
    RequestFrame,
    // ---- sequencing ----
    /// A message that does not belong in the current state (or is not one a station receives).
    Unexpected { state: u8, msg: u8 },
    /// Replay counter not strictly above the last accepted.
    ReplayCounter { got: u64, last: u64 },
    /// The MIC did not verify.
    MicFailure,
    /// The MIC could not be checked: descriptor version unsupported.
    MicUnsupported(u8),
    /// Message 3's ANonce differs from message 1's.
    AnonceChanged,
    /// The AP retransmitted message 1 more times than this station answers.
    TooManyMessage1 { answered: u8 },
    /// The handshake did not finish in time.
    Timeout { waited_ms: u64, limit_ms: u64 },
    // ---- key data ----
    /// The AES key-unwrap of the key data failed.
    KeyDataDecrypt(WrapRefusal),
    /// A PMKID KDE of the wrong length.
    PmkidKde { len: usize },
    /// Message 3 carried no RSNE.
    MissingRsne,
    /// Message 3's RSNE is not byte-identical to the AP's beacon / probe-response RSNE.
    RsneMismatch,
    /// A second RSNE in message 3 names a different AKM, group cipher or a pairwise cipher we refuse.
    RsneDowngrade,
    /// An RSNE that does not parse.
    RsneMalformed,
    /// Message 3 lacks the Install bit although a pairwise cipher is in use.
    MissingInstall,
    /// Key Length field of message 3 disagrees with the pairwise cipher.
    KeyLength { got: u16, want: u16 },
    /// A GTK KDE arrived in key data that was not encrypted.
    GtkNotEncrypted,
    /// Group message 1 carried no GTK KDE, or key data that was not encrypted.
    GtkMissing,
    /// GTK KDE length disagrees with the group cipher.
    GtkLength { got: usize, want: usize },
    /// The driver refused to install a key.
    InstallFailed,
    /// A buffer the caller gave is too small for the frame.
    TxBufferTooSmall { need: usize, have: usize },
    /// A configured RSNE that is longer than an information element can be, or malformed.
    BadConfig,
}

impl Refusal {
    pub fn severity(&self) -> Severity {
        use Refusal::*;
        match self {
            TooManyMessage1 { .. } | Timeout { .. } | RsneMismatch | RsneDowngrade | KeyLength { .. }
            | GtkLength { .. } | InstallFailed => Severity::Fatal,
            _ => Severity::Drop,
        }
    }

    /// The IEEE 802.11-2020 Table 9-49 reason code to deauthenticate with, for a fatal refusal.
    pub fn deauth_reason(&self) -> Option<u16> {
        use Refusal::*;
        match self {
            Timeout { .. } | TooManyMessage1 { .. } => Some(REASON_4WAY_TIMEOUT),
            RsneMismatch => Some(REASON_RSN_DIFFERENT_IE),
            RsneDowngrade => Some(REASON_BAD_PAIRWISE_CIPHER),
            KeyLength { .. } | GtkLength { .. } | InstallFailed => Some(REASON_AUTH_LEAVE),
            _ => None,
        }
    }
}

pub const REASON_AUTH_LEAVE: u16 = 3; // ieee80211.h:955
pub const REASON_4WAY_TIMEOUT: u16 = 15; // ieee80211.h:969
pub const REASON_RSN_DIFFERENT_IE: u16 = 17; // ieee80211.h:971
pub const REASON_BAD_PAIRWISE_CIPHER: u16 = 19; // ieee80211.h:973
