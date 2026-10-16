// SPDX-License-Identifier: GPL-2.0-only
//! The gen1 PCIe transport's rings for the Wireless-AC 9260 (IWL_DEVICE_FAMILY_9000, multi-queue RX,
//! one RX queue, MSI): the RFH receive queue (free-descriptor table, used-descriptor table, status
//! block, the packets inside a receive buffer), the TX scheduler start-up and the host-command queue
//! (queue 0, FIFO 7: its TFD, its command slot, its write pointer, its reclaim).
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/rx.c`
//! (`iwl_pcie_rx_mq_hw_init`, `iwl_pcie_restock_bd`, `iwl_pcie_rxmq_restock`, `iwl_pcie_rxq_inc_wr_ptr`,
//! `iwl_pcie_get_rxb`, `iwl_pcie_rx_handle_rb`, `iwl_pcie_rx_handle`), `pcie/gen1_2/tx.c`
//! (`iwl_pcie_tx_start`, `iwl_trans_pcie_txq_enable`, `iwl_pcie_enqueue_hcmd`,
//! `iwl_pcie_hcmd_complete`, `iwl_pcie_cmdq_reclaim`, `iwl_pcie_txq_inc_wr_ptr`,
//! `iwl_pcie_gen1_tfd_set_tb`), `pcie/gen1_2/tx-gen2.c` (`iwl_txq_space`), `pcie/gen1_2/internal.h`,
//! with registers from `iwl-fh.h`, `iwl-prph.h`, `iwl-scd.h`, `iwl-trans.h`, `fw/api/cmdhdr.h`,
//! `fw/api/txq.h`, `cfg/9000.c` and `iwl-config.h` (Linux v7.0-rc6).
//!
//! Copyright (C) 2003-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! The crate owns the register programs and the index arithmetic; the rings' MEMORY is the caller's.
//! Every function that touches memory takes a byte slice, so a host test hands it a `Vec` and the
//! device hands it a DMA mapping.

#![no_std]
#![forbid(unsafe_code)]

pub mod rx;
pub mod rxregs;
pub mod tx;
pub mod txregs;
