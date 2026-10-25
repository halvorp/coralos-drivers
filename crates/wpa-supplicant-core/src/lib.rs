// SPDX-License-Identifier: ISC
//! The station side of the IEEE 802.11 RSN key handshakes: the 4-way handshake (supplicant) and
//! the group-key handshake, for WPA2-Personal (AKM 00-0F-AC:2, CCMP, key descriptor version 2) and
//! the post-authentication half of WPA2-PSK-SHA256 / WPA3-SAE (AKM :6 / :8, descriptor version 3,
//! given a PMK from elsewhere).
//!
//! Ported from OpenBSD `sys/net80211/ieee80211_pae_input.c` (`ieee80211_eapol_key_input`,
//! `ieee80211_recv_4way_msg1`, `_msg3`, `ieee80211_recv_rsn_group_msg1`,
//! `ieee80211_must_update_group_key`) and `ieee80211_pae_output.c` (`ieee80211_send_4way_msg2`,
//! `_msg4`, `ieee80211_send_group_msg2`), which implement IEEE Std 802.11-2020 12.7.6 and 12.7.7.
//! Copyright (c) 2007, 2008 Damien Bergamini; ISC licence, see LICENSE-OpenBSD-ISC. The OpenBSD
//! code is the in-tree reference for the supplicant because the Linux kernel runs none: Linux
//! leaves the handshake to the `wpa_supplicant` daemon and only installs keys (mac80211
//! `ieee80211_key_link`, iwlwifi `iwl_mvm_set_sta_key` / `ADD_STA_KEY`).
//!
//! No MMIO, no allocation, no I/O, no clock and no entropy of its own: the caller passes received
//! EAPOL PDUs, a transmit buffer, a millisecond timestamp and an [`Entropy`] source, and the
//! driver implements [`KeyInstaller`]. WEP, TKIP pairwise, the WPA1 descriptor and descriptor
//! version 1 (HMAC-MD5 / RC4) are refused by name, not implemented.
//!
//! Deliberate departures from the OpenBSD code, each pinned by a test:
//!  - the MIC is checked over the received bytes with the MIC field zeroed, never over a
//!    re-encoded frame;
//!  - the PTK is installed only after the driver reports message 4 transmitted
//!    ([`Supplicant::tx_done`]), so message 4 never leaves under the key it announces;
//!  - an unchanged ANonce reuses the SNonce, so a retransmitted message 1 does not orphan the
//!    message 2 the authenticator already holds;
//!  - the 4-way handshake and the number of answered message 1s are bounded.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(feature = "testkit")]
extern crate std;

pub mod frame;
pub mod install;
pub mod kde;
pub mod refusal;
pub mod rsne;
pub mod supplicant;
#[cfg(feature = "testkit")]
pub mod testkit;

pub use install::{Entropy, GroupKey, InstallError, KeyInstaller, PairwiseKey};
pub use refusal::{Refusal, Severity};
pub use rsne::{Akm, Cipher};
pub use supplicant::{Config, Reply, State, Supplicant};
