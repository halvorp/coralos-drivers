// SPDX-License-Identifier: GPL-2.0-only
//! The transmit side as far as bring-up needs it: allocate the TX structures, program the 31 queue
//! bases, and send host commands on queue 0. Ported from Linux `pcie/gen1_2/tx.c` (`iwl_pcie_tx_alloc`
//! :786-856, `iwl_pcie_tx_init` :938-996, `iwl_pcie_enqueue_hcmd` :1343-1612, `iwl_pcie_hcmd_complete`
//! :1621-1680, `iwl_trans_pcie_send_hcmd_sync` :2550-2644). The register programs are the cores'.
//!
//! DEVIATION, named. Linux allocates a 256-entry TFD ring (32 KiB) and a first-TB buffer array for each
//! of the 31 queues. Queues 1..30 carry no data until a station exists, and the card reads a queue's ring
//! only after the host moves that queue's write pointer; so every queue but the command queue points at ONE
//! shared, all-zero spare ring (`TxRings::spare`). That saves ~1 MiB of the DMA budget. The data-queue port
//! allocates a ring per queue when it enables the queue; until then nothing can reach the spare ring.
use crate::host::{Host, Wake};
use crate::transport::*;
use iwlwifi_pcie_nic_core::regs::{CSR_RESET_REG_FLAG_FORCE_NMI, HBUS_TARG_WRPTR};
use iwlwifi_pcie_ring_core::tx::{plan_hcmd, tfd_add_tb, Chunk, Cmdq};
use iwlwifi_pcie_ring_core::txregs::*;
use iwlwifi_pcie_trans_core::io::TransIo;
use iwlwifi_pcie_trans_core::regs::{CSR_RESET, IWL_KW_SIZE};
use iwlwifi_pcie_trans_core::tx_init::tx_init;

/// `TFD_QUEUE_SIZE_MAX + TFD_QUEUE_SIZE_BC_DUP`: the byte-count table has this many entries per queue.
/// // iwl-fh.h:590-594
pub const TFD_QUEUE_BC_SIZE: usize = 256 + 64;
/// `sizeof(struct iwl_bc_tbl_entry)`. // iwl-fh.h:719-725
pub const BC_ENTRY_BYTES: usize = 2;

impl<H: Host> Transport<H> {
    /// `iwl_pcie_tx_alloc` (once) + `iwl_pcie_tx_init`. // tx.c:786-856, :938-996
    pub(crate) fn tx_init(&mut self) -> Result<(), TransportError> {
        if self.tx.is_none() {
            let bc = self.alloc("tx byte-count tables", IWL9000_NUM_OF_QUEUES as usize * TFD_QUEUE_BC_SIZE * BC_ENTRY_BYTES, 4096)?; // tx.c:798-803
            let kw = self.alloc("keep-warm buffer", IWL_KW_SIZE as usize, 4096)?; // tx.c:812-817
            let ring = self.alloc("command queue TFD ring", MAX_TFD_QUEUE_SIZE as usize * TFD_SIZE, 4096)?; // tx.c:821-856
            let spare = self.alloc("spare TFD ring", MAX_TFD_QUEUE_SIZE as usize * TFD_SIZE, 4096)?;
            let first_tb = self.alloc("command first-TB buffers", IWL_CMD_QUEUE_SIZE as usize * IWL_FIRST_TB_SIZE_ALIGN, 4096)?; // tx.c:762-767
            let slots = self.alloc("command slots", IWL_CMD_QUEUE_SIZE as usize * TFD_MAX_PAYLOAD_SIZE, 4096)?; // tx.c:749-752
            self.tx = Some(TxRings { cmdq: Cmdq::new(), ring, first_tb, slots, spare, bc, kw });
        }
        let t = self.tx.as_mut().expect("allocated above");
        t.cmdq = Cmdq::new(); // iwl_queue_init: both pointers at 0, tx.c:880-900
        let mut rings = [t.spare.phys; IWL9000_NUM_OF_QUEUES as usize];
        rings[IWL_MVM_DQA_CMD_QUEUE as usize] = t.ring.phys;
        tx_init(&mut self.nic, t.kw.phys, &rings)?;
        Ok(())
    }

    /// Send one command and wait for the firmware's reply (`CMD_WANT_SKB` is implicit: the payload is returned).
    /// // iwl_trans_pcie_send_hcmd_sync, tx.c:2550-2644
    pub fn send_cmd(&mut self, group: u8, opcode: u8, version: u8, chunks: &[Chunk<'_>]) -> Result<Rx, TransportError> {
        self.send_cmd_flags(group, opcode, version, chunks, false)
    }

    /// `CMD_SEND_IN_RFKILL`: the NVM reads set it, so the card can be read with the radio killed.
    /// Without it a command is dropped while the RF-kill switch is in KILL. // tx.c:2655-2660
    pub fn send_cmd_in_rfkill(&mut self, group: u8, opcode: u8, version: u8, chunks: &[Chunk<'_>]) -> Result<Rx, TransportError> {
        self.send_cmd_flags(group, opcode, version, chunks, true)
    }

    fn send_cmd_flags(&mut self, group: u8, opcode: u8, version: u8, chunks: &[Chunk<'_>], in_rfkill: bool) -> Result<Rx, TransportError> {
        if self.rfkill_hw && !in_rfkill {
            return Err(TransportError::RfKill); // "Dropping CMD 0x%x: RF KILL", tx.c:2655-2660
        }
        if let Some(f) = self.fault {
            return Err(TransportError::Fault(f));
        }
        let seq = self.enqueue_hcmd(group, opcode, version, chunks)?;
        self.awaiting = Some(seq);
        self.reply = None;
        let deadline = self.nic.bus.now_ms() + HOST_COMPLETE_TIMEOUT_MS as u64; // tx.c:2581-2584
        loop {
            if let Some(r) = self.reply.take() {
                self.awaiting = None;
                return Ok(r);
            }
            if let Some(f) = self.fault {
                self.awaiting = None;
                return Err(TransportError::Fault(f));
            }
            match self.nic.bus.wait_irq(deadline) {
                Wake::Irq => {
                    if let Err(e) = self.service_irq() {
                        self.awaiting = None;
                        return Err(e);
                    }
                }
                Wake::Deadline => {
                    // "Error sending %s: time out after %dms." then iwl_trans_pcie_sync_nmi. // tx.c:2585-2598
                    self.awaiting = None;
                    self.nic.csr_write(CSR_RESET, CSR_RESET_REG_FLAG_FORCE_NMI);
                    return Err(TransportError::Timeout { what: "host command" });
                }
            }
        }
    }

    /// `iwl_pcie_enqueue_hcmd`: build the command in its slot, build its TFD, ring the doorbell. Returns
    /// the sequence number the reply will carry. // tx.c:1343-1612
    fn enqueue_hcmd(&mut self, group: u8, opcode: u8, version: u8, chunks: &[Chunk<'_>]) -> Result<u16, TransportError> {
        // Stage the NOCOPY chunks first: they need DMA memory of the caller's bytes. // tx.c:1561-1580 (dma_map_single)
        let mut staged = [(0u64, 0usize); IWL_MAX_CMD_TBS_PER_TFD];
        let mut need = 0usize;
        for c in chunks.iter().filter(|c| c.nocopy) {
            need += (c.data.len() + 63) & !63;
        }
        if need > 0 {
            let region = match self.nocopy {
                Some(r) if r.len >= need => r,
                _ => {
                    let r = self.alloc("nocopy command buffer", need.next_power_of_two().max(4096), 4096)?;
                    self.nocopy = Some(r);
                    r
                }
            };
            let mut off = 0usize;
            for (i, c) in chunks.iter().enumerate().filter(|(_, c)| c.nocopy) {
                self.nic.bus.dma_write(&region, off, c.data);
                staged[i] = (region.phys + off as u64, off);
                off += (c.data.len() + 63) & !63;
            }
        }
        let host = &mut self.nic.bus;
        let t = self.tx.as_mut().ok_or(TransportError::IsDown)?;
        if t.cmdq.space() < 1 {
            return Err(TransportError::QueueFull); // "No space in command queue", tx.c:1446-1455
        }
        let write_ptr = t.cmdq.write_ptr;
        let idx = t.cmdq.cmd_index(write_ptr) as usize; // tx.c:1458
        let seq = t.cmdq.sequence();
        let mut slot = [0u8; TFD_MAX_PAYLOAD_SIZE];
        let plan = plan_hcmd(group, opcode, version, seq, chunks, &mut slot)?;
        let slot_off = idx * TFD_MAX_PAYLOAD_SIZE;
        host.dma_write(&t.slots, slot_off, &slot[..plan.copy_size]);
        host.dma_write(&t.first_tb, idx * IWL_FIRST_TB_SIZE_ALIGN, &slot[..plan.tb0_len]); // tx.c:1538-1539
        // iwl_pcie_txq_build_tfd for TB0, TB1 and the NOCOPY chunks. // tx.c:1540-1580
        let mut tfd = [0u8; TFD_SIZE];
        tfd_add_tb(&mut tfd, t.first_tb.phys + (idx * IWL_FIRST_TB_SIZE_ALIGN) as u64, plan.tb0_len as u16, true)?;
        if plan.tb1_len > 0 {
            tfd_add_tb(&mut tfd, t.slots.phys + (slot_off + plan.tb0_len) as u64, plan.tb1_len as u16, false)?;
        }
        for nc in plan.nocopy.iter().flatten() {
            let (ci, skip, len) = *nc;
            tfd_add_tb(&mut tfd, staged[ci].0 + skip as u64, len as u16, false)?;
        }
        host.dma_write(&t.ring, write_ptr as usize * TFD_SIZE, &tfd); // the TFD sits at write_ptr, tx.c:170
        let (reg, val) = t.cmdq.advance(); // tx.c:1606
        debug_assert_eq!(reg, HBUS_TARG_WRPTR);
        self.nic.csr_write(reg, val); // iwl_pcie_txq_inc_wr_ptr, tx.c:1607, :137
        self.stats.cmds_sent += 1;
        Ok(seq)
    }

    /// `iwl_pcie_hcmd_complete`: the reply to command-queue entry `index` arrived. // tx.c:1621-1680
    pub(crate) fn hcmd_complete(&mut self, sequence: u16) -> Result<(), TransportError> {
        let txq_id = ((sequence >> 8) & 0x1f) as u32; // SEQ_TO_QUEUE, fw/api/cmdhdr.h:18
        let index = sequence & 0xff; // SEQ_TO_INDEX
        if txq_id != IWL_MVM_DQA_CMD_QUEUE {
            self.stats.spurious += 1; // "wrong command queue %d (should be %d)", tx.c:1635-1639
            return Ok(());
        }
        let t = self.tx.as_mut().ok_or(TransportError::IsDown)?;
        t.cmdq.reclaim(index)?; // iwl_pcie_cmdq_reclaim, tx.c:1671
        Ok(())
    }
}
