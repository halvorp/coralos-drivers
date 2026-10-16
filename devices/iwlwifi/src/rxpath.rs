// SPDX-License-Identifier: GPL-2.0-only
//! The receive side of the transport: allocate and program the one multi-queue RX queue, keep a fixed
//! pool of 4 KiB receive buffers posted to the card, and turn what the firmware writes into [`Rx`]
//! packets. Ported from Linux `pcie/gen1_2/rx.c` (`iwl_pcie_rx_init` :1167-1190, `_iwl_pcie_rx_init`
//! :1080-1165, `iwl_pcie_rx_alloc` :766-831, `iwl_pcie_rxmq_restock` :245-290, `iwl_pcie_rx_handle`
//! :1498-1625, `iwl_pcie_rx_handle_rb` :1294-1441). The ring arithmetic and the register program are
//! the ring-core's; this file adds the memory and the order. See `iwlwifi-pcie-ring-core::rx` for what is
//! left out of the buffer recycling (the page allocator, emergency mode, `page_stolen`).
use crate::host::Host;
use crate::transport::*;
use alloc::vec;
use alloc::vec::Vec;
use iwlwifi_pcie_trans_core::io::TransIo;
use iwlwifi_pcie_ring_core::rx::*;
use iwlwifi_pcie_ring_core::rxregs::*;

/// The queue size programmed into RFH_RXF_DMA_CFG (`RFH_RXF_DMA_RBDCB_SIZE_512`).
pub const RX_QUEUE_SIZE: u16 = IWL_NUM_RBDS_NON_HE as u16;
/// The commands the op-mode lists as no-reclaim (they are not replies to a command-queue entry).
/// // mvm/ops.c:1203-1205 (`TX_CMD`); fw/api/commands.h:153
pub const NO_RECLAIM_CMDS: [u8; 1] = [0x1c];

impl<H: Host> Transport<H> {
    /// `iwl_pcie_rx_init`: allocate once, reset the indices, program the RFH, post the pool.
    pub(crate) fn rx_init(&mut self) -> Result<(), TransportError> {
        if self.rx.is_none() {
            let bd = self.alloc("rx free-descriptor table", RX_QUEUE_SIZE as usize * FREE_BD_LEN, 4096)?;
            let used = self.alloc("rx used-descriptor table", RX_QUEUE_SIZE as usize * USED_BD_LEN, 4096)?;
            let stts = self.alloc("rx status block", RB_STATUS_LEN, 4096)?;
            let mut pool = Vec::new();
            pool.try_reserve(RX_POOL_BUFS).map_err(|_| TransportError::Dma { what: "rx pool table", why: coralos_hal::DmaRefusal::OutOfMemory { len: RX_POOL_BUFS } })?;
            for _ in 0..RX_POOL_BUFS {
                pool.push(self.alloc("rx buffer", RB_BYTES, 4096)?);
            }
            self.rx = Some(RxRings { q: Rxq::new(RX_QUEUE_SIZE), bd, used, stts, pool, invalid: vec![true; RX_POOL_BUFS] });
        }
        let host = &mut self.nic.bus;
        let r = self.rx.as_mut().expect("allocated above");
        // Set read write pointer to reflect that we have processed and used all buffers, but have not
        // restocked the Rx queue with fresh buffers. // rx.c:1117-1124
        r.q = Rxq::new(RX_QUEUE_SIZE);
        host.dma_write(&r.stts, 0, &[0u8; RB_STATUS_LEN]); // memset(rxq->rb_stts, 0, …), rx.c:1126-1129
        for v in r.invalid.iter_mut() {
            *v = true; // rxb->invalid = true, rx.c:1161
        }
        let dma = RxqDma { bd: r.bd.phys, used_bd: r.used.phys, rb_stts: r.stts.phys };
        rx_mq_hw_init(&mut self.nic, &dma)?; // rx.c:1176
        self.rx_restock()?; // iwl_pcie_rxq_restock, rx.c:1180
        Ok(())
    }

    /// `iwl_pcie_rxmq_restock`: post every buffer the host holds, then tell the card (in eights).
    /// // rx.c:245-290
    pub(crate) fn rx_restock(&mut self) -> Result<(), TransportError> {
        if !self.st.device_enabled {
            return Ok(()); // "the APM has been already stopped", rx.c:255-259
        }
        let r = self.rx.as_mut().ok_or(TransportError::IsDown)?;
        for i in 0..r.pool.len() {
            if !r.invalid[i] {
                continue;
            }
            let slot = r.q.restock_slot(); // rx.c:263-273
            let bd = free_bd(r.pool[i].phys, (i + 1) as u16)?; // rxb->vid = i + 1, rx.c:1160
            self.nic.bus.dma_write(&r.bd, slot as usize * FREE_BD_LEN, &bd.to_le_bytes());
            r.invalid[i] = false;
        }
        if r.q.needs_wr_ptr() {
            let (reg, val) = r.q.wr_ptr_update(0); // rx.c:279-283
            self.nic.csr_write(reg, val);
        }
        Ok(())
    }

    /// `iwl_pcie_rx_handle`: every buffer the card closed since last time. Returns the packets handled.
    /// // rx.c:1498-1625
    pub(crate) fn rx_handle(&mut self) -> Result<u32, TransportError> {
        let mut handled = 0u32;
        let (pending, mut bufs): (u16, Vec<(u16, Vec<u8>)>) = {
            let host = &mut self.nic.bus;
            let r = self.rx.as_mut().ok_or(TransportError::IsDown)?;
            let mut s = [0u8; RB_STATUS_LEN];
            host.dma_read(&r.stts, 0, &mut s);
            let closed = closed_rb(&s)?; // rx.c:1514
            let pending = r.q.pending(closed); // rx.c:1515-1524
            let mut bufs = Vec::new();
            bufs.try_reserve(pending as usize).map_err(|_| TransportError::QueueFull)?;
            for _ in 0..pending {
                let i = r.q.take_read();
                let mut u = [0u8; USED_BD_LEN];
                host.dma_read(&r.used, i as usize * USED_BD_LEN, &mut u);
                let vid = used_bd_vid(u32::from_le_bytes(u)); // rx.c:1473
                if vid == 0 || vid as usize > r.pool.len() || r.invalid[vid as usize - 1] {
                    return Err(TransportError::BadRb { vid }); // "Invalid rxb from HW", rx.c:1476-1485
                }
                r.invalid[vid as usize - 1] = true; // rxb->invalid = true, rx.c:1487
                let mut rb = vec![0u8; RB_BYTES];
                host.dma_read(&r.pool[vid as usize - 1], 0, &mut rb);
                bufs.push((vid, rb));
            }
            (pending, bufs)
        };
        self.stats.rx_buffers += pending as u32;
        for (_vid, rb) in bufs.drain(..) {
            for p in rb_packets(&rb) {
                handled += 1;
                self.stats.rx_packets += 1;
                self.deliver(&p)?;
            }
        }
        self.rx_restock()?; // rx.c:1622
        Ok(handled)
    }

    /// One packet: a reply to a command-queue entry is reclaimed (`iwl_pcie_hcmd_complete`) and handed to
    /// the waiter; anything else is a notification. // rx.c:1364-1409
    fn deliver(&mut self, p: &RxPkt<'_>) -> Result<(), TransportError> {
        let rx = Rx { group: p.group_id, cmd: p.cmd, sequence: p.sequence, payload: p.data.to_vec() };
        if self.inbox.len() >= INBOX_CAP {
            self.inbox.pop_front();
            self.stats.inbox_dropped += 1;
        }
        if reclaim(p, &NO_RECLAIM_CMDS) {
            self.hcmd_complete(p.sequence)?;
            if self.awaiting == Some(p.sequence) {
                self.reply = Some(rx);
            } else {
                self.inbox.push_back(rx);
            }
        } else {
            self.inbox.push_back(rx);
        }
        Ok(())
    }
}
