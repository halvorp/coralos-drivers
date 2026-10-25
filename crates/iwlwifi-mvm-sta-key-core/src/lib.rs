// SPDX-License-Identifier: GPL-2.0-only
//! The station-key half of the iwlwifi MVM op-mode: the `ADD_STA_KEY` host command that installs a
//! CCMP pairwise or group key for the AP station, the firmware key-slot table, and the command's
//! response status. This is what a supplicant's `install_ptk` / `install_gtk` become on a 9260.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/mvm/sta.c` (`iwl_mvm_send_sta_key`
//! 3532-3686, `iwl_mvm_set_fw_key_idx` 3455-3477, `iwl_mvm_set_sta_key` 3841-3935,
//! `iwl_mvm_remove_sta_key` 3943-3990, `__iwl_mvm_remove_sta_key` 2613), `fw/api/sta.h`
//! (`struct iwl_mvm_add_sta_key_cmd_v1` 379, key flags 122-140, status 413),
//! `fw/api/commands.h` (`ADD_STA_KEY` 134) and mac80211 `key.c` (`ieee80211_get_key_rx_seq`
//! 1254, PN byte order `include/net/mac80211.h:2307`; Linux v7.0-rc6).
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2014 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2016 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! Which command layout: the Atlas firmware (`iwlwifi-9260-th-b0-jf-b0-46.ucode`) lists
//! `ADD_STA_KEY` at command version **1** in its `CMD_VERSIONS` TLV (read from the file), so the
//! encoder emits `ADD_MODIFY_STA_KEY_API_S_VER_1` (64 bytes) with the "hole" PN format. The
//! version is a parameter of [`encode::ccmp`] so a firmware that reports 2 or 3 is a one-line
//! caller change, not a rewrite; only CCMP is ported (TKIP/WEP/GCMP/IGTK are not offered by the
//! supplicant crate).
//!
//! No MMIO, no allocation: the caller sends the bytes as an `ADD_STA_KEY` host command and feeds
//! the response word to [`decode::status`].

#![no_std]
#![forbid(unsafe_code)]

pub mod decode;
pub mod encode;
pub mod ids;
pub mod slots;
