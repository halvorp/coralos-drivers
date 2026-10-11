// SPDX-License-Identifier: GPL-2.0-only
//! The DMA halves of bring-up and stop on the gen1 transport: `iwl_pcie_tx_init`'s register writes
//! (scheduler FIFOs off, the keep-warm buffer, each queue's TFD ring base, SCD_GP_CTRL),
//! `iwl_pcie_tx_stop` and the multi-queue `iwl_pcie_rx_stop`. Ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/tx.c:576-618, :938-996`,
//! `drivers/net/wireless/intel/iwlwifi/pcie/gen1_2/rx.c:145-162` and
//! `drivers/net/wireless/intel/iwlwifi/iwl-scd.h:41-44`.
//!
//! Copyright (C) 2003-2014, 2018-2021, 2023-2025 Intel Corporation; Copyright (C) 2013-2015 Intel
//! Mobile Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.
//!
//! The rings themselves (allocation, TFD contents, `iwl_txq_init`'s host state) are the caller's: this
//! module takes their bus addresses and validates them BEFORE the first write, so a bad address
//! refuses with nothing programmed.

use crate::io::{poll_bits, poll_prph_bit, set_bits_prph, TransIo};
use crate::regs::*;

/// tx_stop_fh's idle wait. // tx.c:594
pub const TX_STOP_FH_TIMEOUT_US: u32 = 5000; // tx.c:594
/// rx_stop's idle wait. // rx.c:154-155
pub const RX_STOP_TIMEOUT_US: u32 = 1000; // rx.c:155
/// The gen1 TFD ring base is programmed `>> 8` into a 32-bit register: 256-byte aligned, below 2^40.
pub const TFD_RING_ALIGN: u64 = 256;
/// The keep-warm base is programmed `>> 4`: 16-byte aligned, below 2^36. // tx.c:958-959
pub const KW_ALIGN: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxInitError {
    /// Linux programs one ring per `num_of_queues` (31, cfg/9000.c:31). // tx.c:964-965
    QueueCount { got: usize, need: u32 },
    /// The keep-warm address does not survive `>> 4` into 32 bits, or is not 16-byte aligned.
    KeepWarm { dma: u64 },
    /// A TFD ring address does not survive `>> 8` into 32 bits, or is not 256-byte aligned.
    TfdRing { queue: u32, dma: u64 },
}

/// The register half of `iwl_pcie_tx_init` (tx.c:938-996) for 31 gen1 queues.
pub fn tx_init<I: TransIo>(io: &mut I, kw_dma: u64, tfd_rings: &[u64]) -> Result<(), TxInitError> {
    if tfd_rings.len() != IWL9000_NUM_OF_QUEUES as usize {
        return Err(TxInitError::QueueCount { got: tfd_rings.len(), need: IWL9000_NUM_OF_QUEUES });
    }
    if kw_dma % KW_ALIGN != 0 || kw_dma >> 36 != 0 {
        return Err(TxInitError::KeepWarm { dma: kw_dma });
    }
    for (q, &dma) in tfd_rings.iter().enumerate() {
        if dma % TFD_RING_ALIGN != 0 || dma >> 40 != 0 || fh_mem_cbbc_queue(q as u32).is_none() {
            return Err(TxInitError::TfdRing { queue: q as u32, dma });
        }
    }
    io.prph_write(SCD_TXFACT, 0); // iwl_scd_deactivate_fifos, iwl-scd.h:41-44; tx.c:955
    io.csr_write(FH_KW_MEM_ADDR_REG, (kw_dma >> 4) as u32); // tx.c:958-959
    for (q, &dma) in tfd_rings.iter().enumerate() {
        if let Some(reg) = fh_mem_cbbc_queue(q as u32) {
            io.csr_write(reg, (dma >> 8) as u32); // tx.c:987-988 (every q was checked above)
        }
    }
    set_bits_prph(io, SCD_GP_CTRL, SCD_GP_CTRL_AUTO_ACTIVE_MODE); // tx.c:991
    if IWL9000_NUM_OF_QUEUES > 20 {
        set_bits_prph(io, SCD_GP_CTRL, SCD_GP_CTRL_ENABLE_31_QUEUES); // tx.c:992-994
    }
    Ok(())
}

/// `iwl_pcie_tx_stop` (tx.c:609-618) with `iwl_pcie_tx_stop_fh` (tx.c:576-604): FIFOs off, every TX
/// DMA channel off, then wait for all eight idle bits. Linux only logs a timeout (tx.c:595-598);
/// the port returns `Err(status)` with the FH_TSSR_TX_STATUS_REG value so the caller can log it.
pub fn tx_stop<I: TransIo>(io: &mut I) -> Result<(), u32> {
    io.prph_write(SCD_TXFACT, 0); // tx.c:615
    let mut mask = 0;
    for ch in 0..FH_TCSR_CHNL_NUM {
        io.csr_write(fh_tcsr_chnl_tx_config_reg(ch), 0); // tx.c:589
        mask |= fh_tssr_tx_status_reg_msk_chnl_idle(ch); // tx.c:590
    }
    if poll_bits(io, FH_TSSR_TX_STATUS_REG, mask, TX_STOP_FH_TIMEOUT_US) {
        Ok(())
    } else {
        Err(io.csr_read(FH_TSSR_TX_STATUS_REG)) // tx.c:595-598
    }
}

/// `iwl_pcie_rx_stop`, the mq_rx_supported (< AX210) arm: RX DMA off, wait for RXF_DMA_IDLE.
/// `Err(status)` carries RFH_GEN_STATUS on a timeout. // rx.c:152-155
pub fn rx_stop<I: TransIo>(io: &mut I) -> Result<(), u32> {
    io.prph_write(RFH_RXF_DMA_CFG, 0); // rx.c:153
    if poll_prph_bit(io, RFH_GEN_STATUS, RXF_DMA_IDLE, RXF_DMA_IDLE, RX_STOP_TIMEOUT_US) {
        Ok(())
    } else {
        Err(io.prph_read(RFH_GEN_STATUS))
    }
}
