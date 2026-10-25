// SPDX-License-Identifier: GPL-2.0-only
//! Data transmit queues of the gen1 PCIe transport: enable a queue after ALIVE, put one 802.11 frame
//! on it (`TX_CMD` + header + payload as three transfer buffers), and reclaim what the firmware
//! reports sent. Ported from Linux `pcie/gen1_2/tx.c` (`iwl_trans_pcie_tx` :2108-2290,
//! `iwl_txq_gen1_update_byte_cnt_tbl` :2061-2100, `iwl_txq_gen1_inval_byte_cnt_tbl` and
//! `iwl_pcie_reclaim` :2310-2420, `iwl_pcie_txq_inc_wr_ptr`) with the TFD and byte-count helpers of
//! `iwlwifi-pcie-ring-core`. Linux v7.0-rc6.
//!
//! DEVIATIONS, named. Linux allocates every queue's 256 slots of device command, first-TB and
//! payload buffers up front and takes the payload from the skb. Here the TFD ring has the hardware's
//! 256 entries (the card wraps at 256) but only [`TXQ_INFLIGHT`] frames may be outstanding, so the
//! per-frame buffers are indexed `idx % TXQ_INFLIGHT` and the payload is copied into a slot of
//! [`PAYLOAD_SLOT`] bytes. One queue costs about 110 KiB of DMA memory.
use crate::host::Host;
use crate::transport::{Transport, TransportError};
use alloc::vec::Vec;
use coralos_hal::DmaRegion;
use iwlwifi_mvm_join_core::ids::{CMD_HEADER_LEN, IWL_FIRST_TB_SIZE, TX_CMD, TX_CMD_FLG_MH_PAD, TX_CMD_PARAMS_LEN};
use iwlwifi_mvm_join_core::txcmd::{byte_count_entry, tb_plan, tx_cmd_params, TxParams};
use iwlwifi_pcie_nic_core::bus::NicIo;
use iwlwifi_pcie_nic_core::regs::HBUS_TARG_WRPTR;
use iwlwifi_pcie_ring_core::tx::tfd_add_tb;
use iwlwifi_pcie_ring_core::txregs::{IWL9000_NUM_OF_QUEUES, IWL_FIRST_TB_SIZE_ALIGN, MAX_TFD_QUEUE_SIZE, TFD_SIZE};
use iwlwifi_pcie_trans_core::io::TransIo;
use iwlwifi_pcie_trans_core::regs::fh_mem_cbbc_queue;

/// Frames outstanding on one queue at once (see the module docs).
pub const TXQ_INFLIGHT: usize = 32;
/// Bytes of one payload slot: an MSDU of 1500 bytes, LLC/SNAP and the CCMP header fit twice over.
pub const PAYLOAD_SLOT: usize = 2048;
/// Bytes of one device-command slot: `iwl_cmd_header` + `iwl_tx_cmd_v6_params` + a header of up to 36 bytes + pad.
pub const CMD_SLOT: usize = 128;
/// The scheduler's byte-count table has this many entries per queue. // iwl-fh.h:590-594
pub const TFD_QUEUE_BC_SIZE: usize = 256 + 64;
/// `TFD_QUEUE_SIZE_BC_DUP`: the first entries are duplicated after the 256th. // iwl-fh.h:590-594
pub const TFD_QUEUE_SIZE_BC_DUP: usize = 64;

/// One enabled data queue.
pub struct DataTxq {
    pub id: u8,
    ring: DmaRegion,
    first_tb: DmaRegion,
    cmds: DmaRegion,
    payload: DmaRegion,
    /// `write_ptr` / `read_ptr`: next slot to fill, oldest slot not yet reclaimed. 0..256.
    pub wp: u16,
    pub rp: u16,
    sta: [u8; TXQ_INFLIGHT],
    /// Frames put on the ring, ever.
    pub sent: u32,
}

impl DataTxq {
    /// Frames on the ring that the firmware has not reported yet.
    pub fn outstanding(&self) -> usize {
        (self.wp.wrapping_sub(self.rp) % MAX_TFD_QUEUE_SIZE) as usize
    }
}

fn inc_wrap(i: u16) -> u16 {
    (i + 1) % MAX_TFD_QUEUE_SIZE
}

impl<H: Host> Transport<H> {
    /// Allocate (once) and (re)program the TFD ring of data queue `qid`, and point the write index at 0.
    /// The firmware's own queue configuration (`SCD_QUEUE_CFG`) is a host command the caller sends
    /// next. // iwl_pcie_tx_init :987-988 for the base register; iwl_trans_pcie_txq_enable's write
    /// pointer move, trans.c:
    pub fn txq_enable(&mut self, qid: u8) -> Result<(), TransportError> {
        let reg = match fh_mem_cbbc_queue(qid as u32) {
            Some(r) if (qid as u32) < IWL9000_NUM_OF_QUEUES && qid != 0 => r,
            _ => return Err(TransportError::TxQueueUnknown { qid }),
        };
        let i = match self.dtx.iter().position(|q| q.id == qid) {
            Some(i) => {
                let q = &mut self.dtx[i];
                q.wp = 0;
                q.rp = 0;
                q.sent = 0;
                i
            }
            None => {
                let ring = self.alloc("data TFD ring", MAX_TFD_QUEUE_SIZE as usize * TFD_SIZE, 4096)?;
                let first_tb = self.alloc("data first-TB buffers", TXQ_INFLIGHT * IWL_FIRST_TB_SIZE_ALIGN, 4096)?;
                let cmds = self.alloc("data command slots", TXQ_INFLIGHT * CMD_SLOT, 4096)?;
                let payload = self.alloc("data payload slots", TXQ_INFLIGHT * PAYLOAD_SLOT, 4096)?;
                self.dtx.push(DataTxq { id: qid, ring, first_tb, cmds, payload, wp: 0, rp: 0, sta: [0; TXQ_INFLIGHT], sent: 0 });
                self.dtx.len() - 1
            }
        };
        let phys = self.dtx[i].ring.phys;
        self.nic.csr_write(reg, (phys >> 8) as u32);
        self.nic.write_direct32(HBUS_TARG_WRPTR, (qid as u32) << 8);
        Ok(())
    }

    pub fn txq(&self, qid: u8) -> Option<&DataTxq> {
        self.dtx.iter().find(|q| q.id == qid)
    }

    /// Put one frame on data queue `qid`: the device command (`TX_CMD`, `params`, the 802.11 `hdr`)
    /// and the `body` that follows the header (CCMP header, LLC/SNAP, payload). Returns the ring index
    /// it went to. The caller later reclaims it from the `TX_CMD` response. // iwl_trans_pcie_tx
    pub fn txq_send(&mut self, qid: u8, params: &TxParams, hdr: &[u8], body: &[u8]) -> Result<u16, TransportError> {
        let qi = self.dtx.iter().position(|q| q.id == qid).ok_or(TransportError::TxQueueUnknown { qid })?;
        let plan = tb_plan(hdr.len());
        let cmd_len = IWL_FIRST_TB_SIZE + plan.tb1_len;
        if cmd_len > CMD_SLOT {
            return Err(TransportError::TxFrameTooLong { len: cmd_len, max: CMD_SLOT });
        }
        if body.len() > PAYLOAD_SLOT {
            return Err(TransportError::TxFrameTooLong { len: body.len(), max: PAYLOAD_SLOT });
        }
        if self.dtx[qi].outstanding() >= TXQ_INFLIGHT {
            return Err(TransportError::QueueFull); // iwl_txq_space() below the low mark, tx.c:2135-2150
        }
        let (idx, slot) = (self.dtx[qi].wp, self.dtx[qi].wp as usize % TXQ_INFLIGHT);
        let tb0_phys = self.dtx[qi].first_tb.phys + (slot * IWL_FIRST_TB_SIZE_ALIGN) as u64;
        // scratch lives in the first TB: after the command header, at offsetof(params, scratch) = 8. // tx.c:2166
        let scratch_phys = tb0_phys + CMD_HEADER_LEN as u64 + 8;

        let mut cmd = [0u8; CMD_SLOT];
        cmd[0] = TX_CMD;
        cmd[1] = 0; // LEGACY_GROUP
        cmd[2..4].copy_from_slice(&(((qid as u16) << 8) | idx).to_le_bytes()); // QUEUE_TO_SEQ | INDEX_TO_SEQ, tx.c:2158
        tx_cmd_params(params, scratch_phys, &mut cmd[CMD_HEADER_LEN..CMD_HEADER_LEN + TX_CMD_PARAMS_LEN]).map_err(|_| TransportError::TxFrameTooLong { len: cmd_len, max: CMD_SLOT })?;
        if plan.mh_pad {
            // Tell the NIC about the 2-byte padding after the MAC header. // tx.c:2186-2190
            let f = u32::from_le_bytes([cmd[8], cmd[9], cmd[10], cmd[11]]) | TX_CMD_FLG_MH_PAD;
            cmd[8..12].copy_from_slice(&f.to_le_bytes());
        }
        let ho = CMD_HEADER_LEN + TX_CMD_PARAMS_LEN;
        cmd[ho..ho + hdr.len()].copy_from_slice(hdr);

        let sec_ctl = cmd[CMD_HEADER_LEN + 17];
        let bc = byte_count_entry(params.frame_len, sec_ctl, params.sta_id).map_err(|e| match e {
            iwlwifi_mvm_join_core::txcmd::ByteCountRefusal::TooLong(len) => TransportError::TxByteCount { len },
        })?;

        let tfd_bytes = {
            let q = &self.dtx[qi];
            let host = &mut self.nic.bus;
            host.dma_write(&q.first_tb, slot * IWL_FIRST_TB_SIZE_ALIGN, &cmd[..IWL_FIRST_TB_SIZE]);
            host.dma_write(&q.cmds, slot * CMD_SLOT, &cmd[..cmd_len]);
            let mut tfd = [0u8; TFD_SIZE];
            tfd_add_tb(&mut tfd, tb0_phys, IWL_FIRST_TB_SIZE as u16, true)?;
            tfd_add_tb(&mut tfd, q.cmds.phys + (slot * CMD_SLOT + IWL_FIRST_TB_SIZE) as u64, plan.tb1_len as u16, false)?;
            if !body.is_empty() {
                host.dma_write(&q.payload, slot * PAYLOAD_SLOT, body);
                tfd_add_tb(&mut tfd, q.payload.phys + (slot * PAYLOAD_SLOT) as u64, body.len() as u16, false)?;
            }
            host.dma_write(&q.ring, idx as usize * TFD_SIZE, &tfd);
            tfd
        };
        let _ = tfd_bytes;

        // Set up the entry in the scheduler byte-count table. // tx.c:2268-2272, :2061-2100
        let bc_region = self.tx.as_ref().ok_or(TransportError::IsDown)?.bc;
        let base = (qid as usize * TFD_QUEUE_BC_SIZE + idx as usize) * 2;
        self.nic.bus.dma_write(&bc_region, base, &bc.to_le_bytes());
        if (idx as usize) < TFD_QUEUE_SIZE_BC_DUP {
            self.nic.bus.dma_write(&bc_region, base + MAX_TFD_QUEUE_SIZE as usize * 2, &bc.to_le_bytes());
        }

        let q = &mut self.dtx[qi];
        q.sta[slot] = params.sta_id;
        q.wp = inc_wrap(q.wp);
        q.sent += 1;
        let wp = q.wp;
        // Tell the device the write index just past the TFD filled. // tx.c:2288, :137
        self.nic.csr_write(HBUS_TARG_WRPTR, wp as u32 | ((qid as u32) << 8));
        self.stats.cmds_sent += 1;
        Ok(idx)
    }

    /// Free the frames before `ssn` (exclusive): the firmware reports them sent or dropped. Returns how
    /// many were freed. A `ssn` beyond the write pointer is refused by name. // iwl_pcie_reclaim, tx.c:2336-2420
    pub fn txq_reclaim(&mut self, qid: u8, ssn: u16) -> Result<u16, TransportError> {
        let qi = self.dtx.iter().position(|q| q.id == qid).ok_or(TransportError::TxQueueUnknown { qid })?;
        let (rp, wp) = (self.dtx[qi].rp, self.dtx[qi].wp);
        let ssn = ssn % MAX_TFD_QUEUE_SIZE;
        let span = (wp + MAX_TFD_QUEUE_SIZE - rp) % MAX_TFD_QUEUE_SIZE;
        let want = (ssn + MAX_TFD_QUEUE_SIZE - rp) % MAX_TFD_QUEUE_SIZE;
        if want > span {
            return Err(TransportError::TxReclaimRange { qid, ssn, read: rp, write: wp });
        }
        let bc_region = self.tx.as_ref().ok_or(TransportError::IsDown)?.bc;
        let mut i = rp;
        let mut freed = 0;
        while i != ssn {
            // iwl_txq_gen1_inval_byte_cnt_tbl: len 1 with the station id of the frame. // tx.c:2300-2325
            let sta = self.dtx[qi].sta[i as usize % TXQ_INFLIGHT];
            let ent = (1u16 | ((sta as u16) << 12)).to_le_bytes();
            let base = (qid as usize * TFD_QUEUE_BC_SIZE + i as usize) * 2;
            self.nic.bus.dma_write(&bc_region, base, &ent);
            if (i as usize) < TFD_QUEUE_SIZE_BC_DUP {
                self.nic.bus.dma_write(&bc_region, base + MAX_TFD_QUEUE_SIZE as usize * 2, &ent);
            }
            i = inc_wrap(i);
            freed += 1;
        }
        self.dtx[qi].rp = ssn;
        Ok(freed)
    }

    /// Queue ids that are enabled, for the log.
    pub fn txq_ids(&self) -> Vec<u8> {
        self.dtx.iter().map(|q| q.id).collect()
    }
}
