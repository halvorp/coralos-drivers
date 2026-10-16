// SPDX-License-Identifier: GPL-2.0-only
//! The simulated Atlas Wi-Fi: one `World` that is a Wireless-AC 9260 (CSR block, HBUS indirection, SRAM, the
//! flow handler's service channel, the RFH receive queue, the TX scheduler and the command queue's TFD
//! fetch), a firmware that behaves as the INIT and REGULAR images do, host DMA memory, and a virtual clock.
//! Time moves only when the driver sleeps or waits: a wait runs the scheduled device events up to the next
//! thing that could wake it, so a test sees one deterministic interleaving.
//!
//! The hardware side is written from the Linux register definitions as LITERALS (not through the driver's
//! constants), so a wrong constant in the driver is a wrong register in the sim. What it enforces, as the
//! silicon does: periphery, SRAM and flow-handler accesses need the MAC awake (MAC_ACCESS_REQ held); an
//! upload chunk aimed at 0x40000..=0x57FFF needs LMPM_CHICK's extended-address bit; the firmware boots
//! only when every section of ONE image is in SRAM at the right addresses and the load-status ladder was
//! written; a notification needs a posted receive buffer. A violation is RECORDED (`violations`), never
//! silently tolerated, and the tests assert the list is empty.
#![allow(dead_code)]
pub mod blob;
pub mod fwmodel;
pub mod nvmdata;
pub mod regs;

use coralos_hal::{DmaRefusal, DmaRegion};
use iwlwifi_device::host::{Host, Wake};
use iwlwifi_pcie_nic_core::bus::RawBus;
use std::cell::RefCell;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::cmp::Reverse;
use std::rc::Rc;

pub use fwmodel::{FwImageModel, SecModel};

pub struct Region {
    pub phys: u64,
    pub data: Vec<u8>,
}

pub enum Ev {
    /// The service channel finished a chunk: raise FH_TX.
    ChunkDone,
    /// The firmware, released from reset and loaded, announces itself.
    Boot,
    /// Post a packet into the RX queue (cmd, group, seq, payload).
    Post { group: u8, cmd: u8, seq: u16, payload: Vec<u8> },
    /// Anything a test wants to schedule.
    Custom(Box<dyn FnOnce(&mut World)>),
}

pub struct World {
    pub now_us: u64,
    evs: BinaryHeap<Reverse<(u64, u64)>>,
    ev_data: HashMap<u64, Ev>,
    seq: u64,
    // ---- BAR0 ----
    pub csr: HashMap<u32, u32>,
    pub inta: u32,
    pub int_mask: u32,
    pub fh_int: u32,
    pub gp_cntrl: u32,
    pub irq_pending: bool,
    pub prph: HashMap<u32, u32>,
    pub prph_raddr: u32,
    pub prph_waddr: u32,
    pub mem_raddr: u32,
    pub mem_waddr: u32,
    pub sram: BTreeMap<u32, u8>,
    // ---- flow handler ----
    pub fh: HashMap<u32, u32>,
    pub srvc_chunks: Vec<(u32, u64, u32)>,
    pub load_status_history: Vec<u32>,
    // ---- RX queue (device side) ----
    pub rx_free_ridx: u16,
    pub rx_free_widx: u16,
    pub rx_used_widx: u16,
    pub rx_overruns: u32,
    // ---- command queue (device side) ----
    pub cmd_seen_wptr: u16,
    pub cmds: Vec<SeenCmd>,
    // ---- firmware ----
    pub fw: fwmodel::Fw,
    // ---- host memory ----
    pub regions: Vec<Region>,
    pub next_phys: u64,
    pub dma_quota_bytes: usize,
    pub dma_used: usize,
    // ---- bookkeeping the tests read ----
    pub violations: Vec<String>,
    pub writes: Vec<(u64, &'static str, u32, u32)>,
    pub log: Vec<String>,
    pub forced_nmi: u32,
    /// Fault injection: MAC_ACCESS_REQ never produces MAC_CLOCK_READY (the card does not wake).
    pub wedge_mac_access: bool,
    /// The RF-kill switch (a pin, not card state): true = KILL, GP_CNTRL bit 27 reads clear.
    pub hw_rfkill: bool,
    pub wait_log: Vec<(u64, Option<u64>, bool)>,
}

/// A host command as the card saw it.
#[derive(Clone, Debug)]
pub struct SeenCmd {
    pub group: u8,
    pub cmd: u8,
    pub seq: u16,
    pub payload: Vec<u8>,
    pub tbs: Vec<(u64, u16)>,
}

#[derive(Clone)]
pub struct Sim(pub Rc<RefCell<World>>);

impl Sim {
    pub fn new() -> Sim {
        let mut w = World {
            now_us: 0,
            evs: BinaryHeap::new(),
            ev_data: HashMap::new(),
            seq: 0,
            csr: HashMap::new(),
            inta: 0,
            int_mask: 0,
            fh_int: 0,
            gp_cntrl: regs::GP_CNTRL_RESET_VALUE,
            irq_pending: false,
            prph: HashMap::new(),
            prph_raddr: 0,
            prph_waddr: 0,
            mem_raddr: 0,
            mem_waddr: 0,
            sram: BTreeMap::new(),
            fh: HashMap::new(),
            srvc_chunks: Vec::new(),
            load_status_history: Vec::new(),
            rx_free_ridx: 0,
            rx_free_widx: 0,
            rx_used_widx: 0,
            rx_overruns: 0,
            cmd_seen_wptr: 0,
            cmds: Vec::new(),
            fw: fwmodel::Fw::new(),
            regions: Vec::new(),
            next_phys: 0x2_0000_0000, // above 4 GiB: the hi address bits are exercised
            dma_quota_bytes: 1024 * 4096,
            dma_used: 0,
            violations: Vec::new(),
            writes: Vec::new(),
            log: Vec::new(),
            forced_nmi: 0,
            wedge_mac_access: false,
            hw_rfkill: false,
            wait_log: Vec::new(),
        };
        w.csr.insert(regs::CSR_HW_REV, 0x0000_0321);
        w.csr.insert(regs::CSR_HW_RF_ID, 0x0010_5100);
        w.csr.insert(regs::CSR_MAC_ADDR0_STRAP, 0);
        w.csr.insert(regs::CSR_MAC_ADDR1_STRAP, 0);
        w.csr.insert(regs::CSR_MAC_ADDR0_OTP, 0x1222_3344); // iwl_flip_hw_address: dest[0..4] = bytes 3,2,1,0 -> 12:22:33:44 (bit 0 of byte 0 would be multicast)
        w.csr.insert(regs::CSR_MAC_ADDR1_OTP, 0x0000_5566); // ... dest[4], dest[5] = bytes 1, 0 -> 55:66
        Sim(Rc::new(RefCell::new(w)))
    }

    pub fn w(&self) -> std::cell::RefMut<'_, World> {
        self.0.borrow_mut()
    }

    pub fn host(&self) -> SimHost {
        SimHost(self.clone())
    }
}

pub struct SimHost(pub Sim);

impl World {
    pub fn schedule(&mut self, after_us: u64, ev: Ev) {
        self.seq += 1;
        self.evs.push(Reverse((self.now_us + after_us, self.seq)));
        self.ev_data.insert(self.seq, ev);
    }

    pub fn violation(&mut self, what: String) {
        self.violations.push(what);
    }

    /// Run every event due at or before `t_us`, in order; the clock stops at each event's time.
    pub fn run_until(&mut self, t_us: u64) {
        while let Some(&Reverse((due, id))) = self.evs.peek() {
            if due > t_us {
                break;
            }
            self.evs.pop();
            self.now_us = self.now_us.max(due);
            let ev = self.ev_data.remove(&id).unwrap();
            self.run_ev(ev);
        }
        self.now_us = self.now_us.max(t_us);
    }

    fn run_ev(&mut self, ev: Ev) {
        match ev {
            Ev::ChunkDone => {
                self.fh_int |= 0x3;
                self.raise(regs::INT_FH_TX);
            }
            Ev::Boot => fwmodel::boot(self),
            Ev::Post { group, cmd, seq, payload } => fwmodel::post(self, group, cmd, seq, &payload),
            Ev::Custom(f) => f(self),
        }
    }

    /// Raise interrupt cause bits; the MSI fires if any is unmasked (edge: only on a new unmasked cause).
    pub fn raise(&mut self, bits: u32) {
        let new = bits & !self.inta;
        self.inta |= bits;
        if new & self.int_mask != 0 {
            self.irq_pending = true;
        }
    }

    pub fn region_of(&self, phys: u64, len: usize) -> Option<usize> {
        self.regions.iter().position(|r| phys >= r.phys && phys + len as u64 <= r.phys + r.data.len() as u64)
    }

    pub fn dma_read(&self, phys: u64, out: &mut [u8]) -> bool {
        match self.region_of(phys, out.len()) {
            Some(i) => {
                let r = &self.regions[i];
                let o = (phys - r.phys) as usize;
                out.copy_from_slice(&r.data[o..o + out.len()]);
                true
            }
            None => false,
        }
    }

    pub fn dma_write(&mut self, phys: u64, data: &[u8]) -> bool {
        match self.region_of(phys, data.len()) {
            Some(i) => {
                let r = &mut self.regions[i];
                let o = (phys - r.phys) as usize;
                r.data[o..o + data.len()].copy_from_slice(data);
                true
            }
            None => false,
        }
    }

    pub fn sram_read(&self, addr: u32, out: &mut [u8]) {
        for (i, b) in out.iter_mut().enumerate() {
            *b = self.sram.get(&(addr + i as u32)).copied().unwrap_or(0);
        }
    }

    pub fn sram_write(&mut self, addr: u32, data: &[u8]) {
        for (i, b) in data.iter().enumerate() {
            self.sram.insert(addr + i as u32, *b);
        }
    }

    /// Whether the MAC is awake: MAC_ACCESS_REQ held AND the clock running (INIT_DONE, not wedged).
    pub fn awake(&self) -> bool {
        self.gp_cntrl & regs::GP_MAC_ACCESS_REQ != 0 && self.gp_cntrl & regs::GP_INIT_DONE != 0 && !self.wedge_mac_access
    }
}

impl RawBus for SimHost {
    fn read32(&mut self, off: u32) -> u32 {
        regs::read32(&mut self.0.w(), off)
    }
    fn write32(&mut self, off: u32, val: u32) {
        regs::write32(&mut self.0.w(), off, val)
    }
    fn write8(&mut self, off: u32, val: u8) {
        regs::write8(&mut self.0.w(), off, val)
    }
    fn sleep_us(&mut self, us: u32) {
        let mut w = self.0.w();
        let t = w.now_us + us as u64;
        w.run_until(t);
    }
}

impl Host for SimHost {
    fn dma_alloc(&mut self, len: usize, align: usize) -> Result<DmaRegion, DmaRefusal> {
        let mut w = self.0.w();
        let pages = len.div_ceil(4096) * 4096;
        if w.dma_used + pages > w.dma_quota_bytes {
            return Err(DmaRefusal::OverQuota { len });
        }
        w.dma_used += pages;
        let a = align.max(1) as u64;
        let phys = w.next_phys.div_ceil(a) * a;
        w.next_phys = phys + pages as u64;
        w.regions.push(Region { phys, data: vec![0u8; len] });
        Ok(DmaRegion { virt: phys + 0x7000_0000_0000, phys, len })
    }

    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]) {
        assert!(off + data.len() <= region.len, "host DMA write past its region");
        assert!(self.0.w().dma_write(region.phys + off as u64, data));
    }

    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]) {
        assert!(off + out.len() <= region.len, "host DMA read past its region");
        assert!(self.0.w().dma_read(region.phys + off as u64, out));
    }

    fn now_ms(&mut self) -> u64 {
        self.0.w().now_us / 1000
    }

    fn wait_irq(&mut self, deadline_ms: u64) -> Wake {
        let mut w = self.0.w();
        let start = w.now_us;
        let deadline_us = deadline_ms * 1000;
        loop {
            if w.irq_pending {
                w.irq_pending = false;
                let t = w.now_us;
                w.wait_log.push((start, Some(t), true));
                return Wake::Irq;
            }
            let next = w.evs.peek().map(|Reverse((due, _))| *due);
            match next {
                Some(due) if due <= deadline_us => w.run_until(due),
                _ => {
                    let t = deadline_us.max(w.now_us);
                    w.run_until(t);
                    if w.irq_pending {
                        continue;
                    }
                    let t = w.now_us;
                    w.wait_log.push((start, Some(t), false));
                    return Wake::Deadline;
                }
            }
        }
    }

    fn log(&mut self, line: &str) {
        self.0.w().log.push(line.to_string());
    }
}
