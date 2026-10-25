// SPDX-License-Identifier: ISC
//! The seams the driver implements: key installation and entropy. Counterpart of OpenBSD's
//! `ic_set_key` callback (ieee80211_pae_input.c:560, :583, :613); on iwlwifi the three calls
//! become `ADD_STA_KEY` commands (mvm/sta.c `iwl_mvm_send_sta_key`, `__iwl_mvm_set_sta_key`).

use crate::rsne::Cipher;

/// The pairwise key to install for the AP station.
pub struct PairwiseKey<'a> {
    pub cipher: Cipher,
    /// The temporal key (16 bytes for CCMP).
    pub tk: &'a [u8],
    /// Receive sequence counter to start the replay window from (message 3's Key RSC).
    pub rsc: u64,
}

/// A group key for broadcast / multicast reception.
pub struct GroupKey<'a> {
    pub cipher: Cipher,
    /// Key index 0..=3.
    pub key_id: u8,
    pub key: &'a [u8],
    /// The AP transmits with this key (the GTK KDE's Tx bit).
    pub tx: bool,
    pub rsc: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallError {
    /// The device refused or failed the key command.
    Refused,
}

/// Implemented by the driver. Each call returns once the key command is ISSUED in order with the
/// driver's other commands; a later failure is reported with `Supplicant::fail_install`.
pub trait KeyInstaller {
    fn install_ptk(&mut self, key: &PairwiseKey<'_>) -> Result<(), InstallError>;
    fn install_gtk(&mut self, key: &GroupKey<'_>) -> Result<(), InstallError>;
    /// Called once, after the first successful install of a complete key set: the controlled port
    /// opens and data frames may flow (OpenBSD `ni_port_valid = 1`, ieee80211_pae_input.c:663).
    fn open_port(&mut self);
}

/// Cryptographically strong random bytes for the SNonce.
pub trait Entropy {
    fn fill(&mut self, out: &mut [u8]);
}
