// SPDX-License-Identifier: GPL-2.0-only
//! What the receive descriptor says about encryption, and the CCMP replay check the host still
//! owes: the firmware decrypts and strips the MIC, mac80211 (here: this module) checks the packet
//! number. Ported from `mvm/rxmq.c` (`iwl_mvm_rx_crypto`, CCM branch) and `net/mac80211/wpa.c`
//! (`ieee80211_crypto_ccmp_decrypt`, the PN comparison).

use crate::ids::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxSec {
    /// The frame was not encrypted (or is not a kind that is).
    Clear,
    /// CCMP, decrypted by the firmware, MIC verified; the 8-byte CCMP header is still in the frame.
    CcmpDecrypted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxSecRefusal {
    /// CCMP but the firmware did not verify the MIC: "Dropping packet, bad MIC (CCM/GCM)".
    BadMic,
    /// The firmware reports an algorithm other than none or CCMP; carries the 3-bit code.
    Algorithm(u32),
    /// A protected frame the firmware did not decrypt (no key yet, or the key is not valid).
    NotDecrypted,
}

/// `status` is the descriptor's status word; `protected` the Protected bit of the frame.
/// // iwl_mvm_rx_crypto, mvm/rxmq.c
pub fn rx_security(status: u32, protected: bool) -> Result<RxSec, RxSecRefusal> {
    let sec = status & IWL_RX_MPDU_STATUS_SEC_MASK;
    if !protected || sec == IWL_RX_MPDU_STATUS_SEC_NONE {
        return if protected { Err(RxSecRefusal::NotDecrypted) } else { Ok(RxSec::Clear) };
    }
    if sec == IWL_RX_MPDU_STATUS_SEC_CCM {
        return if status & IWL_RX_MPDU_STATUS_MIC_OK != 0 { Ok(RxSec::CcmpDecrypted) } else { Err(RxSecRefusal::BadMic) };
    }
    Err(RxSecRefusal::Algorithm(sec >> 8))
}

/// Per-key CCMP replay window: a packet number is accepted only if it is strictly above the last
/// accepted one (IEEE 802.11-2020 12.5.3.4.4). One window per key (pairwise; each group key id).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PnReplay {
    last: Option<u64>,
}

impl PnReplay {
    /// Start a window at the RSC the key was installed with: that value itself is already used.
    pub fn from_rsc(rsc: u64) -> PnReplay {
        PnReplay { last: Some(rsc) }
    }

    pub fn accept(&mut self, pn: u64) -> bool {
        match self.last {
            Some(l) if pn <= l => false,
            _ => {
                self.last = Some(pn);
                true
            }
        }
    }
}
