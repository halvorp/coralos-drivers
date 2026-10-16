// SPDX-License-Identifier: GPL-2.0-only
//! The multi-queue receive path of the gen1 transport, one queue: the RFH programming, the free and
//! used descriptor tables, the write-pointer rule, the status block, and the packets inside one
//! receive buffer. Ported from Linux `pcie/gen1_2/rx.c:167-203` (`iwl_pcie_rxq_inc_wr_ptr`),
//! `:221-238` (`iwl_pcie_restock_bd`), `:245-290` (`iwl_pcie_rxmq_restock`), `:703-710`
//! (`iwl_pcie_rb_stts_size`), `:900-986` (`iwl_pcie_rx_mq_hw_init`), `:1294-1380`
//! (`iwl_pcie_rx_handle_rb`), `:1443-1490` (`iwl_pcie_get_rxb`), `:1498-1625` (`iwl_pcie_rx_handle`),
//! with `iwl-fh.h:571-587`.
//!
//! Copyright (C) 2003-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
//!
//! WHAT IS LEFT OUT, by name. Linux recycles receive buffers through a page allocator with a work
//! item, an "emergency" mode and `page_stolen` (the upper layer keeps the page of a frame it queues);
//! the port's pool is FIXED at init and every buffer goes back to the free list after its packets were
//! handled — the caller copies out what it keeps. Multiple RX queues (RSS) need MSI-X, which this
//! transport does not model.

use crate::rxregs::*;
use iwlwifi_pcie_nic_core::bus::{NicFault, NicIo};
use iwlwifi_pcie_nic_core::regs::{CSR_INT_COALESCING, IWL_HOST_INT_TIMEOUT_DEF};

/// The three DMA regions one RX queue hands the RFH. // rx.c:933-944
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxqDma {
    /// Free RBD table, `queue_size` × 8 bytes. // rx.c:664-665
    pub bd: u64,
    /// Used RBD table, `queue_size` × 4 bytes. // rx.c:676-678
    pub used_bd: u64,
    /// The 12-byte status block. // rx.c:703-710
    pub rb_stts: u64,
}

/// `FIELD_PREP(mask, val)`: `val` shifted to the mask's lowest set bit, clipped to the mask.
pub const fn field_prep(mask: u32, val: u32) -> u32 {
    (val << mask.trailing_zeros()) & mask
}

/// `iwl_pcie_rx_mq_hw_init` for one queue, a 4K receive buffer, 512 RBDs, a discrete (non-integrated)
/// part. Returns the NIC fault when the grab fails (Linux returns silently, rx.c:924-926).
/// // rx.c:900-986
pub fn rx_mq_hw_init<I: NicIo>(io: &mut I, dma: &RxqDma) -> Result<(), NicFault> {
    let rb_size = RFH_RXF_DMA_RB_SIZE_4K; // rx.c:904-922 for IWL_AMSDU_4K
    if !io.grab() {
        return Err(io.fault().unwrap_or(NicFault::AccessTimeout { gp_cntrl: 0 })); // rx.c:924-926
    }
    // Stop Rx DMA; disable free and used rx queue operation. // rx.c:927-930
    io.prph_write(RFH_RXF_DMA_CFG, 0);
    io.prph_write(RFH_RXF_RXQ_ACTIVE, 0);
    let mut enabled = 0u32;
    let q = 0u32; // num_rxqs == 1 in MSI mode, trans.c:1658
    io.write_prph64_no_grab(rfh_q_frbdcb_ba_lsb(q), dma.bd); // rx.c:934-936: where the RBD free table is
    io.write_prph64_no_grab(rfh_q_urbdcb_ba_lsb(q), dma.used_bd); // rx.c:938-940: where the RBD used table is
    io.write_prph64_no_grab(rfh_q_urbd_stts_wptr_lsb(q), dma.rb_stts); // rx.c:942-944: where the status goes
    io.prph_write(rfh_q_frbdcb_widx(q), 0); // rx.c:946 reset the device index tables
    io.prph_write(rfh_q_frbdcb_ridx(q), 0); // rx.c:947
    io.prph_write(rfh_q_urbdcb_widx(q), 0); // rx.c:948
    enabled |= (1 << q) | (1 << (q + 16)); // rx.c:950
    // Enable Rx DMA: 4K buffers, min RB 4/8, drop frames that exceed the RB, 512 RBDs. // rx.c:952-964
    io.prph_write(
        RFH_RXF_DMA_CFG,
        RFH_DMA_EN_ENABLE_VAL | rb_size | RFH_RXF_DMA_MIN_RB_4_8 | RFH_RXF_DMA_DROP_TOO_LARGE_MASK | RFH_RXF_DMA_RBDCB_SIZE_512,
    );
    // Activate DMA snooping; 128-byte RX DMA chunks for PCIe; default queue 0. // rx.c:966-978
    io.prph_write(
        RFH_GEN_CFG,
        RFH_GEN_CFG_RFH_DMA_SNOOP
            | field_prep(RFH_GEN_CFG_DEFAULT_RXQ_NUM, 0) // RFH_GEN_CFG_VAL(DEFAULT_RXQ_NUM, DEFAULT_RXQ_NUM_0)
            | RFH_GEN_CFG_SERVICE_DMA_SNOOP
            | field_prep(RFH_GEN_CFG_RB_CHUNK_SIZE, RFH_GEN_CFG_RB_CHUNK_SIZE_128), // 64 for an integrated part
    );
    // Enable the relevant rx queues. // rx.c:979-980
    io.prph_write(RFH_RXF_RXQ_ACTIVE, enabled);
    io.release(); // rx.c:982
    // Set interrupt coalescing timer to default (2048 usecs). // rx.c:984-985
    io.write8(CSR_INT_COALESCING, IWL_HOST_INT_TIMEOUT_DEF);
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxError {
    /// VID 0 means "no buffer" and the field is 12 bits. // rx.c:1476
    Vid { vid: u16 },
    /// The buffer address must leave the VID bits clear. // rx.c:232-236
    AddrOverlapsVid { page_dma: u64 },
    /// A table or block shorter than its entry.
    Buf { len: usize },
}

/// One free RBD (`iwl_pcie_restock_bd`, pre-AX210): `page_dma | vid`. // rx.c:230-236
pub fn free_bd(page_dma: u64, vid: u16) -> Result<u64, RxError> {
    if vid == 0 || vid as u32 > RX_VID_MASK {
        return Err(RxError::Vid { vid });
    }
    if page_dma & RX_VID_MASK as u64 != 0 {
        return Err(RxError::AddrOverlapsVid { page_dma });
    }
    Ok(page_dma | vid as u64)
}

/// The VID the firmware wrote into used RBD `i`. // rx.c:1473 (`le32_to_cpu(cd[i]) & 0x0FFF`)
pub fn used_bd_vid(used_bd: u32) -> u16 {
    (used_bd & RX_VID_MASK) as u16
}

/// `closed_rb_num` out of the status block, the 12-bit index of the last buffer the firmware filled.
/// // iwl-fh.h:581-583; pcie/gen1_2/internal.h:195-207 (`iwl_get_closed_rb_stts`)
pub fn closed_rb(rb_stts: &[u8]) -> Result<u16, RxError> {
    if rb_stts.len() < RB_STATUS_LEN {
        return Err(RxError::Buf { len: rb_stts.len() });
    }
    Ok(u16::from_le_bytes([rb_stts[0], rb_stts[1]]) & 0x0FFF)
}

/// The host side of one RX queue's indices. // rx.c:117-135, :245-290
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rxq {
    pub queue_size: u16,
    /// Next used RBD to look at.
    pub read: u16,
    /// Next free RBD slot to fill.
    pub write: u16,
    /// What the device was last told, a multiple of 8.
    pub write_actual: u16,
}

impl Rxq {
    /// `iwl_pcie_rx_init`'s reset: nothing posted yet. // rx.c:1117-1121
    pub fn new(queue_size: u16) -> Self {
        Rxq { queue_size, read: 0, write: 0, write_actual: 0 }
    }

    /// Take the next free-table slot (and advance `write`). // rx.c:258-266
    pub fn restock_slot(&mut self) -> u16 {
        let i = self.write;
        self.write = (self.write + 1) & (self.queue_size - 1);
        i
    }

    /// Whether the device has to be told: `write_actual != round_down(write, 8)`. // rx.c:279-283
    pub fn needs_wr_ptr(&self) -> bool {
        self.write_actual != self.write & !(RX_WRITE_PTR_ALIGN - 1)
    }

    /// `iwl_pcie_rxq_inc_wr_ptr`, shadow registers on, MAC awake not required: the trigger register.
    /// // rx.c:193-201 (`rxq->write_actual = round_down(rxq->write, 8)`; WIDX_TRG for < BZ)
    pub fn wr_ptr_update(&mut self, q: u32) -> (u32, u32) {
        self.write_actual = self.write & !(RX_WRITE_PTR_ALIGN - 1);
        (rfh_q_frbdcb_widx_trg(q), self.write_actual as u32)
    }

    /// Entries filled by the firmware and not yet handled, given `closed_rb`. // rx.c:1514-1521
    pub fn pending(&self, closed_rb: u16) -> u16 {
        let r = closed_rb & (self.queue_size - 1); // "W/A 9000 device step A0 wrap-around bug", rx.c:1517-1518
        r.wrapping_sub(self.read) & (self.queue_size - 1)
    }

    /// Consume the next used entry; returns its index. // rx.c:1568 (`i = (i + 1) & (rxq->queue_size - 1)`)
    pub fn take_read(&mut self) -> u16 {
        let i = self.read;
        self.read = (self.read + 1) & (self.queue_size - 1);
        i
    }
}

/// The packets inside one receive buffer, walked as `iwl_pcie_rx_handle_rb` walks them: stop at the
/// invalid marker, advance by the 64-byte-aligned frame length, stop on anything that does not fit.
/// Each item is `(offset_in_rb, packet)`. // rx.c:1311-1355
pub struct RbPackets<'a> {
    rb: &'a [u8],
    offset: usize,
    done: bool,
}

pub fn rb_packets(rb: &[u8]) -> RbPackets<'_> {
    RbPackets { rb, offset: 0, done: false }
}

/// sizeof(struct iwl_rx_packet): len_n_flags + the 4-byte command header. // iwl-trans.h:71-90
pub const RX_PACKET_HDR_LEN: usize = 8;

/// One packet of a receive buffer. `data` is the payload after the 8-byte header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxPkt<'a> {
    pub offset: usize,
    pub len_n_flags: u32,
    pub cmd: u8,
    pub group_id: u8,
    pub sequence: u16,
    pub data: &'a [u8],
}

impl RxPkt<'_> {
    /// `SEQ_RX_FRAME`: the packet originated in the firmware (a notification), not a command reply.
    /// // fw/api/cmdhdr.h:22
    pub fn is_notification(&self) -> bool {
        self.sequence & 0x8000 != 0
    }

    pub fn rx_queue(&self) -> u8 {
        ((self.len_n_flags & FH_RSCSR_RXQ_MASK) >> FH_RSCSR_RXQ_POS) as u8
    }
}

impl<'a> Iterator for RbPackets<'a> {
    type Item = RxPkt<'a>;

    fn next(&mut self) -> Option<RxPkt<'a>> {
        if self.done {
            return None;
        }
        let max_len = self.rb.len();
        // `while (offset + sizeof(u32) + sizeof(struct iwl_cmd_header) < max_len)` // rx.c:1311
        if self.offset + 4 + 4 >= max_len {
            self.done = true;
            return None;
        }
        let o = self.offset;
        let w = u32::from_le_bytes([self.rb[o], self.rb[o + 1], self.rb[o + 2], self.rb[o + 3]]);
        if w == FH_RSCSR_FRAME_INVALID {
            self.done = true; // "RB end marker", rx.c:1325-1330
            return None;
        }
        let frame = (w & FH_RSCSR_FRAME_SIZE_MSK) as usize; // iwl_rx_packet_len, iwl-trans.h:96
        let len = frame + 4; // "account for status word", rx.c:1348
        let next = o + (len + FH_RSCSR_FRAME_ALIGN - 1) / FH_RSCSR_FRAME_ALIGN * FH_RSCSR_FRAME_ALIGN; // ALIGN(len, 0x40), rx.c:1350
        // check that what the device tells us made sense // rx.c:1353
        if len < RX_PACKET_HDR_LEN || next > max_len {
            self.done = true;
            return None;
        }
        self.offset = next;
        Some(RxPkt {
            offset: o,
            len_n_flags: w,
            cmd: self.rb[o + 4],
            group_id: self.rb[o + 5],
            sequence: u16::from_le_bytes([self.rb[o + 6], self.rb[o + 7]]),
            data: &self.rb[o + RX_PACKET_HDR_LEN..o + len],
        })
    }
}

/// Whether a command reply must be reclaimed from the command queue: replies only (`!SEQ_RX_FRAME`),
/// and not the group-0 commands the op-mode lists as no-reclaim. // rx.c:1364-1376
pub fn reclaim(p: &RxPkt<'_>, no_reclaim_cmds: &[u8]) -> bool {
    let mut r = !p.is_notification();
    if r && p.group_id == 0 {
        r = !no_reclaim_cmds.contains(&p.cmd); // rx.c:1366-1374
    }
    r
}

/// Convenience for the status-block layout test: the fields of `struct iwl_rb_status`.
pub fn rb_status_fields(rb_stts: &[u8]) -> Result<[u16; 4], RxError> {
    if rb_stts.len() < RB_STATUS_LEN {
        return Err(RxError::Buf { len: rb_stts.len() });
    }
    let w = |i: usize| u16::from_le_bytes([rb_stts[i], rb_stts[i + 1]]);
    Ok([w(0), w(2), w(4), w(6)]) // closed_rb_num, closed_fr_num, finished_rb_num, finished_fr_num
}
