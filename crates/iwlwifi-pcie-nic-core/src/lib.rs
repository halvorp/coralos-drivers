// SPDX-License-Identifier: GPL-2.0-only
//! What sits between `iwlwifi-pcie-trans-core`'s power sequences and a running firmware on the
//! Wireless-AC 9260 (IWL_DEVICE_FAMILY_9000, gen1 transport, MSI): the NIC-access handshake and the
//! indirect periphery / SRAM accessors over a raw BAR0 seam, the interrupt masks and cause decode,
//! the firmware section upload, and the PHY/shadow-register NIC configuration.
//!
//! Ported from Linux `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/trans.c`
//! (`_iwl_trans_pcie_grab_nic_access`, `iwl_trans_pcie_release_nic_access`,
//! `iwl_trans_pcie_read_prph` / `_write_prph`, `iwl_pcie_load_firmware_chunk_fh`,
//! `iwl_pcie_load_firmware_chunk`, `iwl_pcie_load_section`, `iwl_pcie_load_cpu_sections_8000`,
//! `iwl_pcie_load_given_ucode_8000`, `iwl_pcie_nic_init`), `pcie/gen1_2/internal.h` (the interrupt
//! enable/disable helpers), `pcie/gen1_2/rx.c` (`iwl_pcie_irq_handler`'s cause decode),
//! `mvm/ops.c` (`iwl_mvm_nic_config`), `iwl-io.c` (`iwl_poll_bits_mask`, the `*_prph` and `*_direct32`
//! accessors), `iwl-trans.c` (`iwl_trans_write_mem`, `iwl_trans_read_mem`), with registers from
//! `iwl-csr.h`, `iwl-fh.h`, `iwl-prph.h` (Linux v7.0-rc6).
//!
//! Copyright (C) 2003-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH. The Linux files are
//! `GPL-2.0 OR BSD-3-Clause`; this port is distributed under GPL-2.0-only.
//!
//! FAMILY BRANCHES taken: 9000 (>= 8000, < BZ): the MAC_ACCESS_REQ handshake with its 2 us settle,
//! `device_family < AX210` nic_config, `gen2` off (the FH_UCODE_LOAD_STATUS CSR, not UREG), `msix`
//! off. No I/O here: everything goes through [`bus::RawBus`] or the caller's [`load::ChunkSink`].

#![no_std]
#![forbid(unsafe_code)]

pub mod bus;
pub mod irq;
pub mod load;
pub mod nicinit;
pub mod regs;
