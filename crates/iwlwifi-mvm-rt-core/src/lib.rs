// SPDX-License-Identifier: GPL-2.0-only
//! The RT-image (REGULAR) half of the iwlwifi MVM op-mode for the Wireless-AC 9260: the host-command encoders of
//! `iwl_mvm_up` (BT, SoC latency, DQA, aux station, device power, MCC, temperature thresholds, scan config), the paging
//! plan and `FW_PAGING_BLOCK_CMD`, the passive `SCAN_REQ_UMAC` (the v8 layout this firmware takes), the scan-complete and
//! RX_MPDU decoders, and the beacon / probe-response walk that turns a received frame into an (SSID, BSSID, channel,
//! RSSI, security) result.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/mvm/fw.c` (`iwl_mvm_up`), `mvm/scan.c`, `mvm/rxmq.c`,
//! `mvm/nvm.c` (`iwl_mvm_update_mcc`), `mvm/sta.c` (`iwl_mvm_add_aux_sta`), `mvm/coex.c`, `mvm/power.c`, `mvm/tt.c`,
//! `fw/paging.c`, `fw/dump.c` and `fw/api/{scan,rx,sta,tx,txq,coex,power,phy,system,nvm-reg,paging,commands}.h`
//! (Linux v7.0-rc6; `references/linux-ref`). The IEEE 802.11 element walk follows IEEE 802.11-2020 9.4.2 and the
//! RSN/WPA element layouts of 9.4.2.24 and the WFA WPA vendor element.
//!
//! Copyright (C) 2012-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile Communications GmbH;
//! Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are `GPL-2.0 OR BSD-3-Clause`; this port is
//! distributed under GPL-2.0-only.
//!
//! No MMIO, no allocation, no I/O. Left out on purpose (Linux does it, this crate does not): the smart-FIFO command
//! (`iwl_mvm_sf_update`; the firmware advertises no SMART_FIFO_OFFLOAD, so Linux sends an async SF_CFG_CMD with
//! state INIT_OFF), LARI/PPAG/SAR/GEO/SGOM/TAS/UATS/RFI configuration (BIOS tables a Chromebook lacks), the LTR
//! command (`ltr_enabled` is read from PCI config space, which is the config authority's), RSS/RXQ configuration
//! (one RX queue: `iwl_send_rss_cfg_cmd` and `iwl_configure_rxq` return early), PHY contexts and MAC contexts
//! (Linux adds them when an interface is added, not in `iwl_mvm_up`), scheduled scan, 6 GHz scan.

#![no_std]
#![forbid(unsafe_code)]

pub mod decode;
pub mod encode;
pub mod frame;
pub mod ids;
pub mod paging;
pub mod phydb;
pub mod scanreq;
