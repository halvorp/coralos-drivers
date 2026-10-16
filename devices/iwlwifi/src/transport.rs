// SPDX-License-Identifier: GPL-2.0-only
//! The gen1 PCIe transport of the Wireless-AC 9260: power-up, the NIC init (APM, radio config, RX and TX
//! rings), the firmware upload, `fw_alive`, and the state the rest of the driver shares. This file owns
//! the struct and the start-up sequences; [`crate::rxpath`] owns the receive side, [`crate::txpath`]
//! the host-command queue, [`crate::irqloop`] the interrupt service and the waits.
//!
//! Ported from Linux `pcie/gen1_2/trans.c` (`iwl_trans_pcie_start_fw` :1283-1370, `iwl_pcie_nic_init`
//! :485-520, `iwl_trans_pcie_fw_alive` :1372-1376, `iwl_pcie_check_hw_rf_kill` :1010-1030) over the cores
//! that hold each register sequence; this crate adds the ORDER, the memory, and the waits. Linux v7.0-rc6.
//!
//! Copyright (C) 2007-2015, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2015-2017 Intel Deutschland GmbH.
use crate::host::Host;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use coralos_hal::{DmaRefusal, DmaRegion};
use iwlwifi_fw_image_core::sections::Section;
use iwlwifi_pcie_nic_core::bus::{Nic, NicFault, NicIo};
use iwlwifi_pcie_nic_core::irq::{enable_fw_load_int, IrqCtl};
use iwlwifi_pcie_nic_core::load::{load_given_ucode_8000, LoadEnv, LoadError, LoadRefusal, LoadReport, FH_BOUNCE_BYTES};
use iwlwifi_pcie_nic_core::nicinit::{enable_shadow_regs, nic_config};
use iwlwifi_pcie_nic_core::regs::*;
use iwlwifi_pcie_ring_core::rx::{RxError, Rxq};
use iwlwifi_pcie_ring_core::tx::{Cmdq, ReclaimError, TxError};
use iwlwifi_pcie_trans_core::apm::{apm_init_9000, prepare_card_hw, ApmError};
use iwlwifi_pcie_trans_core::hw::{disable_interrupts, enable_rfkill_int, is_rfkill_set, start_hw, stop_device, HwError, StopReport, TransState};
use iwlwifi_pcie_trans_core::io::TransIo;
use iwlwifi_pcie_trans_core::regs::CSR_INT;
use iwlwifi_pcie_trans_core::tx_init::TxInitError;

/// Receive buffers posted to the card: a multiple of 8 (the device's write index moves in eights,
/// rx.c:193), fewer than the 512 descriptors of the table (rx.c:725 `queue_size`); Linux posts
/// `num_rx_bufs - 1` and recycles through a page allocator, this driver posts a fixed pool and reposts each
/// buffer as soon as its packets are copied out. 128 x 4 KiB is 512 KiB of the DMA budget.
pub const RX_POOL_BUFS: usize = 128;
/// The step gate: only silicon steps B and C of the 9000 family are supported. // iwl-drv.c:332-339
pub const SILICON_B_STEP: u32 = 1; // iwl-csr.h:323
pub const SILICON_C_STEP: u32 = 2; // iwl-csr.h:324

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportError {
    /// "Only HW steps B and C are currently supported". // iwl-drv.c:332-339
    UnsupportedStep { hw_rev: u32 },
    Hw(HwError),
    Apm(ApmError),
    Nic(NicFault),
    Dma { what: &'static str, why: DmaRefusal },
    Load(LoadError),
    Tx(TxError),
    TxInit(TxInitError),
    Rx(RxError),
    Reclaim(ReclaimError),
    /// The hardware RF-kill switch is in KILL and the caller did not ask to run in rfkill. // trans.c:1316-1318
    RfKill,
    /// `is_down`: "Can't start_fw since the HW hasn't been started". // trans.c:1322-1327
    IsDown,
    /// A bounded wait ran out.
    Timeout { what: &'static str },
    /// The firmware or the hardware reported an error through the interrupt.
    Fault(Fault),
    /// The link reads all-ones: "HARDWARE GONE??". // rx.c:1943-1950
    HardwareGone { inta: u32 },
    /// "No space in command queue". // tx.c:1446-1455
    QueueFull,
    /// A used descriptor named a buffer that is not in the pool, or one the host already holds.
    BadRb { vid: u16 },
}

impl TransportError {
    pub fn name(&self) -> &'static str {
        match self {
            TransportError::UnsupportedStep { .. } => "IWL-STEP",
            TransportError::Hw(_) => "IWL-HW",
            TransportError::Apm(_) => "IWL-APM",
            TransportError::Nic(_) => "IWL-NIC-ACCESS",
            TransportError::Dma { .. } => "IWL-DMA",
            TransportError::Load(_) => "IWL-FW-LOAD",
            TransportError::Tx(_) | TransportError::TxInit(_) | TransportError::QueueFull => "IWL-TX",
            TransportError::Rx(_) | TransportError::BadRb { .. } => "IWL-RX",
            TransportError::Reclaim(_) => "IWL-RECLAIM",
            TransportError::RfKill => "IWL-RFKILL",
            TransportError::IsDown => "IWL-IS-DOWN",
            TransportError::Timeout { .. } => "IWL-TIMEOUT",
            TransportError::Fault(_) => "IWL-FAULT",
            TransportError::HardwareGone { .. } => "IWL-HW-GONE",
        }
    }

    /// The opaque errno-shaped integer the MVM init machine records (`mvm/fw.c:316-322`).
    pub fn errno(&self) -> i32 {
        match self {
            TransportError::RfKill => -132, // ERFKILL
            TransportError::Timeout { .. } => -110, // ETIMEDOUT
            TransportError::Dma { .. } => -12, // ENOMEM
            TransportError::QueueFull => -28, // ENOSPC
            TransportError::IsDown => -5,
            _ => -5, // EIO
        }
    }
}

macro_rules! from_err {
    ($($t:ty => $v:ident),*) => { $(impl From<$t> for TransportError { fn from(e: $t) -> Self { TransportError::$v(e) } })* };
}
from_err!(HwError => Hw, ApmError => Apm, NicFault => Nic, LoadError => Load, TxError => Tx, TxInitError => TxInit, RxError => Rx, ReclaimError => Reclaim, Fault => Fault);

/// An error the card reported by interrupt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// `CSR_INT_BIT_HW_ERR`: "Hardware error detected". // rx.c:1973-1986
    HwError { inta: u32 },
    /// `CSR_INT_BIT_SW_ERR`: "Microcode SW error detected". // rx.c:2031-2043
    SwError { inta: u32 },
}

/// One packet the firmware sent: a command reply or a notification. `payload` is after the 8-byte header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rx {
    pub group: u8,
    pub cmd: u8,
    pub sequence: u16,
    pub payload: Vec<u8>,
}

impl Rx {
    pub fn is_notification(&self) -> bool {
        self.sequence & 0x8000 != 0
    }
}

pub struct RxRings {
    pub q: Rxq,
    pub bd: DmaRegion,
    pub used: DmaRegion,
    pub stts: DmaRegion,
    pub pool: Vec<DmaRegion>,
    /// `rxb->invalid`: true while the host holds the buffer (not posted to the card). // rx.c:1161, :1478
    pub invalid: Vec<bool>,
}

pub struct TxRings {
    pub cmdq: Cmdq,
    pub ring: DmaRegion,
    pub first_tb: DmaRegion,
    pub slots: DmaRegion,
    pub spare: DmaRegion,
    pub bc: DmaRegion,
    pub kw: DmaRegion,
}

/// Counters the log line and the tests read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub irqs: u32,
    pub rx_buffers: u32,
    pub rx_packets: u32,
    pub cmds_sent: u32,
    pub spurious: u32,
}

pub struct Transport<H: Host> {
    pub nic: Nic<H>,
    pub st: TransState,
    pub ctl: IrqCtl,
    pub hw_rev: u32,
    pub rfkill_hw: bool,
    pub fault: Option<Fault>,
    pub inbox: VecDeque<Rx>,
    pub stats: Stats,
    pub scd_base: u32,
    pub(crate) rx: Option<RxRings>,
    pub(crate) tx: Option<TxRings>,
    pub(crate) bounce: Option<DmaRegion>,
    pub(crate) nocopy: Option<DmaRegion>,
    pub(crate) write_complete: bool,
    pub(crate) awaiting: Option<u16>,
    pub(crate) reply: Option<Rx>,
}

impl<H: Host> Transport<H> {
    pub fn new(host: H) -> Self {
        Transport {
            nic: Nic::new(host),
            st: TransState::default(),
            ctl: IrqCtl::default(),
            hw_rev: 0,
            rfkill_hw: false,
            fault: None,
            inbox: VecDeque::new(),
            stats: Stats::default(),
            scd_base: 0,
            rx: None,
            tx: None,
            bounce: None,
            nocopy: None,
            write_complete: false,
            awaiting: None,
            reply: None,
        }
    }

    pub fn host(&mut self) -> &mut H {
        &mut self.nic.bus
    }

    pub(crate) fn alloc(&mut self, what: &'static str, len: usize, align: usize) -> Result<DmaRegion, TransportError> {
        self.nic.bus.dma_alloc(len, align).map_err(|why| TransportError::Dma { what, why })
    }

    /// `_iwl_trans_pcie_start_hw` for the 9000 family, behind the silicon step gate. Returns whether the
    /// hardware RF-kill switch is in KILL. // trans.c:1786-1832; iwl-drv.c:332-339
    pub fn start_hw(&mut self) -> Result<bool, TransportError> {
        self.hw_rev = self.nic.csr_read(CSR_HW_REV);
        let step = self.hw_rev & 0xF; // trans.c:3892 (family >= 8000)
        if step != SILICON_B_STEP && step != SILICON_C_STEP {
            return Err(TransportError::UnsupportedStep { hw_rev: self.hw_rev });
        }
        self.fault = None;
        let kill = start_hw(&mut self.nic, &mut self.st)?;
        self.rfkill_hw = kill;
        Ok(kill)
    }

    /// `_iwl_trans_pcie_stop_device`. // trans.c:1184-1267
    pub fn stop_device(&mut self) -> StopReport {
        let r = stop_device(&mut self.nic, &mut self.st);
        self.ctl.int_enabled = false;
        self.write_complete = false;
        self.awaiting = None;
        self.reply = None;
        r
    }

    /// `iwl_pcie_check_hw_rf_kill`: read the switch, remember it. // trans.c:1010-1030
    pub fn check_hw_rf_kill(&mut self) -> bool {
        self.rfkill_hw = is_rfkill_set(&mut self.nic);
        self.rfkill_hw
    }

    /// `iwl_pcie_nic_init`. // trans.c:485-520
    fn nic_init(&mut self, phy_config: u32) -> Result<(), TransportError> {
        apm_init_9000(&mut self.nic)?; // trans.c:491-495
        // iwl_pcie_set_pwr(trans, false): apmg_not_supported, a no-op (trans.c:187-188)
        nic_config(&mut self.nic, self.hw_rev, phy_config); // iwl_op_mode_nic_config, trans.c:501
        self.rx_init()?; // trans.c:504-506
        self.tx_init()?; // trans.c:509-513
        enable_shadow_regs(&mut self.nic); // shadow_reg_enable, trans.c:513-517
        Ok(())
    }

    /// `iwl_trans_pcie_start_fw` for the 9000 family: everything up to and including the section upload.
    /// The caller then waits for ALIVE. // trans.c:1283-1370
    pub fn start_fw(&mut self, secs: &[Section<'_>], phy_config: u32, run_in_rfkill: bool) -> Result<LoadReport, TransportError> {
        // This may fail if AMT took ownership of the device. // trans.c:1294-1298
        prepare_card_hw(&mut self.nic)?;
        enable_rfkill_int(&mut self.nic, &mut self.st); // trans.c:1300
        self.nic.csr_write(CSR_INT, 0xFFFF_FFFF); // trans.c:1302
        // We enabled the RF-Kill interrupt and the handler may very well be running. Disable the
        // interrupts to make sure no other interrupt can be fired. // trans.c:1304-1310
        self.ctl.disable_all(&mut self.nic, &mut self.st);
        // If platform's RF_KILL switch is NOT set to KILL // trans.c:1315-1319
        let hw_rfkill = self.check_hw_rf_kill();
        if hw_rfkill && !run_in_rfkill {
            return Err(TransportError::RfKill);
        }
        // Someone called stop_device, don't try to start_fw // trans.c:1322-1327
        if self.st.is_down {
            return Err(TransportError::IsDown);
        }
        // make sure rfkill handshake bits are cleared // trans.c:1330-1333
        self.nic.csr_write(CSR_UCODE_DRV_GP1_CLR, CSR_UCODE_SW_BIT_RFKILL);
        self.nic.csr_write(CSR_UCODE_DRV_GP1_CLR, CSR_UCODE_DRV_GP1_BIT_CMD_BLOCKED);
        // clear (again), then enable host interrupts // trans.c:1336
        self.nic.csr_write(CSR_INT, 0xFFFF_FFFF);
        self.nic_init(phy_config)?; // trans.c:1338-1342
        // Now, we load the firmware and don't want to be interrupted, even by the RF-Kill interrupt:
        // mask all the interrupts besides FH_TX, which the upload needs. // trans.c:1344-1350
        enable_fw_load_int(&mut self.nic, &mut self.st);
        // really make sure rfkill handshake bits are cleared // trans.c:1352-1354
        self.nic.csr_write(CSR_UCODE_DRV_GP1_CLR, CSR_UCODE_SW_BIT_RFKILL);
        self.nic.csr_write(CSR_UCODE_DRV_GP1_CLR, CSR_UCODE_SW_BIT_RFKILL);
        // Load the given image to the HW. // trans.c:1356-1361 (family >= 8000)
        let loaded = load_given_ucode_8000(self, secs);
        // re-check RF-Kill state since we may have missed the interrupt // trans.c:1363-1365
        let hw_rfkill = self.check_hw_rf_kill();
        if hw_rfkill && !run_in_rfkill {
            return Err(TransportError::RfKill);
        }
        Ok(loaded?)
    }

    /// `iwl_trans_pcie_fw_alive`: the firmware is up, start the TX scheduler and the command queue.
    /// `iwl_pcie_reset_ict` is not run: the ICT is not used (nic-core irq.rs). // trans.c:1372-1376
    pub fn fw_alive(&mut self) -> Result<u32, TransportError> {
        let bc = self.tx.as_ref().map(|t| t.bc.phys).ok_or(TransportError::IsDown)?;
        let base = iwlwifi_pcie_ring_core::tx::tx_start(&mut self.nic, bc)?;
        self.scd_base = base;
        Ok(base)
    }

    pub fn disable_interrupts(&mut self) {
        self.ctl.int_enabled = false;
        disable_interrupts(&mut self.nic, &mut self.st);
    }
}

// The cores' sequences take `&mut impl TransIo / NicIo`; the transport is the NIC.
impl<H: Host> TransIo for Transport<H> {
    fn csr_read(&mut self, reg: u32) -> u32 {
        self.nic.csr_read(reg)
    }
    fn csr_write(&mut self, reg: u32, val: u32) {
        self.nic.csr_write(reg, val)
    }
    fn prph_read(&mut self, addr: u32) -> u32 {
        self.nic.prph_read(addr)
    }
    fn prph_write(&mut self, addr: u32, val: u32) {
        self.nic.prph_write(addr, val)
    }
    fn sleep_us(&mut self, us: u32) {
        self.nic.sleep_us(us)
    }
}

impl<H: Host> NicIo for Transport<H> {
    fn grab(&mut self) -> bool {
        self.nic.grab()
    }
    fn release(&mut self) {
        self.nic.release()
    }
    fn read_direct32(&mut self, reg: u32) -> u32 {
        self.nic.read_direct32(reg)
    }
    fn write_direct32(&mut self, reg: u32, val: u32) {
        self.nic.write_direct32(reg, val)
    }
    fn write8(&mut self, reg: u32, val: u8) {
        NicIo::write8(&mut self.nic, reg, val)
    }
    fn write_prph64_no_grab(&mut self, addr: u32, val: u64) {
        self.nic.write_prph64_no_grab(addr, val)
    }
    fn write_mem(&mut self, addr: u32, words: &[u32]) -> Result<(), NicFault> {
        self.nic.write_mem(addr, words)
    }
    fn zero_mem(&mut self, addr: u32, dwords: usize) -> Result<(), NicFault> {
        self.nic.zero_mem(addr, dwords)
    }
    fn read_mem(&mut self, addr: u32, out: &mut [u32]) -> Result<(), NicFault> {
        self.nic.read_mem(addr, out)
    }
    fn fault(&self) -> Option<NicFault> {
        self.nic.fault()
    }
}

impl<H: Host> LoadEnv for Transport<H> {
    /// One bounce buffer, allocated once and reused (CoralOS has no DMA free). `ucode_write_complete` is
    /// cleared here, before the chunk's registers are written. // trans.c:638
    fn stage(&mut self, chunk: &[u8]) -> Result<u64, LoadRefusal> {
        self.write_complete = false;
        let region = match self.bounce {
            Some(r) => r,
            None => {
                let r = self.nic.bus.dma_alloc(FH_BOUNCE_BYTES, 4096).map_err(|_| LoadRefusal::NoBounceBuffer)?;
                self.bounce = Some(r);
                r
            }
        };
        self.nic.bus.dma_write(&region, 0, chunk);
        Ok(region.phys)
    }

    fn wait_chunk_done(&mut self, timeout_ms: u32) -> bool {
        let deadline = self.nic.bus.now_ms() + timeout_ms as u64;
        loop {
            if self.write_complete {
                return true;
            }
            if self.fault.is_some() {
                return false;
            }
            match self.nic.bus.wait_irq(deadline) {
                crate::host::Wake::Deadline => return self.write_complete,
                crate::host::Wake::Irq => {
                    let _ = self.service_irq();
                }
            }
        }
    }

    fn enable_interrupts(&mut self) {
        self.ctl.enable_all(&mut self.nic, &mut self.st); // iwl_enable_interrupts, trans.c:764
    }
}
