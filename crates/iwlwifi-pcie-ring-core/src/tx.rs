// SPDX-License-Identifier: GPL-2.0-only
//! TX scheduler start-up and the host-command queue (queue 0, FIFO 7) of the gen1 transport.
//! Ported from Linux `pcie/gen1_2/tx.c:78-141` (`iwl_pcie_txq_inc_wr_ptr`), `:144-191`
//! (`iwl_pcie_gen1_tfd_set_tb`, `iwl_pcie_txq_build_tfd`), `:480-534` (`iwl_pcie_tx_start`),
//! `:1004-1033` (`iwl_pcie_set_cmd_in_flight`), `:1036-1146` (`iwl_txq_progress`, `iwl_txq_used`,
//! `iwl_pcie_cmdq_reclaim`), `:1147-1257` (`iwl_trans_pcie_txq_enable`), `:1343-1612`
//! (`iwl_pcie_enqueue_hcmd`), `:1621-1680` (`iwl_pcie_hcmd_complete`), `pcie/gen1_2/tx-gen2.c:527-555`
//! (`iwl_txq_space`), `pcie/gen1_2/internal.h:658-715`, `iwl-trans.h:975-1000`
//! (`iwl_trans_ac_txq_enable`), `iwl-scd.h` (the scheduler helpers).
//!
//! Copyright (C) 2003-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! Family branches taken: `shadow_reg_enable` (so `iwl_pcie_txq_inc_wr_ptr` writes the doorbell with no
//! wake-up dance, tx.c:90-101), `apmg_wake_up_wa` unset (`iwl_pcie_set_cmd_in_flight` is a no-op,
//! tx.c:1011-1013), `scd_chain_ext_wa` unset, family >= 8000 (no L1-active clear, tx.c:531).
//!
//! WHAT IS LEFT OUT, by name: data queues (`iwl_trans_pcie_tx`, TSO, byte-count tables per frame),
//! aggregation, the stuck-queue watchdog timer, queue freezing, the DUP flag (a command chunk the
//! transport duplicates for the caller). Only the command queue is driven.

use crate::txregs::*;
use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo};
use iwlwifi_pcie_nic_core::regs::{
    fh_tcsr_chnl_tx_config_reg, FH_TCSR_CHNL_NUM, FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_ENABLE, FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_ENABLE,
    FH_TX_CHICKEN_BITS_REG, FH_TX_CHICKEN_BITS_SCD_AUTO_RETRY_EN, HBUS_TARG_WRPTR,
};

/// `iwl_pcie_gen1_tfd_set_tb` + `iwl_pcie_txq_build_tfd`: append one transfer buffer to a TFD.
/// `tfd` is the 128-byte slot; `reset` zeroes it first (the first TB of a command). Returns the TB
/// index used. // tx.c:144-156, :163-191
pub fn tfd_add_tb(tfd: &mut [u8], addr: u64, len: u16, reset: bool) -> Result<usize, TxError> {
    if tfd.len() < TFD_SIZE {
        return Err(TxError::Buf { len: tfd.len() });
    }
    if reset {
        tfd[..TFD_SIZE].fill(0); // tx.c:172-173
    }
    let num_tbs = (tfd[3] & 0x1f) as usize; // iwl_txq_gen1_tfd_get_num_tbs, tx.c:158-161, :175
    if num_tbs >= IWL_NUM_OF_TBS {
        return Err(TxError::TooManyTbs); // "Error can not send more than %d chunks", tx.c:178-182
    }
    if addr >> IWL_TX_DMA_BITS != 0 {
        return Err(TxError::AddrTooWide { addr }); // tx.c:184-186
    }
    if len > 0x0FFF {
        return Err(TxError::TbTooLong { len }); // hi_n_len holds the length in bits 15:4
    }
    let o = 4 + num_tbs * 6; // struct iwl_tfd: __reserved1[3], num_tbs, tbs[]
    tfd[o..o + 4].copy_from_slice(&(addr as u32).to_le_bytes()); // put_unaligned_le32(addr, &tb->lo), tx.c:150
    let hi_n_len = (len << 4) | iwlwifi_pcie_nic_core::load::dma_hi_addr(addr) as u16; // tx.c:145, :151
    tfd[o + 4..o + 6].copy_from_slice(&hi_n_len.to_le_bytes()); // tx.c:153
    tfd[3] = (num_tbs + 1) as u8; // tfd->num_tbs = idx + 1, tx.c:155
    Ok(num_tbs)
}

/// The (address, length) of TB `idx` of a TFD, and the TB count. // tx.c:158-161, :259-275
pub fn tfd_tb(tfd: &[u8], idx: usize) -> Option<(u64, u16)> {
    if tfd.len() < TFD_SIZE || idx >= (tfd[3] & 0x1f) as usize {
        return None;
    }
    let o = 4 + idx * 6;
    let lo = u32::from_le_bytes([tfd[o], tfd[o + 1], tfd[o + 2], tfd[o + 3]]) as u64;
    let hi_n_len = u16::from_le_bytes([tfd[o + 4], tfd[o + 5]]);
    Some((lo | ((hi_n_len & 0xF) as u64) << 32, hi_n_len >> 4))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxError {
    Buf { len: usize },
    TooManyTbs,
    AddrTooWide { addr: u64 },
    TbTooLong { len: u16 },
    /// "unsupported wide command". // tx.c:1363-1366 (wide_cmd_header is true on mvm, so unreachable)
    WideCmd { group: u8 },
    /// "Command %s (%#x) is too large (%d bytes)". // tx.c:1431-1437
    TooLarge { copy_size: usize },
    /// A chunk after a NOCOPY chunk that is not itself NOCOPY. // tx.c:1411-1413
    CopyAfterNocopy,
    /// "No space in command queue". // tx.c:1446-1455
    QueueFull,
    /// The command queue has not been started (`tx_start`) yet.
    Nic(NicFault),
}

impl From<NicFault> for TxError {
    fn from(f: NicFault) -> Self {
        TxError::Nic(f)
    }
}

/// `iwl_pcie_tx_start` for the 9000 family with the mvm command queue. Returns the scheduler's SRAM
/// base (`scd_base_addr`). // tx.c:480-534
pub fn tx_start<I: NicIo>(io: &mut I, scd_bc_tbls_dma: u64) -> Result<u32, TxError> {
    let nq = IWL9000_NUM_OF_QUEUES; // tx.c:483
    let clear_dwords = (scd_trans_tbl_offset_queue(nq) - SCD_CONTEXT_MEM_LOWER_BOUND) / 4; // tx.c:486-487
    // (queue_stopped / queue_used are cleared by the caller's Cmdq::new, tx.c:490-493)
    let scd_base_addr = io.prph_read(SCD_SRAM_BASE_ADDR); // tx.c:495
    // reset context data, TX status and translation data. // tx.c:498-501
    io.zero_mem(scd_base_addr + SCD_CONTEXT_MEM_LOWER_BOUND, clear_dwords as usize)?;
    io.prph_write(SCD_DRAM_BASE_ADDR, (scd_bc_tbls_dma >> 10) as u32); // tx.c:503-504
    // (scd_chain_ext_wa is unset on 9000: SCD_CHAINEXT_EN is not written, tx.c:509-510)
    cmd_queue_enable(io, scd_base_addr)?; // iwl_trans_ac_txq_enable, tx.c:512-514
    io.prph_write(SCD_TXFACT, 0xFF); // iwl_scd_activate_fifos: IWL_MASK(0, 7), tx.c:517
    // Enable DMA channel. // tx.c:519-523
    for chan in 0..FH_TCSR_CHNL_NUM {
        io.write_direct32(
            fh_tcsr_chnl_tx_config_reg(chan),
            FH_TCSR_TX_CONFIG_REG_VAL_DMA_CHNL_ENABLE | FH_TCSR_TX_CONFIG_REG_VAL_DMA_CREDIT_ENABLE,
        );
    }
    // Update FH chicken bits. // tx.c:525-528
    let reg_val = io.read_direct32(FH_TX_CHICKEN_BITS_REG);
    io.write_direct32(FH_TX_CHICKEN_BITS_REG, reg_val | FH_TX_CHICKEN_BITS_SCD_AUTO_RETRY_EN);
    Ok(scd_base_addr)
}

/// `iwl_trans_ac_txq_enable(trans, cmd_queue, cmd_fifo, wd)` → `iwl_trans_pcie_txq_enable` with
/// `cfg = {fifo 7, sta -1, aggregate false, frame_limit 64}` and `ssn = 0`: the queue starts at read =
/// write = 0. // iwl-trans.h:987-1000; tx.c:1147-1257
pub fn cmd_queue_enable<I: NicIo>(io: &mut I, scd_base_addr: u32) -> Result<(), TxError> {
    let q = IWL_MVM_DQA_CMD_QUEUE;
    let fifo = IWL_MVM_TX_FIFO_CMD;
    // Disable the scheduler prior to configuring the cmd queue. // tx.c:1163-1168
    io.prph_write(SCD_EN_CTRL, 0);
    // Stop this Tx queue before configuring it. // tx.c:1169-1170 (iwl_scd_txq_set_inactive, iwl-scd.h:76-82)
    io.prph_write(scd_queue_status_bits(q), (0 << SCD_QUEUE_STTS_REG_POS_ACTIVE) | (1 << SCD_QUEUE_STTS_REG_POS_SCD_ACT_EN));
    // (the chain bit is set for non-cmd queues only, tx.c:1173-1174)
    // Not aggregated: disable aggregation for the queue; ssn = txq->read_ptr = 0. // tx.c:1191-1193 (iwl-scd.h:25-29)
    let agg = io.prph_read(SCD_AGGR_SEL);
    io.prph_write(SCD_AGGR_SEL, agg & !(1 << q));
    let ssn: u32 = 0;
    io.write_direct32(HBUS_TARG_WRPTR, (ssn & 0xff) | (q << 8)); // tx.c:1216-1219
    io.prph_write(scd_queue_rdptr(q), ssn); // tx.c:1224
    // Set up Tx window size and frame limit for this queue. // tx.c:1226-1233
    io.write_mem(scd_base_addr + scd_context_queue_offset(q), &[0])?;
    io.write_mem(
        scd_base_addr + scd_context_queue_offset(q) + 4,
        &[field(SCD_QUEUE_CTX_REG2_WIN_SIZE, IWL_FRAME_LIMIT) | field(SCD_QUEUE_CTX_REG2_FRAME_LIMIT, IWL_FRAME_LIMIT)],
    )?;
    // Set up status area in SRAM, map to Tx DMA/FIFO, activate. // tx.c:1235-1241
    io.prph_write(
        scd_queue_status_bits(q),
        (1 << SCD_QUEUE_STTS_REG_POS_ACTIVE) | (fifo << SCD_QUEUE_STTS_REG_POS_TXF) | (1 << SCD_QUEUE_STTS_REG_POS_WSL) | SCD_QUEUE_STTS_REG_MSK,
    );
    // Enable the scheduler for this queue (only). // tx.c:1243-1245
    io.prph_write(SCD_EN_CTRL, 1 << q);
    Ok(())
}

/// `FIELD_PREP`.
fn field(mask: u32, val: u32) -> u32 {
    (val << mask.trailing_zeros()) & mask
}

/// The host side of the command queue's indices. // tx.c:1036-1146; internal.h:658-715; tx-gen2.c:527-555
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cmdq {
    /// `n_window`. // tx.c:855 (`slots_num`)
    pub n_window: u16,
    pub read_ptr: u16,
    pub write_ptr: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReclaimError {
    /// "Read index for DMA queue txq id (%d), index %d is out of range". // tx.c:1089-1100
    OutOfRange { idx: u16 },
    /// "HCMD skipped: index (%d) %d %d" — more than one entry reclaimed at once. // tx.c:1103-1106
    Skipped { idx: u16, write_ptr: u16, r: u16 },
}

impl Cmdq {
    /// `iwl_queue_init` for the command queue: both pointers at 0. // tx.c:880-900
    pub fn new() -> Self {
        Cmdq { n_window: IWL_CMD_QUEUE_SIZE, read_ptr: 0, write_ptr: 0 }
    }

    /// `iwl_txq_inc_wrap`: `++index & (max_tfd_queue_size - 1)`. // internal.h:711-715
    pub fn inc_wrap(index: u16) -> u16 {
        (index + 1) & (MAX_TFD_QUEUE_SIZE - 1)
    }

    /// `iwl_txq_get_cmd_index`: the command slot (not the TFD) of a queue index. // internal.h:664-667
    pub fn cmd_index(&self, index: u16) -> u16 {
        index & (self.n_window - 1)
    }

    /// `iwl_txq_space`. // tx-gen2.c:527-555
    pub fn space(&self) -> u16 {
        let max = if self.n_window < MAX_TFD_QUEUE_SIZE { self.n_window } else { MAX_TFD_QUEUE_SIZE - 1 };
        let used = self.write_ptr.wrapping_sub(self.read_ptr) & (MAX_TFD_QUEUE_SIZE - 1);
        if used > max {
            return 0; // WARN_ON(used > max)
        }
        max - used
    }

    /// `iwl_txq_used`. // tx.c:1056-1068
    pub fn used(&self, i: u16, read_ptr: u16, write_ptr: u16) -> bool {
        let index = self.cmd_index(i);
        let r = self.cmd_index(read_ptr);
        let w = self.cmd_index(write_ptr);
        if w >= r {
            index >= r && index < w
        } else {
            !(index < r && index >= w)
        }
    }

    /// The sequence number of the command at `write_ptr`: `QUEUE_TO_SEQ(cmd_queue) | INDEX_TO_SEQ(write_ptr)`.
    /// // tx.c:1475-1477; fw/api/cmdhdr.h:18-21
    pub fn sequence(&self) -> u16 {
        (((IWL_MVM_DQA_CMD_QUEUE & 0x1f) as u16) << 8) | (self.write_ptr & 0xff)
    }

    /// After the TFD is built: `txq->write_ptr = iwl_txq_inc_wrap(...)`, and the doorbell value
    /// `iwl_pcie_txq_inc_wr_ptr` writes: `HBUS_TARG_WRPTR ← write_ptr | (txq_id << 8)`.
    /// // tx.c:1606-1607, :137-138
    pub fn advance(&mut self) -> (u32, u32) {
        self.write_ptr = Self::inc_wrap(self.write_ptr);
        (HBUS_TARG_WRPTR, self.write_ptr as u32 | (IWL_MVM_DQA_CMD_QUEUE << 8))
    }

    /// `iwl_pcie_cmdq_reclaim(trans, txq_id, idx)`: the firmware answered the command at `idx`.
    /// // tx.c:1079-1115
    pub fn reclaim(&mut self, idx: u16) -> Result<(), ReclaimError> {
        let idx = self.cmd_index(idx); // tx.c:1086
        let mut r = self.cmd_index(self.read_ptr); // tx.c:1087
        if idx >= MAX_TFD_QUEUE_SIZE || !self.used(idx, self.read_ptr, self.write_ptr) {
            return Err(ReclaimError::OutOfRange { idx }); // tx.c:1089-1100
        }
        let mut nfreed = 0u32;
        let end = Self::inc_wrap(idx); // tx.c:1102 (`idx = iwl_txq_inc_wrap(trans, idx)`)
        while r != end {
            self.read_ptr = Self::inc_wrap(self.read_ptr); // tx.c:1103
            if nfreed > 0 {
                return Err(ReclaimError::Skipped { idx: end, write_ptr: self.write_ptr, r }); // tx.c:1105-1108
            }
            nfreed += 1;
            r = Self::inc_wrap(r); // tx.c:1102 (loop increment)
        }
        Ok(())
    }
}

impl Default for Cmdq {
    fn default() -> Self {
        Self::new()
    }
}

/// One chunk of a host command. `nocopy` is `IWL_HCMD_DFL_NOCOPY`: the transport maps the caller's
/// buffer instead of copying it into the slot. // iwl-trans.h:170-200
#[derive(Debug, Clone, Copy)]
pub struct Chunk<'a> {
    pub data: &'a [u8],
    pub nocopy: bool,
}

/// What `iwl_pcie_enqueue_hcmd` decided: the bytes now in the command slot, and the TBs to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HcmdPlan {
    /// Total command size, `cmd_size`: header + every chunk. // tx.c:1352-1353, :1424
    pub cmd_size: usize,
    /// Bytes of the slot the TFD covers (`copy_size` after the copy loop). // tx.c:1503-1531
    pub copy_size: usize,
    /// TB0 length (`tb0_size`): the first min(copy_size, 20) bytes, from the 64-byte first-TB slot.
    pub tb0_len: usize,
    /// TB1: the rest of the copied bytes, mapped from the slot at `tb0_len`. // tx.c:1544-1559
    pub tb1_len: usize,
    /// NOCOPY chunks to map after, as (chunk index, bytes to skip, length). // tx.c:1561-1580
    pub nocopy: [Option<(usize, usize, usize)>; IWL_MAX_CMD_TBS_PER_TFD],
}

/// `iwl_pcie_enqueue_hcmd` up to (not including) the DMA mapping and the doorbell: validate, build the
/// header with `sequence`, copy what is copied into `slot` (`TFD_MAX_PAYLOAD_SIZE` bytes), and say
/// which TBs the TFD gets. // tx.c:1343-1531
pub fn plan_hcmd(group: u8, opcode: u8, version: u8, sequence: u16, chunks: &[Chunk<'_>], slot: &mut [u8]) -> Result<HcmdPlan, TxError> {
    if slot.len() < TFD_MAX_PAYLOAD_SIZE {
        return Err(TxError::Buf { len: slot.len() });
    }
    if chunks.len() > IWL_MAX_CMD_TBS_PER_TFD {
        return Err(TxError::TooManyTbs);
    }
    let hdr = if group != 0 { CMD_HDR_WIDE_LEN } else { CMD_HDR_LEN }; // tx.c:1367-1372
    let mut copy_size = hdr;
    let mut cmd_size = hdr;
    let mut cmdskip = [0usize; IWL_MAX_CMD_TBS_PER_TFD];
    let mut cmdlen = [0usize; IWL_MAX_CMD_TBS_PER_TFD];
    let mut had_nocopy = false;
    for (i, c) in chunks.iter().enumerate() {
        cmdlen[i] = c.data.len(); // tx.c:1375-1376
        if c.data.is_empty() {
            continue; // tx.c:1382-1383
        }
        // need at least IWL_FIRST_TB_SIZE copied // tx.c:1385-1394
        if copy_size < IWL_FIRST_TB_SIZE {
            let copy = (IWL_FIRST_TB_SIZE - copy_size).min(cmdlen[i]);
            cmdlen[i] -= copy;
            cmdskip[i] += copy;
            copy_size += copy;
        }
        if c.nocopy {
            had_nocopy = true; // tx.c:1396-1398
        } else {
            // NOCOPY must not be followed by normal! // tx.c:1420-1424
            if had_nocopy {
                return Err(TxError::CopyAfterNocopy);
            }
            copy_size += cmdlen[i]; // tx.c:1425
        }
        cmd_size += c.data.len(); // tx.c:1427
    }
    if copy_size > TFD_MAX_PAYLOAD_SIZE {
        return Err(TxError::TooLarge { copy_size }); // tx.c:1436-1444
    }
    // set up the header // tx.c:1466-1490
    slot[..TFD_MAX_PAYLOAD_SIZE].fill(0);
    slot[0] = opcode;
    slot[1] = group;
    slot[2..4].copy_from_slice(&sequence.to_le_bytes());
    if group != 0 {
        slot[4..6].copy_from_slice(&((cmd_size - CMD_HDR_WIDE_LEN) as u16).to_le_bytes()); // hdr_wide.length, tx.c:1471-1473
        slot[6] = 0; // reserved
        slot[7] = version; // hdr_wide.version
    }
    let mut cmd_pos = hdr;
    copy_size = hdr;
    // and copy the data that needs to be copied // tx.c:1493-1531
    for c in chunks.iter() {
        if c.data.is_empty() {
            continue;
        }
        if !c.nocopy {
            slot[cmd_pos..cmd_pos + c.data.len()].copy_from_slice(c.data); // tx.c:1504-1509
            cmd_pos += c.data.len();
            copy_size += c.data.len();
            continue;
        }
        // Otherwise we need at least IWL_FIRST_TB_SIZE copied in total (bi-directional DMA), but
        // copy up to what fits into the payload for debug dump purposes. // tx.c:1511-1518
        let copy = (TFD_MAX_PAYLOAD_SIZE - cmd_pos).min(c.data.len());
        slot[cmd_pos..cmd_pos + copy].copy_from_slice(&c.data[..copy]);
        cmd_pos += copy;
        // However, treat copy_size the proper way, we need it below. // tx.c:1520-1527
        if copy_size < IWL_FIRST_TB_SIZE {
            let copy = (IWL_FIRST_TB_SIZE - copy_size).min(c.data.len());
            copy_size += copy;
        }
    }
    // start the TFD with the minimum copy bytes // tx.c:1538-1543
    let tb0_len = copy_size.min(IWL_FIRST_TB_SIZE);
    let tb1_len = copy_size.saturating_sub(tb0_len); // tx.c:1545-1560
    let mut nocopy = [None; IWL_MAX_CMD_TBS_PER_TFD];
    for (i, c) in chunks.iter().enumerate() {
        if cmdlen[i] == 0 || !c.nocopy {
            continue; // tx.c:1562-1580
        }
        nocopy[i] = Some((i, cmdskip[i], cmdlen[i]));
    }
    Ok(HcmdPlan { cmd_size, copy_size, tb0_len, tb1_len, nocopy })
}
