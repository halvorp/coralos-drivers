// SPDX-License-Identifier: GPL-2.0-only
//! The iwlwifi gen1 PCIe transport's power-up and power-down register sequences for the
//! IWL_DEVICE_FAMILY_9000 discrete parts (Wireless-AC 9260, PCI 8086:2526), MSI mode.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c`
//! (`iwl_pcie_prepare_card_hw`, `iwl_pcie_apm_init`, `iwl_pcie_apm_stop`, `iwl_trans_pcie_sw_reset`,
//! `iwl_trans_pcie_clear_persistence_bit`, `iwl_pcie_conf_msix_hw`, `_iwl_trans_pcie_start_hw`,
//! `_iwl_trans_pcie_stop_device`, `iwl_pcie_gen1_2_activate_nic`), `pcie/gen1_2/tx.c`
//! (`iwl_pcie_tx_init`, `iwl_pcie_tx_stop`), `pcie/gen1_2/rx.c` (`iwl_pcie_rx_stop`) and
//! `pcie/gen1_2/internal.h`, with registers from `iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h` and
//! `iwl-trans.h`, and the family parameters from `cfg/9000.c` (Linux v7.0-rc6). OpenBSD
//! `sys/dev/pci/if_iwm.c` (`iwm_start_hw`, `iwm_stop_device`, `iwm_nic_tx_init`,
//! `iwm_conf_msix_hw`) was read as a second opinion on the 9000-family order and agrees.
//!
//! Copyright (C) 2007-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! FAMILY BRANCHES taken (cfg/9000.c:29-75): `apmg_not_supported` (no APMG clock writes),
//! `mq_rx_supported` (RFH RX, the MSI UREG_CHICK select), not `gen2`, not `integrated`, below
//! IWL_DEVICE_FAMILY_BZ (CSR_RESET carries the SW reset), `num_of_queues` 31 (> 20: ENABLE_31_QUEUES),
//! no `lp_xtal_workaround`, no `host_interrupt_operation_mode`. MSI-X is not modelled.
//!
//! The crate performs no MMIO: every access goes through the [`io::TransIo`] seam, so a scripted
//! register file tests each sequence.

#![no_std]
#![forbid(unsafe_code)]

pub mod apm;
pub mod hw;
pub mod io;
pub mod regs;
pub mod tx_init;
